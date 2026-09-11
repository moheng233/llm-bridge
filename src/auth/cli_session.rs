//! 设备码登录会话服务（PLAN.md §4.1，RFC 8628 风格）。
//!
//! vscode 插件无需手动复制粘贴即可获取 API Token：
//!
//! 1. 插件 `POST /api/v1/auth/cli-sessions` 创建会话 → 得到 `{ sessionId, userCode, verificationUrl, expiresIn, interval }`
//! 2. 用户浏览器打开验证页（`GET /auth/cli-verify?code=`，前端路由），登录后主动输码 + 点确认
//! 3. `POST /api/v1/auth/cli-sessions/confirm`（Session）：签发 `vscode:` Token，标记 `approved`
//! 4. 插件 `GET /api/v1/auth/cli-sessions/{sessionId}`（凭据即 sessionId）轮询：
//!    - `pending` → 202 + `{ status }`
//!    - `approved` → 200 + `{ status, token, tokenPrefix }`（明文仅此一次）→ 会话转 `consumed`
//!    - `consumed` / `expired` → 410
//!
//! ## 安全约束
//!
//! - 用户码 6 位数字（仅 2-9，不含 0/1），10 分钟过期，轮询间隔 5s
//! - 会话凭据 `session_key` 为强随机会话标识，仅创建时返回一次
//! - 防钓鱼：URL 中出现用户码不构成授权，必须登录用户主动输码 + 确认
//! - 签发前列出该用户旧 `vscode:` Token 并置 `active = false`（不物理删除，保留审计），
//!   同一用户同时刻最多一个有效 vscode Token
//! - 并发确认 / 并发领取均由单事务原子化：`confirm` 以状态 CAS（`pending → approved`），
//!   `consume`（轮询领取）以状态 CAS（`approved → consumed`），双重确认不会被重放
//! - Token 明文仅在 `approved → consumed` 之间暂存；被领取或过期后持久化清除

use jiff::{SignedDuration, Timestamp};
use rand::RngExt;
use tracing::info;

use crate::auth::token;
use crate::db::{
    self,
    models::{CliSession, CliSessionStatus, Token},
};

/// 会话有效期：10 分钟。
pub const SESSION_TTL: SignedDuration = SignedDuration::from_secs(600);

/// 插件轮询间隔（秒）：5s。
pub const POLL_INTERVAL_SECONDS: u64 = 5;

/// 用户码长度。
const USER_CODE_LEN: usize = 6;

/// 用户码字符集：仅 2-9（避开 0/1/O/I 混淆字符）。
const USER_CODE_CHARS: &[u8] = b"23456789";

/// 会话凭据随机字节数（128-bit）。
const SESSION_KEY_BYTES: usize = 16;

/// vscode Token 名称前缀（命名约定，复用现有 Token 表）。
pub const VSCODE_TOKEN_PREFIX: &str = "vscode:";

// ── DTO ──

/// 创建 CLI 会话响应（给插件）。
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateCliSessionResponse {
    /// 会话 ID（轮询凭据）。
    pub session_id: String,
    /// 6 位用户码（用户在浏览器输入）。
    pub user_code: String,
    /// 验证页完整 URL。
    pub verification_url: String,
    /// 会话有效期（秒）。
    pub expires_in: u64,
    /// 建议轮询间隔（秒）。
    pub interval: u64,
}

/// 轮询响应：pending / consumed / expired（无 Token）。
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CliSessionPoll {
    pub status: String,
}

/// 轮询响应：approved（带一次性明文 Token）。
#[derive(Debug, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CliSessionApproved {
    pub status: String,
    /// 明文 Token（仅此一次，响应后即转 consumed）。
    pub token: String,
    /// Token 前缀（UI / 审计识别）。
    pub token_prefix: String,
}

/// 确认请求体（验证页提交）。
#[derive(Debug, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmCliSessionRequest {
    pub user_code: String,
}

// ── 生成 ──

/// 生成 6 位用户码（仅 2-9）。
fn generate_user_code() -> String {
    let mut rng = rand::rng();
    (0..USER_CODE_LEN)
        .map(|_| USER_CODE_CHARS[rng.random_range(0..USER_CODE_CHARS.len())] as char)
        .collect()
}

/// 生成强随机会话凭据（`cs_` + 32 位 hex，128-bit 熵）。
fn generate_session_key() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; SESSION_KEY_BYTES] = rng.random();
    format!(
        "cs_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

// ── 服务函数 ──

/// 创建新的 CLI 登录会话。
///
/// 用户码空间为 8^6；唯一约束冲突时重新生成。
pub async fn create_session(
    db: &db::Db,
    verification_url_base: &str,
) -> Result<CreateCliSessionResponse, String> {
    for _ in 0..8 {
        let user_code = generate_user_code();
        let session_key = generate_session_key();
        let now = Timestamp::now();
        let expires_at = now.checked_add(SESSION_TTL).map_err(|e| e.to_string())?;

        let result = toasty::create!(CliSession {
            user_code: user_code.clone(),
            session_key: session_key.clone(),
            status: CliSessionStatus::Pending,
            user_id: None,
            token_plaintext: None,
            token_prefix: None,
            expires_at,
        })
        .exec(&mut db.clone())
        .await;

        match result {
            Ok(_) => {
                let verification_url =
                    format!("{verification_url_base}/auth/cli-verify?code={user_code}");
                return Ok(CreateCliSessionResponse {
                    session_id: session_key,
                    user_code,
                    verification_url,
                    expires_in: SESSION_TTL.as_secs() as u64,
                    interval: POLL_INTERVAL_SECONDS,
                });
            }
            Err(error) => {
                let collisions = CliSession::filter(
                    CliSession::fields()
                        .user_code()
                        .eq(&user_code)
                        .or(CliSession::fields().session_key().eq(&session_key)),
                )
                .exec(&mut db.clone())
                .await
                .map_err(|query_error| query_error.to_string())?;
                if collisions.is_empty() {
                    return Err(error.to_string());
                }
            }
        }
    }
    Err("failed to generate unique user code after retries".into())
}

/// 确认授权（验证页提交，Session 用户）。
///
/// 事务内原子化：
/// 1. 校验用户码 + 会话 pending（CAS pending → approved 失败即 409/410）
/// 2. 吊销该用户旧 `vscode:` Token（`active = false`，不物理删除）
/// 3. 签发新 Token（`vscode:<8位随机后缀>`，allowed_models=[] / unlimited）
/// 4. 暂存 Token 明文 + 前缀，会话转 approved
///
/// 返回：`Ok(Some(()))` 成功；`Ok(None)` 用户码不匹配；`Err(msg)` 已过期/已确认/内部错误。
pub async fn confirm_session(
    db: &db::Db,
    user_id: u64,
    req: &ConfirmCliSessionRequest,
) -> Result<Option<()>, String> {
    let user_code = normalize_code(&req.user_code);
    if user_code.len() != USER_CODE_LEN
        || !user_code
            .bytes()
            .all(|byte| USER_CODE_CHARS.contains(&byte))
    {
        return Ok(None);
    }
    let prepared = token::prepare_token().await?;

    let mut db = db.clone();
    let mut tx = db.transaction().await.map_err(|e| e.to_string())?;
    toasty::sql::statement(format!("UPDATE users SET id = id WHERE id = {user_id}"))
        .exec(&mut tx)
        .await
        .map_err(|error| error.to_string())?;
    let user = crate::db::models::User::get_by_id(&mut tx, &user_id)
        .await
        .map_err(|error| error.to_string())?;
    if !user.active {
        return Err("account disabled".into());
    }
    CliSession::filter(CliSession::fields().user_code().eq(user_code.clone()))
        .update()
        .user_code(user_code.clone())
        .exec(&mut tx)
        .await
        .map_err(|error| error.to_string())?;

    let sessions = CliSession::filter(CliSession::fields().user_code().eq(user_code.clone()))
        .exec(&mut tx)
        .await
        .map_err(|e| e.to_string())?;
    let Some(session) = sessions.into_iter().next() else {
        return Ok(None);
    };

    // 惰性过期判断（事务内读到的行）
    if session.status == CliSessionStatus::Pending && Timestamp::now() >= session.expires_at {
        CliSession::filter(
            CliSession::fields()
                .id()
                .eq(session.id)
                .and(CliSession::fields().status().eq(CliSessionStatus::Pending)),
        )
        .update()
        .status(CliSessionStatus::Expired)
        .exec(&mut tx)
        .await
        .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        return Err("session expired".into());
    }
    if session.status != CliSessionStatus::Pending {
        return Err("session already confirmed or consumed".into());
    }

    // 1) CAS：pending → approved（并发重复确认只有一个成功）
    CliSession::filter(
        CliSession::fields()
            .id()
            .eq(session.id)
            .and(CliSession::fields().status().eq(CliSessionStatus::Pending)),
    )
    .update()
    .status(CliSessionStatus::Approved)
    .user_id(Some(user_id))
    .exec(&mut tx)
    .await
    .map_err(|e| e.to_string())?;

    // 2) 吊销该用户旧 vscode Token（不物理删除，保留审计）
    let old_tokens = Token::filter(
        Token::fields()
            .user_id()
            .eq(user_id)
            .and(Token::fields().active().eq(true)),
    )
    .exec(&mut tx)
    .await
    .map_err(|e| e.to_string())?;
    for old in old_tokens {
        if old.name.starts_with(VSCODE_TOKEN_PREFIX) {
            Token::filter(Token::fields().id().eq(old.id))
                .update()
                .active(false)
                .exec(&mut tx)
                .await
                .map_err(|e| e.to_string())?;
            info!(token_id = old.id, user_id, "revoked old vscode token");
        }
    }

    // 3) 签发新 Token（默认 allowed_models=[] 全模型、unlimited 配额）
    let suffix: String = {
        let mut rng = rand::rng();
        (0..8)
            .map(|_| {
                const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
                CHARS[rng.random_range(0..CHARS.len())] as char
            })
            .collect()
    };
    let resp = token::create_prepared_in_tx(
        &mut tx,
        user_id,
        prepared,
        token::TokenIssueSpec {
            name: format!("{VSCODE_TOKEN_PREFIX}{suffix}"),
            allowed_models: vec![],
            request_quota: 0,
            token_quota: 0,
            quota_period: "unlimited".to_string(),
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    // 4) 暂存明文 + 前缀（consumed 后清除）
    let prefix = resp.token_prefix.clone();
    CliSession::filter(CliSession::fields().id().eq(session.id))
        .update()
        .token_plaintext(Some(resp.token.clone()))
        .token_prefix(Some(prefix.clone()))
        .exec(&mut tx)
        .await
        .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    info!(user_id, token_prefix = %prefix, "CLI session approved");
    Ok(Some(()))
}

/// 插件轮询领取 Token。
///
/// 事务内原子化：仅 `approved` 状态一次性交付明文并转 `consumed`；
/// 明文交付后立即持久化清除。并发重复领取只有一个成功。
///
/// 返回：
/// - `Ok(None)`：会话不存在
/// - `Ok(Some(Err статус))`：pending / consumed / expired
/// - `Ok(Some(Ok(CliSessionApproved)))`：一次领取成功
pub async fn poll_and_consume(
    db: &db::Db,
    session_key: &str,
) -> Result<Option<Result<CliSessionApproved, CliSessionPoll>>, String> {
    let mut db = db.clone();
    let mut tx = db.transaction().await.map_err(|e| e.to_string())?;
    CliSession::filter(CliSession::fields().session_key().eq(session_key))
        .update()
        .session_key(session_key)
        .exec(&mut tx)
        .await
        .map_err(|error| error.to_string())?;

    let sessions = CliSession::filter(
        CliSession::fields()
            .session_key()
            .eq(session_key.to_string()),
    )
    .exec(&mut tx)
    .await
    .map_err(|e| e.to_string())?;
    let Some(session) = sessions.into_iter().next() else {
        return Ok(None);
    };

    if matches!(
        session.status,
        CliSessionStatus::Pending | CliSessionStatus::Approved
    ) && Timestamp::now() >= session.expires_at
    {
        CliSession::filter(CliSession::fields().id().eq(session.id))
            .update()
            .status(CliSessionStatus::Expired)
            .token_plaintext(None::<String>)
            .exec(&mut tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        return Ok(Some(Err(CliSessionPoll {
            status: "expired".into(),
        })));
    }

    match session.status {
        CliSessionStatus::Pending => {
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(Some(Err(CliSessionPoll {
                status: "pending".into(),
            })))
        }
        CliSessionStatus::Consumed | CliSessionStatus::Expired => {
            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(Some(Err(CliSessionPoll {
                status: session.status.status_str().into(),
            })))
        }
        CliSessionStatus::Approved => {
            let Some(plaintext) = session.token_plaintext.clone() else {
                return Err("approved session missing token plaintext".into());
            };
            let prefix = session.token_prefix.clone().unwrap_or_default();

            // CAS：approved → consumed（并发重复领取只有一个成功）
            CliSession::filter(
                CliSession::fields()
                    .id()
                    .eq(session.id)
                    .and(CliSession::fields().status().eq(CliSessionStatus::Approved)),
            )
            .update()
            .status(CliSessionStatus::Consumed)
            .token_plaintext(None::<String>)
            .exec(&mut tx)
            .await
            .map_err(|e| e.to_string())?;

            tx.commit().await.map_err(|e| e.to_string())?;
            Ok(Some(Ok(CliSessionApproved {
                status: "approved".into(),
                token: plaintext,
                token_prefix: prefix,
            })))
        }
    }
}

/// 规范化用户码：去空格与连字符。
fn normalize_code(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
}

impl CliSessionStatus {
    /// 小写序列化名（与 DTO `status` 字符串一致）。
    pub fn status_str(&self) -> &'static str {
        match self {
            CliSessionStatus::Pending => "pending",
            CliSessionStatus::Approved => "approved",
            CliSessionStatus::Consumed => "consumed",
            CliSessionStatus::Expired => "expired",
        }
    }
}

// ── 行为测试（本批次交付测试代码；统一验证由 Main 执行）──
//
// 覆盖：
// - 创建会话 → 用户码 6 位且仅 2-9、session_key 强随机唯一、10 分钟 TTL
// - pending 轮询返回 pending（不泄露明文）
// - 确认后 approved 一次领取，第二次领取 consumed、明文不再可得
// - 过期会话轮询/确认均 expired/拒绝
// - 重复确认（并发语义：CAS）仅一次成功
// - 签发新 vscode Token 前吊销同用户旧 vscode Token
//
// 注：测试直接调用服务函数（内存 SQLite），不经过 HTTP 层。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::User;

    async fn test_db() -> db::Db {
        db::init(db::all_models(), "sqlite::memory:")
            .await
            .expect("init")
    }

    async fn create_user(db: &db::Db, sub: &str) -> u64 {
        let user = toasty::create!(User {
            oidc_sub: sub.to_string(),
            name: sub.to_string(),
            role: crate::db::models::UserRole::Member,
            active: true,
        })
        .exec(&mut db.clone())
        .await
        .expect("create user");
        user.id
    }

    fn assert_valid_user_code(code: &str) {
        assert_eq!(code.len(), 6);
        assert!(
            code.bytes().all(|b| (b'2'..=b'9').contains(&b)),
            "user code must only contain digits 2-9, got {code}"
        );
    }

    #[tokio::test]
    async fn create_session_generates_valid_code_and_key() {
        let db = test_db().await;
        let resp = create_session(&db, "https://bridge.example.com")
            .await
            .expect("create");
        assert_valid_user_code(&resp.user_code);
        assert!(resp.session_id.starts_with("cs_"));
        assert_eq!(resp.session_id.len(), 3 + SESSION_KEY_BYTES * 2);
        assert_eq!(resp.expires_in, 600);
        assert_eq!(resp.interval, 5);
        assert_eq!(
            resp.verification_url,
            format!(
                "https://bridge.example.com/auth/cli-verify?code={}",
                resp.user_code
            )
        );
    }

    #[tokio::test]
    async fn pending_poll_returns_pending_without_token() {
        let db = test_db().await;
        let resp = create_session(&db, "").await.expect("create");
        let poll = poll_and_consume(&db, &resp.session_id)
            .await
            .expect("poll")
            .expect("found");
        let err = poll.expect_err("pending must not deliver token");
        assert_eq!(err.status, "pending");
    }

    #[tokio::test]
    async fn unknown_session_key_returns_none() {
        let db = test_db().await;
        let poll = poll_and_consume(&db, "cs_nonexistent").await.expect("poll");
        assert!(poll.is_none());
    }

    #[tokio::test]
    async fn confirm_then_single_consumption() {
        let db = test_db().await;
        let user_id = create_user(&db, "sub-confirm").await;
        let resp = create_session(&db, "").await.expect("create");

        confirm_session(
            &db,
            user_id,
            &ConfirmCliSessionRequest {
                user_code: resp.user_code.clone(),
            },
        )
        .await
        .expect("confirm")
        .expect("code matched");

        // 第一次领取成功，拿到明文
        let first = poll_and_consume(&db, &resp.session_id)
            .await
            .expect("poll")
            .expect("found")
            .expect("approved");
        assert_eq!(first.status, "approved");
        assert!(first.token.starts_with("lb_"));

        // 第二次领取：consumed，不再有明文
        let second = poll_and_consume(&db, &resp.session_id)
            .await
            .expect("poll")
            .expect("found")
            .expect_err("consumed");
        assert_eq!(second.status, "consumed");

        // 持久化明文已被清除
        let sessions = CliSession::filter(
            CliSession::fields()
                .session_key()
                .eq(resp.session_id.clone()),
        )
        .exec(&mut db.clone())
        .await
        .expect("query");
        let row = sessions.into_iter().next().expect("row");
        assert_eq!(row.status, CliSessionStatus::Consumed);
        assert!(
            row.token_plaintext.is_none(),
            "plaintext must be cleared after consumption"
        );
    }

    #[tokio::test]
    async fn duplicate_confirm_is_rejected() {
        let db = test_db().await;
        let user_id = create_user(&db, "sub-dup").await;
        let resp = create_session(&db, "").await.expect("create");
        let req = ConfirmCliSessionRequest {
            user_code: resp.user_code.clone(),
        };

        confirm_session(&db, user_id, &req)
            .await
            .expect("first")
            .expect("matched");
        let second = confirm_session(&db, user_id, &req).await;
        assert!(second.is_err(), "second confirm must be rejected");
    }

    #[tokio::test]
    async fn wrong_code_returns_none() {
        let db = test_db().await;
        let user_id = create_user(&db, "sub-wrong").await;
        let result = confirm_session(
            &db,
            user_id,
            &ConfirmCliSessionRequest {
                user_code: "999999".to_string(),
            },
        )
        .await
        .expect("no db error");
        assert!(
            result.is_none(),
            "unknown code must return None, not authorize"
        );
    }

    #[tokio::test]
    async fn expired_session_polls_410_and_confirm_rejected() {
        let db = test_db().await;
        let user_id = create_user(&db, "sub-expired").await;
        let resp = create_session(&db, "").await.expect("create");

        // 手动把 expires_at 拨回过去
        let sessions = CliSession::filter(
            CliSession::fields()
                .session_key()
                .eq(resp.session_id.clone()),
        )
        .exec(&mut db.clone())
        .await
        .expect("query");
        let row = sessions.into_iter().next().expect("row");
        let past = Timestamp::now()
            .checked_sub(SignedDuration::from_secs(60))
            .expect("past ts");
        CliSession::filter(CliSession::fields().id().eq(row.id))
            .update()
            .expires_at(past)
            .exec(&mut db.clone())
            .await
            .expect("backdate");

        // 轮询 → expired
        let poll = poll_and_consume(&db, &resp.session_id)
            .await
            .expect("poll")
            .expect("found")
            .expect_err("expired");
        assert_eq!(poll.status, "expired");

        // 确认 → 拒绝
        let confirm = confirm_session(
            &db,
            user_id,
            &ConfirmCliSessionRequest {
                user_code: resp.user_code.clone(),
            },
        )
        .await;
        assert!(confirm.is_err(), "confirm on expired session must fail");

        // 过期后明文清除
        let sessions = CliSession::filter(
            CliSession::fields()
                .session_key()
                .eq(resp.session_id.clone()),
        )
        .exec(&mut db.clone())
        .await
        .expect("query");
        let row = sessions.into_iter().next().expect("row");
        assert!(row.token_plaintext.is_none());
    }

    #[tokio::test]
    async fn new_vscode_token_revokes_previous_ones() {
        let db = test_db().await;
        let user_id = create_user(&db, "sub-revoke").await;

        // 预置一个旧 vscode Token（手工构造：直接建行，绕过明文返回逻辑）
        let old = toasty::create!(Token {
            user_id,
            name: "vscode:oldsuffix".to_string(),
            token_hash: "$2y$05$legacyhashplaceholder".to_string(),
            token_prefix: "lb_old...".to_string(),
            allowed_models: "[]".to_string(),
            request_quota: 0,
            token_quota: 0,
            quota_period: "unlimited".to_string(),
            active: true,
            last_used_at: None,
        })
        .exec(&mut db.clone())
        .await
        .expect("seed old token");
        // 以及一个无关 Token，不应被吊销
        let unrelated = toasty::create!(Token {
            user_id,
            name: "dev-machine".to_string(),
            token_hash: "$2y$05$otherhashplaceholder".to_string(),
            token_prefix: "lb_oth...".to_string(),
            allowed_models: "[]".to_string(),
            request_quota: 0,
            token_quota: 0,
            quota_period: "unlimited".to_string(),
            active: true,
            last_used_at: None,
        })
        .exec(&mut db.clone())
        .await
        .expect("seed unrelated token");

        let resp = create_session(&db, "").await.expect("create");
        confirm_session(
            &db,
            user_id,
            &ConfirmCliSessionRequest {
                user_code: resp.user_code.clone(),
            },
        )
        .await
        .expect("confirm")
        .expect("matched");

        let old_row = Token::get_by_id(&mut db.clone(), &old.id)
            .await
            .expect("old");
        assert!(!old_row.active, "old vscode token must be revoked");

        let unrelated_row = Token::get_by_id(&mut db.clone(), &unrelated.id)
            .await
            .expect("unrelated");
        assert!(unrelated_row.active, "non-vscode token must not be touched");

        // 新 Token 生效且命名规范
        let tokens = token::list_user_tokens(&db, user_id).await.expect("list");
        let vscode_ones: Vec<_> = tokens
            .iter()
            .filter(|t| t.name.starts_with(VSCODE_TOKEN_PREFIX) && t.active)
            .collect();
        assert_eq!(vscode_ones.len(), 1, "exactly one active vscode token");
        let name = &vscode_ones[0].name;
        let suffix = name.strip_prefix(VSCODE_TOKEN_PREFIX).expect("prefix");
        assert_eq!(suffix.len(), 8);
        assert!(parse_allowed_models_empty(vscode_ones[0]));
        assert_eq!(vscode_ones[0].request_quota, 0);
        assert_eq!(vscode_ones[0].token_quota, 0);
        assert_eq!(vscode_ones[0].quota_period, "unlimited");
    }

    fn parse_allowed_models_empty(t: &Token) -> bool {
        serde_json::from_str::<Vec<String>>(&t.allowed_models)
            .map(|v| v.is_empty())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn concurrent_confirm_only_one_wins() {
        let db = test_db().await;
        let user_id = create_user(&db, "sub-conc").await;
        let resp = create_session(&db, "").await.expect("create");
        let req = ConfirmCliSessionRequest {
            user_code: resp.user_code.clone(),
        };

        // 模拟并发：两个任务同时确认
        let d1 = db.clone();
        let d2 = db.clone();
        let r1 = tokio::spawn(async move { confirm_session(&d1, user_id, &req).await });
        let req2 = ConfirmCliSessionRequest {
            user_code: resp.user_code.clone(),
        };
        let r2 = tokio::spawn(async move { confirm_session(&d2, user_id, &req2).await });
        let (a, b) = futures::join!(r1, r2);
        let (a, b) = (a.expect("join"), b.expect("join"));

        // 至少一个成功，且至多一个成功（CAS 保证）
        let ok_count = usize::from(a.clone().is_ok()) + usize::from(b.clone().is_ok());
        assert!(
            ok_count >= 1,
            "at least one confirm must succeed, got a={a:?} b={b:?}"
        );
        assert!(
            ok_count <= 1,
            "concurrent confirms must not both authorize, got a={a:?} b={b:?}"
        );
    }

    #[tokio::test]
    async fn concurrent_polls_deliver_token_once() {
        let db = test_db().await;
        let user_id = create_user(&db, "sub-pollrace").await;
        let resp = create_session(&db, "").await.expect("create");
        confirm_session(
            &db,
            user_id,
            &ConfirmCliSessionRequest {
                user_code: resp.user_code.clone(),
            },
        )
        .await
        .expect("confirm")
        .expect("matched");

        let d1 = db.clone();
        let d2 = db.clone();
        let key = resp.session_id.clone();
        let key2 = resp.session_id.clone();
        let p1 = tokio::spawn(async move { poll_and_consume(&d1, &key).await });
        let p2 = tokio::spawn(async move { poll_and_consume(&d2, &key2).await });
        let (a, b) = futures::join!(p1, p2);
        let (a, b) = (a.expect("join"), b.expect("join"));

        let delivered: Vec<_> = [a, b]
            .into_iter()
            .filter_map(|r| r.expect("found"))
            .collect();
        let tokens: Vec<_> = delivered.iter().filter_map(|r| r.as_ref().ok()).collect();
        assert_eq!(
            tokens.len(),
            1,
            "token must be delivered exactly once, got {delivered:?}"
        );
        assert_eq!(
            delivered.len() - tokens.len(),
            1,
            "the other poller must see consumed"
        );
    }
}
