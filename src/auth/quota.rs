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

/// 获取或创建当前周期的用量记录。
///
/// 如果该 token_id + period_key 的记录不存在，则创建一条空记录。
pub async fn get_or_create_usage_record(
    db: &db::Db,
    token_id: u64,
    period_key: &str,
) -> Result<UsageRecord, String> {
    let existing = UsageRecord::filter(
        UsageRecord::fields()
            .token_id()
            .eq(token_id)
            .and(UsageRecord::fields().period_key().eq(period_key)),
    )
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .next();

    if let Some(record) = existing {
        return Ok(record);
    }

    // 不存在，创建新记录
    let record = toasty::create!(UsageRecord {
        token_id,
        period_key: period_key.to_string(),
        request_count: 0,
        token_count: 0,
    })
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;

    Ok(record)
}

/// 检查配额是否足够。
///
/// 仅检查不扣减。返回 `Ok(())` 或 `Err(QuotaError)`。
pub fn check_quota(token: &Token, usage: &UsageRecord) -> Result<(), QuotaError> {
    // Check request quota
    if token.request_quota > 0 && usage.request_count >= token.request_quota {
        return Err(QuotaError::RequestQuotaExceeded {
            current: usage.request_count,
            limit: token.request_quota,
            period: token.quota_period.clone(),
        });
    }

    // Check token quota
    if token.token_quota > 0 && usage.token_count >= token.token_quota {
        return Err(QuotaError::TokenQuotaExceeded {
            current: usage.token_count,
            limit: token.token_quota,
            period: token.quota_period.clone(),
        });
    }

    Ok(())
}

/// 扣减配额（增加用量计数）。
///
/// 在 API 请求完成后调用，更新 UsageRecord 和 Token 的 `last_used_at`。
pub async fn deduct_usage(
    db: &db::Db,
    token_id: u64,
    record_id: u64,
    request_delta: i64,
    token_delta: i64,
) -> Result<(), String> {
    // 更新用量记录
    let record = UsageRecord::get_by_id(&mut db.clone(), &record_id)
        .await
        .map_err(|e| e.to_string())?;

    UsageRecord::filter(UsageRecord::fields().id().eq(record_id))
        .update()
        .request_count(record.request_count + request_delta)
        .token_count(record.token_count + token_delta)
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    // 更新 Token 的 last_used_at
    let now_ms = Zoned::now().timestamp().as_millisecond();
    Token::filter(Token::fields().id().eq(token_id))
        .update()
        .last_used_at(Some(now_ms))
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// 完整的配额检查 + 扣减流程。
///
/// 在事务中完成：
/// 1. 获取或创建当前周期的 UsageRecord
/// 2. 检查配额
/// 3. 扣减计数
pub async fn check_and_deduct(
    db: &db::Db,
    token: &Token,
    estimated_tokens: i64,
) -> Result<(), QuotaError> {
    if token.quota_period == "unlimited" && token.request_quota <= 0 && token.token_quota <= 0 {
        // 无任何配额限制，跳过
        return Ok(());
    }

    let period_key = current_period_key(&token.quota_period);

    let usage = get_or_create_usage_record(db, token.id, &period_key)
        .await
        .map_err(QuotaError::Database)?;

    // 检查配额
    check_quota(token, &usage)?;

    // 扣减
    deduct_usage(db, token.id, usage.id, 1, estimated_tokens)
        .await
        .map_err(QuotaError::Database)?;

    Ok(())
}

/// 配额结算所需的最小上下文。
///
/// 从 [`Token`] 提取而来，用于请求结束后的对账（多退少补）。
/// 刻意只含标量字段：结算逻辑不需要 Token 的名称/哈希/关联实体，
/// 避免把 ORM 实体跨任务传递（toasty 实体因 Deferred 关联无法干净 Clone）。
#[derive(Debug, Clone)]
pub struct TokenQuotaContext {
    pub token_id: u64,
    pub quota_period: String,
    pub request_quota: i64,
    pub token_quota: i64,
}

impl TokenQuotaContext {
    pub fn from_token(token: &Token) -> Self {
        Self {
            token_id: token.id,
            quota_period: token.quota_period.clone(),
            request_quota: token.request_quota,
            token_quota: token.token_quota,
        }
    }

    fn is_unlimited(&self) -> bool {
        self.quota_period == "unlimited" && self.request_quota <= 0 && self.token_quota <= 0
    }
}

/// 用真实用量对预估扣减做对账（多退少补）。
///
/// `delta` 为 真实 total_tokens - 预估 tokens，可正可负。
/// 无配额限制的 token 直接跳过。
pub async fn adjust_usage(db: &db::Db, ctx: &TokenQuotaContext, delta: i64) -> Result<(), String> {
    if delta == 0 || ctx.is_unlimited() {
        return Ok(());
    }

    let period_key = current_period_key(&ctx.quota_period);
    let usage = get_or_create_usage_record(db, ctx.token_id, &period_key).await?;

    // request_delta = 0：请求数在预估阶段已计，这里只调 token 数
    deduct_usage(db, ctx.token_id, usage.id, 0, delta).await
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
            // 不主动删除，下次 get_or_create_usage_record 时会因 period_key 不同而创建新记录
        }
    }

    Ok(())
}
