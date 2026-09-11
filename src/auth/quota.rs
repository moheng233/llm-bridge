//! 配额服务（Phase 2.2）。
//!
//! 管理每个 Token 的用量追踪与配额检查：
//! - 按周期（daily / monthly / unlimited）创建 UsageRecord
//! - 原子性检查配额 + 扣减用量
//! - 配额超额时返回明确错误信息

use jiff::Zoned;
use tracing::warn;

use crate::db::{
    self,
    models::{Token, UsageRecord},
};

/// 配额错误类型。
#[derive(Debug, Clone)]
pub enum QuotaError {
    /// 请求数超限
    RequestQuotaExceeded {
        current: i64,
        limit: i64,
        period: String,
    },
    /// Token 消耗量超限
    TokenQuotaExceeded {
        current: i64,
        limit: i64,
        period: String,
    },
    /// 数据库错误
    Database(String),
}

impl std::fmt::Display for QuotaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuotaError::RequestQuotaExceeded {
                current,
                limit,
                period,
            } => {
                write!(
                    f,
                    "request quota exceeded: {current}/{limit} for period {period}"
                )
            }
            QuotaError::TokenQuotaExceeded {
                current,
                limit,
                period,
            } => {
                write!(
                    f,
                    "token quota exceeded: {current}/{limit} for period {period}"
                )
            }
            QuotaError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

/// 获取当前周期标识。
///
/// - `daily` → `"2026-05-19"`
/// - `monthly` → `"2026-05"`
/// - `unlimited` → `"unlimited"`
pub fn current_period_key(quota_period: &str) -> String {
    match quota_period {
        "daily" => {
            let now = Zoned::now();
            now.strftime("%Y-%m-%d").to_string()
        }
        "monthly" => {
            let now = Zoned::now();
            now.strftime("%Y-%m").to_string()
        }
        _ => "unlimited".to_string(),
    }
}

/// 准入时固定的账本归属；结束结算不得重新计算当前周期。
#[derive(Debug)]
pub struct TokenQuotaContext {
    pub token_id: u64,
    pub period_key: String,
    pub record_id: u64,
}

/// 首条语句取得 Token 写锁，使同 Token 的准入和结算跨连接串行。
/// SQLite 同时取得数据库写锁，避免先读后写的锁升级竞争。
async fn lock_token(tx: &mut toasty::Transaction<'_>, token_id: u64) -> Result<(), String> {
    Token::filter(Token::fields().id().eq(token_id))
        .update()
        .last_used_at(Some(Zoned::now().timestamp().as_millisecond()))
        .exec(tx)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 检查包含本次预留的用量，边界相等可以准入。
fn check_reservation(
    token: &Token,
    request_count: i64,
    token_count: i64,
    estimated_tokens: i64,
) -> Result<(i64, i64), QuotaError> {
    let next_requests = request_count
        .checked_add(1)
        .ok_or_else(|| QuotaError::Database("request counter overflow".into()))?;
    let next_tokens = token_count
        .checked_add(estimated_tokens)
        .ok_or_else(|| QuotaError::Database("token counter overflow".into()))?;
    if token.request_quota > 0 && next_requests > token.request_quota {
        return Err(QuotaError::RequestQuotaExceeded {
            current: next_requests,
            limit: token.request_quota,
            period: token.quota_period.clone(),
        });
    }
    if token.token_quota > 0 && next_tokens > token.token_quota {
        return Err(QuotaError::TokenQuotaExceeded {
            current: next_tokens,
            limit: token.token_quota,
            period: token.quota_period.clone(),
        });
    }
    Ok((next_requests, next_tokens))
}

/// 在同一事务中锁定 Token、检查最新配额并预留本次用量。
/// 无限额 Token 只跳过限额检查，仍记录真实用量。
pub async fn check_and_deduct(
    db: &db::Db,
    token_id: u64,
    estimated_tokens: i64,
) -> Result<TokenQuotaContext, QuotaError> {
    if estimated_tokens < 0 {
        return Err(QuotaError::Database("negative token reservation".into()));
    }
    let mut db = db.clone();
    let mut tx = db
        .transaction()
        .await
        .map_err(|e| QuotaError::Database(e.to_string()))?;
    lock_token(&mut tx, token_id)
        .await
        .map_err(QuotaError::Database)?;
    let current = Token::get_by_id(&mut tx, &token_id)
        .await
        .map_err(|e| QuotaError::Database(e.to_string()))?;
    let period_key = current_period_key(&current.quota_period);
    let existing = UsageRecord::filter(
        UsageRecord::fields()
            .token_id()
            .eq(token_id)
            .and(UsageRecord::fields().period_key().eq(&period_key)),
    )
    .exec(&mut tx)
    .await
    .map_err(|e| QuotaError::Database(e.to_string()))?
    .into_iter()
    .next();
    let (requests, tokens) = check_reservation(
        &current,
        existing.as_ref().map_or(0, |r| r.request_count),
        existing.as_ref().map_or(0, |r| r.token_count),
        estimated_tokens,
    )?;
    let record_id = if let Some(record) = existing {
        UsageRecord::filter(UsageRecord::fields().id().eq(record.id))
            .update()
            .request_count(requests)
            .token_count(tokens)
            .exec(&mut tx)
            .await
            .map_err(|e| QuotaError::Database(e.to_string()))?;
        record.id
    } else {
        toasty::create!(UsageRecord {
            token_id,
            period_key: period_key.clone(),
            request_count: requests,
            token_count: tokens,
        })
        .exec(&mut tx)
        .await
        .map_err(|e| QuotaError::Database(e.to_string()))?
        .id
    };
    tx.commit()
        .await
        .map_err(|e| QuotaError::Database(e.to_string()))?;
    Ok(TokenQuotaContext {
        token_id,
        period_key,
        record_id,
    })
}

/// 对原预留记录按真实用量多退少补；请求数始终只在准入时增加一次。
/// 真实上游用量可能超过估算，结算不能截断事实账本来掩盖超额。
pub async fn adjust_usage(db: &db::Db, ctx: &TokenQuotaContext, delta: i64) -> Result<(), String> {
    if delta == 0 {
        return Ok(());
    }
    let mut db = db.clone();
    let mut tx = db.transaction().await.map_err(|e| e.to_string())?;
    lock_token(&mut tx, ctx.token_id).await?;
    let record = UsageRecord::get_by_id(&mut tx, &ctx.record_id)
        .await
        .map_err(|e| e.to_string())?;
    if record.token_id != ctx.token_id || record.period_key != ctx.period_key {
        return Err("quota reservation ownership mismatch".into());
    }
    let tokens = record
        .token_count
        .checked_add(delta)
        .filter(|count| *count >= 0)
        .ok_or_else(|| "invalid quota settlement delta".to_string())?;
    UsageRecord::filter(UsageRecord::fields().id().eq(ctx.record_id))
        .update()
        .token_count(tokens)
        .exec(&mut tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())
}

/// 后台配额重置任务：清理过期周期记录。
///
/// 仅对 `daily` 和 `monthly` 周期的记录生效，unlimited 记录不会被重置。
/// 不删除旧记录（保留历史），仅确保新周期有空的 UsageRecord。
pub async fn reset_expired_cycles(db: &db::Db) -> Result<(), String> {
    let now = Zoned::now();
    let today = now.strftime("%Y-%m-%d").to_string();
    let this_month = now.strftime("%Y-%m").to_string();

    // 加载所有非 unlimited 的 usage records
    let all_records = UsageRecord::all()
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    for record in all_records {
        if record.period_key == "unlimited" {
            continue;
        }

        let expired = if record.period_key.len() == 10 {
            // daily format: "YYYY-MM-DD"
            record.period_key != today
        } else {
            // monthly format: "YYYY-MM"
            record.period_key != this_month
        };

        if expired {
            warn!(
                token_id = record.token_id,
                period = %record.period_key,
                "found expired usage record (will be reset on next usage)"
            );
            // 不主动删除，下一次准入会因 period_key 不同而创建新记录。
        }
    }

    Ok(())
}
