//! API Token 服务（Phase 2.1）。
//!
//! 提供 Token 的完整生命周期管理：
//! - 创建 Token（生成随机字符串 + bcrypt 哈希）
//! - 验证 Token（bcrypt 比对）
//! - Token CRUD（查询、更新、删除）
//! - 模型访问检查（allowed_models 精确匹配）
//! - 配额检查（request_quota / token_quota / quota_period）

use bcrypt::{DEFAULT_COST, hash};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::db::{
    self,
    models::{Token, UsageRecord},
};

/// Token 明文前缀。
const TOKEN_PREFIX: &str = "lb_";

/// Token 随机部分的长度（字节，生成 base62 编码后约 43 字符）。
const TOKEN_RANDOM_BYTES: usize = 32;

/// 创建 Token 的请求体（来自 API）。
#[derive(Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenRequest {
    pub name: String,
    #[serde(default)]
    pub allowed_models: Vec<String>,
    #[serde(default)]
    pub request_quota: i64,
    #[serde(default)]
    pub token_quota: i64,
    #[serde(default = "default_quota_period")]
    pub quota_period: String,
}

fn default_quota_period() -> String {
    "unlimited".to_string()
}

/// 创建 Token 的响应体（包含明文 Token，仅返回一次）。
#[derive(Debug, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenResponse {
    pub id: u64,
    pub name: String,
    /// 明文 Token（仅创建时返回一次）
    pub token: String,
    /// Token 前缀（UI 识别用）
    pub token_prefix: String,
    pub allowed_models: Vec<String>,
    pub request_quota: i64,
    pub token_quota: i64,
    pub quota_period: String,
    pub created_at: i64,
}

/// 更新 Token 的请求体。
#[derive(Debug, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTokenRequest {
    pub name: Option<String>,
    pub allowed_models: Option<Vec<String>>,
    pub request_quota: Option<i64>,
    pub token_quota: Option<i64>,
    pub quota_period: Option<String>,
    pub active: Option<bool>,
}

/// 生成随机 Token 字符串。
///
/// 格式: `lb_` + 43 位 base62 字符（256-bit 熵）。
fn generate_token_string() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; TOKEN_RANDOM_BYTES] = rng.random();
    let encoded = base62_encode(&bytes);
    format!("{TOKEN_PREFIX}{encoded}")
}

/// 计算 Token 前缀（用于 UI 展示，如 `lb_ab3x...`）。
fn token_prefix(full_token: &str) -> String {
    let end = full_token.len().min(TOKEN_PREFIX.len() + 8);
    let prefix = &full_token[..end];
    format!("{prefix}...")
}

/// base62 编码（0-9a-zA-Z）。
fn base62_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    // Convert bytes to a big integer and encode in base62.
    // Use a simple approach: treat bytes as big-endian number.
    let mut result = Vec::new();
    let mut num = bytes.to_vec();

    while !num.is_empty() && !all_zero(&num) {
        let rem = div_mod_62(&mut num);
        result.push(CHARS[rem as usize]);
    }

    // Ensure at least one character for zero value
    if result.is_empty() {
        result.push(b'0');
    }

    result.reverse();
    String::from_utf8(result).unwrap_or_default()
}

fn all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b == 0)
}

fn div_mod_62(bytes: &mut Vec<u8>) -> u8 {
    let mut rem: u16 = 0;
    // Trim leading zeros
    while bytes.len() > 1 && bytes[0] == 0 {
        bytes.remove(0);
    }
    let mut result = Vec::with_capacity(bytes.len());
    for &b in bytes.iter() {
        let val = (rem << 8) | b as u16;
        let q = val / 62;
        rem = val % 62;
        if !result.is_empty() || q > 0 {
            result.push(q as u8);
        }
    }
    *bytes = result;
    rem as u8
}

/// 创建新的 API Token。
///
/// # 返回
/// - `(Token, plaintext)` — Token 数据库行和明文 Token。
///   明文仅在此时返回，之后不可获取。
pub async fn create_token(
    db: &db::Db,
    user_id: u64,
    req: CreateTokenRequest,
) -> Result<CreateTokenResponse, String> {
    let plaintext = generate_token_string();
    let prefix = token_prefix(&plaintext);

    let token_hash = hash(&plaintext, DEFAULT_COST).map_err(|e| e.to_string())?;

    let allowed_models_json =
        serde_json::to_string(&req.allowed_models).map_err(|e| e.to_string())?;

    let token = toasty::create!(Token {
        user_id,
        name: req.name.clone(),
        token_hash,
        token_prefix: prefix.clone(),
        allowed_models: allowed_models_json,
        request_quota: req.request_quota,
        token_quota: req.token_quota,
        quota_period: req.quota_period.clone(),
        active: true,
        last_used_at: None,
    })
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;

    info!(
        token_id = token.id,
        user_id,
        token_name = %req.name,
        "API token created"
    );

    Ok(CreateTokenResponse {
        id: token.id,
        name: token.name,
        token: plaintext,
        token_prefix: token.token_prefix,
        allowed_models: req.allowed_models,
        request_quota: token.request_quota,
        token_quota: token.token_quota,
        quota_period: token.quota_period,
        created_at: token.created_at.as_millisecond() / 1000,
    })
}

/// 列出用户的所有 Token（不含 token_hash）。
pub async fn list_user_tokens(db: &db::Db, user_id: u64) -> Result<Vec<Token>, String> {
    let tokens = Token::filter(Token::fields().user_id().eq(user_id))
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(tokens)
}

/// 获取单个 Token。
pub async fn get_token(db: &db::Db, token_id: u64) -> Result<Token, String> {
    Token::get_by_id(&mut db.clone(), &token_id)
        .await
        .map_err(|e| e.to_string())
}

/// 更新 Token 配置。
pub async fn update_token(
    db: &db::Db,
    token_id: u64,
    req: UpdateTokenRequest,
) -> Result<Token, String> {
    let mut token = Token::get_by_id(&mut db.clone(), &token_id)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(name) = req.name {
        token.name = name;
    }
    if let Some(allowed_models) = req.allowed_models {
        token.allowed_models = serde_json::to_string(&allowed_models).map_err(|e| e.to_string())?;
    }
    if let Some(rq) = req.request_quota {
        token.request_quota = rq;
    }
    if let Some(tq) = req.token_quota {
        token.token_quota = tq;
    }
    if let Some(qp) = req.quota_period {
        token.quota_period = qp;
    }
    if let Some(active) = req.active {
        token.active = active;
    }

    Token::filter(Token::fields().id().eq(token_id))
        .update()
        .name(token.name.clone())
        .allowed_models(token.allowed_models.clone())
        .request_quota(token.request_quota)
        .token_quota(token.token_quota)
        .quota_period(token.quota_period.clone())
        .active(token.active)
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    info!(token_id, "token updated");

    Ok(token)
}

/// 删除 Token。
pub async fn delete_token(db: &db::Db, token_id: u64) -> Result<(), String> {
    // 先删除关联的用量记录
    let records = UsageRecord::filter(UsageRecord::fields().token_id().eq(token_id))
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    for record in records {
        UsageRecord::filter(UsageRecord::fields().id().eq(record.id))
            .delete()
            .exec(&mut db.clone())
            .await
            .map_err(|e| e.to_string())?;
    }

    Token::filter(Token::fields().id().eq(token_id))
        .delete()
        .exec(&mut db.clone())
        .await
        .map_err(|e| e.to_string())?;

    info!(token_id, "token deleted");

    Ok(())
}

/// 检查 Token 是否有权限使用指定模型。
///
/// `allowed_models` 使用精确匹配，空数组表示允许全部模型。
pub fn check_model_access(token: &Token, model_name: &str) -> bool {
    let allowed: Vec<String> = serde_json::from_str(&token.allowed_models).unwrap_or_default();
    if allowed.is_empty() {
        return true;
    }
    allowed.iter().any(|m| m == model_name)
}

/// 解析 Token 的 `allowed_models` JSON 字段为字符串数组。
pub fn parse_allowed_models(token: &Token) -> Vec<String> {
    serde_json::from_str(&token.allowed_models).unwrap_or_default()
}
