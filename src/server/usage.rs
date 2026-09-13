//! 用量与请求追踪查询 API（PLAN.md §5 O4/O5）。
//!
//! | 端点 | 说明 |
//! |------|------|
//! | `GET /api/v1/usage/summary` | 仪表盘聚合（usage_daily rollup 内存求和 + 按日序列 + 模型排行） |
//! | `GET /api/v1/usage/traces` | trace 分页列表（多可选维度筛选） |
//! | `GET /api/v1/usage/traces/{request_id}` | 单条 trace 详情（含 Opt-In 内容快照） |
//!
//! 汇总统计不建第二套管线：`usage_daily` 已是 finalize 时同事务维护的预聚合表，
//! 仪表盘聚合查询在内存中对 rollup 行求和，避免全表扫 trace。

use axfetchum::ApiRouter;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::models::{LlmRequestTrace, TraceInterface, TraceStatus, UsageDaily};
use crate::middleware::session_auth::{SessionAuth, is_admin_role};
use crate::server::AppState;
use crate::types::{LMResponsePart, LanguageModelChatMessage};

/// 汇总响应的上一周期部分（同构但不含 errorRate/avgTtftMs/modelRanking）。
#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct PrevSummary {
    pub total_requests: i64,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub daily: Vec<DailyPoint>,
}

pub fn usage_routes() -> ApiRouter<AppState> {
    ApiRouter::<AppState>::new()
        .group("usage")
        .get("/api/v1/usage/summary", get_usage_summary)
        .query::<SummaryQuery>()
        .response::<UsageSummaryResponse>()
        .auth()
        .done()
        .get("/api/v1/usage/traces", list_traces)
        .query::<TracesQuery>()
        .response::<TraceListResponse>()
        .auth()
        .done()
        .get("/api/v1/usage/traces/{request_id}", get_trace)
        .response::<TraceDetail>()
        .auth()
        .done()
}

fn db_err(e: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": e.to_string()})),
    )
        .into_response()
}

// ── Summary ──

#[derive(Clone, Copy, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum UsageScope {
    All,
    Mine,
}

fn view_all_usage(scope: Option<UsageScope>, role: &str) -> Result<bool, Response> {
    let is_admin = is_admin_role(role);
    match scope {
        Some(UsageScope::All) if !is_admin => Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "usage_scope_forbidden"})),
        )
            .into_response()),
        Some(UsageScope::All) => Ok(true),
        Some(UsageScope::Mine) => Ok(false),
        None => Ok(is_admin),
    }
}

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SummaryQuery {
    /// 聚合最近 N 天（含今天），默认 14，上限 90。
    pub days: Option<u32>,
    #[ts(optional)]
    pub scope: Option<UsageScope>,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummaryResponse {
    pub days: u32,
    pub total_requests: i64,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub error_rate: f64,
    pub avg_ttft_ms: Option<i64>,
    /// 按日序列（升序）
    pub daily: Vec<DailyPoint>,
    /// 模型用量排行（按 total_tokens 降序）
    pub model_ranking: Vec<ModelRanking>,
    /// 上一周期（同长度）真实汇总，用于真环比；无数据时为 null。
    pub prev_summary: Option<PrevSummary>,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DailyPoint {
    pub day: String,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ModelRanking {
    pub model: String,
    pub requests: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
}

async fn get_usage_summary(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
    Query(q): Query<SummaryQuery>,
) -> Result<Json<UsageSummaryResponse>, Response> {
    let days = q.days.unwrap_or(14).clamp(1, 90);
    let mut db = state.db.clone();
    let view_all = view_all_usage(q.scope, &user.role)?;
    let user_id = user.user_id;

    // member 仅统计本人 token 的 rollup（usage_daily 无 user_id 列，需先取本人 token id 集）。
    let own_token_ids: Option<Vec<u64>> = if view_all {
        None
    } else {
        Some(
            crate::auth::token::list_user_tokens(&state.db, user.user_id)
                .await
                .map_err(db_err)?
                .into_iter()
                .map(|token| token.id)
                .collect(),
        )
    };

    // ── 当前窗口 rollup 查询 ──
    let now = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC);
    let (rows, prev_rows) =
        query_rollup_windows(&mut db, &now, days, own_token_ids.as_deref()).await?;

    // ── 汇总（内存求和）──
    let total_requests: i64 = rows.iter().map(|r| r.request_count).sum();
    let total_tokens: i64 = rows.iter().map(|r| r.total_tokens).sum();
    let total_cost_usd: f64 = rows.iter().map(|r| r.cost_usd).sum();

    let daily = aggregate_daily(&rows);

    // 模型排行
    let mut by_model: std::collections::HashMap<String, ModelRanking> = Default::default();
    for r in &rows {
        let e = by_model
            .entry(r.model.clone())
            .or_insert_with(|| ModelRanking {
                model: r.model.clone(),
                requests: 0,
                total_tokens: 0,
                cost_usd: 0.0,
            });
        e.requests += r.request_count;
        e.total_tokens += r.total_tokens;
        e.cost_usd += r.cost_usd;
    }
    let mut model_ranking: Vec<ModelRanking> = by_model.into_values().collect();
    model_ranking.sort_by(|left, right| {
        right
            .total_tokens
            .cmp(&left.total_tokens)
            .then_with(|| left.model.cmp(&right.model))
    });

    // ── 上一周期真实汇总（复用 rollup；无数据 → null，前端不再伪造环比）──
    let prev_summary = if prev_rows.is_empty() {
        None
    } else {
        Some(PrevSummary {
            total_requests: prev_rows.iter().map(|r| r.request_count).sum(),
            total_tokens: prev_rows.iter().map(|r| r.total_tokens).sum(),
            total_cost_usd: prev_rows.iter().map(|r| r.cost_usd).sum(),
            daily: aggregate_daily(&prev_rows),
        })
    };

    // 错误率与平均 TTFT 需查 trace 表（窗口内终态行）。
    // 这两列不在 usage_daily 中（rollup 无状态维度），但数据量有限（retention 默认 30 天）。
    let first_day = now
        .checked_sub(jiff::SignedDuration::from_hours(24 * (days as i64 - 1)))
        .map_err(db_err)?;
    let window_start: jiff::Timestamp = format!("{}T00:00:00Z", first_day.strftime("%Y-%m-%d"))
        .parse()
        .map_err(db_err)?;
    let mut trace_query =
        LlmRequestTrace::filter(LlmRequestTrace::fields().created_at().ge(window_start));
    if !view_all {
        trace_query = trace_query.filter(LlmRequestTrace::fields().user_id().eq(user_id));
    }
    let mut traces: Vec<LlmRequestTrace> = trace_query.exec(&mut db).await.map_err(db_err)?;
    if let Some(ids) = &own_token_ids {
        traces.retain(|t| ids.contains(&t.token_id));
    }

    let finals: Vec<&LlmRequestTrace> = traces.iter().filter(|t| t.status.is_final()).collect();
    let errors = finals
        .iter()
        .filter(|t| matches!(t.status, TraceStatus::Error))
        .count();
    let error_rate = if finals.is_empty() {
        0.0
    } else {
        errors as f64 / finals.len() as f64
    };
    let ttfts: Vec<i64> = traces.iter().filter_map(|t| t.ttft_ms).collect();
    let avg_ttft_ms = if ttfts.is_empty() {
        None
    } else {
        Some(ttfts.iter().sum::<i64>() / ttfts.len() as i64)
    };

    Ok(Json(UsageSummaryResponse {
        days,
        total_requests,
        total_tokens,
        total_cost_usd,
        error_rate,
        avg_ttft_ms,
        daily,
        model_ranking,
        prev_summary,
    }))
}

/// 查询当前窗口与上一窗口的 rollup 行（member 限定本人 token id 集）。
async fn query_rollup_windows(
    db: &mut crate::db::Db,
    now: &jiff::Zoned,
    days: u32,
    own_token_ids: Option<&[u64]>,
) -> Result<(Vec<UsageDaily>, Vec<UsageDaily>), Response> {
    let cur_start = now
        .checked_sub(jiff::SignedDuration::from_hours(24 * (days as i64 - 1)))
        .map_err(db_err)?;
    let cur_start_day = cur_start.strftime("%Y-%m-%d").to_string();
    let prev_start = now
        .checked_sub(jiff::SignedDuration::from_hours(24 * (2 * days as i64 - 1)))
        .map_err(db_err)?;
    let prev_start_day = prev_start.strftime("%Y-%m-%d").to_string();

    // 当前窗口：day >= cur_start_day
    let cur: Vec<UsageDaily> = UsageDaily::filter(UsageDaily::fields().day().ge(&cur_start_day))
        .exec(&mut db.clone())
        .await
        .map_err(db_err)?;
    // 上一窗口：day ∈ [prev_start_day, cur_start_day)
    let prev_all: Vec<UsageDaily> =
        UsageDaily::filter(UsageDaily::fields().day().ge(prev_start_day))
            .exec(&mut db.clone())
            .await
            .map_err(db_err)?;
    let prev: Vec<UsageDaily> = prev_all
        .into_iter()
        .filter(|r| r.day.as_str() < cur_start_day.as_str())
        .collect();

    let scoped = |rows: Vec<UsageDaily>| -> Vec<UsageDaily> {
        match own_token_ids {
            Some(ids) => rows
                .into_iter()
                .filter(|r| ids.contains(&r.token_id))
                .collect(),
            None => rows,
        }
    };

    Ok((scoped(cur), scoped(prev)))
}

/// rollup 行按日聚合（升序）。
fn aggregate_daily(rows: &[UsageDaily]) -> Vec<DailyPoint> {
    let mut by_day: std::collections::BTreeMap<String, DailyPoint> = Default::default();
    for r in rows {
        let e = by_day.entry(r.day.clone()).or_insert_with(|| DailyPoint {
            day: r.day.clone(),
            requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            cached_tokens: 0,
            total_tokens: 0,
            cost_usd: 0.0,
        });
        e.requests += r.request_count;
        e.input_tokens += r.input_tokens;
        e.output_tokens += r.output_tokens + r.reasoning_tokens;
        e.cached_tokens += r.cached_tokens;
        e.total_tokens += r.total_tokens;
        e.cost_usd += r.cost_usd;
    }
    by_day.into_values().collect()
}

// ── Trace list ──

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TracesQuery {
    #[ts(optional)]
    pub scope: Option<UsageScope>,
    pub status: Option<String>,
    pub model: Option<String>,
    pub token_id: Option<u64>,
    pub interface: Option<String>,
    /// 模糊匹配 request_id 前缀 / error_message
    pub search: Option<String>,
    /// 创建时间范围（unix 秒，含边界），可选
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    /// 页码（0 起），默认 0
    pub page: Option<u32>,
    /// 每页条数，默认 50，上限 200
    pub page_size: Option<u32>,
}

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TraceListResponse {
    pub items: Vec<TraceSummary>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

/// 列表页条目（不含内容快照大字段）
#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummary {
    pub request_id: String,
    pub interface: String,
    pub token_prefix: String,
    pub model: String,
    pub status: String,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
    pub finish_reason: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub ttft_ms: Option<i64>,
    pub latency_ms: Option<i64>,
    /// unix 秒
    pub created_at: i64,
    /// 是否有内容快照（列表页打标记用）
    pub has_snapshot: bool,
}

fn status_to_str(s: TraceStatus) -> &'static str {
    match s {
        TraceStatus::Pending => "pending",
        TraceStatus::Streaming => "streaming",
        TraceStatus::Success => "success",
        TraceStatus::Error => "error",
        TraceStatus::Cancelled => "cancelled",
    }
}

fn interface_to_str(i: TraceInterface) -> &'static str {
    match i {
        TraceInterface::OpenAiHttp => "openai_http",
        TraceInterface::WsRpc => "ws_rpc",
    }
}

fn trace_to_summary(t: &LlmRequestTrace) -> TraceSummary {
    TraceSummary {
        request_id: t.request_id.clone(),
        interface: interface_to_str(t.interface).to_string(),
        token_prefix: t.token_prefix.clone(),
        model: t.model.clone(),
        status: status_to_str(t.status).to_string(),
        error_type: t.error_type.clone(),
        error_message: t.error_message.clone(),
        finish_reason: t.finish_reason.clone(),
        input_tokens: t.input_tokens.map(|v| v as i64),
        output_tokens: t.output_tokens.map(|v| v as i64),
        cached_tokens: t.cached_tokens.map(|v| v as i64),
        total_tokens: t.total_tokens.map(|v| v as i64),
        cost_usd: t.cost_usd,
        ttft_ms: t.ttft_ms,
        latency_ms: t.latency_ms,
        created_at: t.created_at.as_second(),
        has_snapshot: t.request_messages.is_some() || t.response_parts.is_some(),
    }
}

async fn list_traces(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
    Query(q): Query<TracesQuery>,
) -> Result<Json<TraceListResponse>, Response> {
    let page = q.page.unwrap_or(0);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);
    let mut db = state.db.clone();
    let view_all = view_all_usage(q.scope, &user.role)?;

    // 动态条件叠加（toasty Query builder，多次 filter 以 AND 合并）
    let mut query = toasty::stmt::Query::<toasty::stmt::List<LlmRequestTrace>>::all();
    if let Some(status) = &q.status
        && let Some(s) = parse_status(status)
    {
        query = query.filter(LlmRequestTrace::fields().status().eq(s));
    }
    if let Some(model) = &q.model {
        query = query.filter(LlmRequestTrace::fields().model().eq(model));
    }
    if let Some(interface) = &q.interface
        && let Some(i) = parse_interface(interface)
    {
        query = query.filter(LlmRequestTrace::fields().interface().eq(i));
    }

    // 全量查出后内存筛选（token/user 权限、时间范围、search）+ 分页。
    // trace 表有 retention（默认 30 天），窗口内行数有限；待数据量增长后再下沉为 SQL LIKE。
    let mut rows: Vec<LlmRequestTrace> = query
        .order_by(LlmRequestTrace::fields().id().desc())
        .exec(&mut db)
        .await
        .map_err(db_err)?;

    apply_trace_filters(
        &mut rows,
        view_all,
        user.user_id,
        q.token_id,
        q.date_from,
        q.date_to,
        q.search.as_deref(),
    );

    let total = rows.len() as u64;
    let items: Vec<TraceSummary> = rows
        .into_iter()
        .skip(page as usize * page_size as usize)
        .take(page_size as usize)
        .map(|t| trace_to_summary(&t))
        .collect();

    Ok(Json(TraceListResponse {
        items,
        total,
        page,
        page_size,
    }))
}

/// 列表页内存过滤：权限（member 仅本人）、token_id、时间范围、search。
/// member 传他人 token_id → 空集（不 403，不泄漏他人数据存在性）。
#[cfg_attr(not(test), allow(dead_code))]
fn apply_trace_filters(
    rows: &mut Vec<LlmRequestTrace>,
    is_admin: bool,
    viewer_user_id: u64,
    token_id: Option<u64>,
    date_from: Option<i64>,
    date_to: Option<i64>,
    search: Option<&str>,
) {
    if !is_admin {
        rows.retain(|t| t.user_id == viewer_user_id);
    }
    if let Some(token_id) = token_id {
        rows.retain(|t| t.token_id == token_id);
    }
    if let Some(from) = date_from {
        rows.retain(|t| t.created_at.as_second() >= from);
    }
    if let Some(to) = date_to {
        rows.retain(|t| t.created_at.as_second() <= to);
    }
    if let Some(search) = search {
        let needle = search.trim().to_lowercase();
        if !needle.is_empty() {
            rows.retain(|t| {
                t.request_id.to_lowercase().contains(&needle)
                    || t.model.to_lowercase().contains(&needle)
                    || t.error_message
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&needle)
            });
        }
    }
}

fn parse_status(s: &str) -> Option<TraceStatus> {
    match s {
        "pending" => Some(TraceStatus::Pending),
        "streaming" => Some(TraceStatus::Streaming),
        "success" => Some(TraceStatus::Success),
        "error" => Some(TraceStatus::Error),
        "cancelled" => Some(TraceStatus::Cancelled),
        _ => None,
    }
}

fn parse_interface(s: &str) -> Option<TraceInterface> {
    match s {
        "openai_http" => Some(TraceInterface::OpenAiHttp),
        "ws_rpc" => Some(TraceInterface::WsRpc),
        _ => None,
    }
}

// ── Trace detail ──

#[derive(Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TraceDetail {
    #[serde(flatten)]
    pub summary: TraceSummary,
    pub trace_id: Option<String>,
    pub user_id: i64,
    pub token_id: i64,
    pub provider_id: String,
    pub provider_model_id: String,
    pub protocol: String,
    pub upstream_status: Option<i64>,
    pub upstream_request_id: Option<String>,
    pub estimated_tokens: i64,
    pub reasoning_tokens: Option<i64>,
    pub first_chunk_at: Option<i64>,
    pub completed_at: Option<i64>,
    /// Opt-In 内容快照
    pub request_messages: Option<Vec<LanguageModelChatMessage>>,
    pub response_parts: Option<Vec<LMResponsePart>>,
}

async fn get_trace(
    State(state): State<AppState>,
    SessionAuth(user): SessionAuth,
    Path(request_id): Path<String>,
) -> Result<Json<TraceDetail>, Response> {
    let mut db = state.db.clone();
    // toasty get_by_<unique> 未命中时返回 Err（含 "not found"），区分 404 与 500
    let t = match LlmRequestTrace::get_by_request_id(&mut db, &request_id).await {
        Ok(t) => t,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("NotFound") {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": "trace not found"})),
                )
                    .into_response());
            }
            return Err(db_err(e));
        }
    };

    // 越权防护（BUG010）：member 访问他人 trace → 403，不泄漏快照内容。
    if !is_admin_role(&user.role) && t.user_id != user.user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "forbidden"})),
        )
            .into_response());
    }

    let summary = trace_to_summary(&t);
    Ok(Json(TraceDetail {
        summary,
        trace_id: t.trace_id.clone(),
        user_id: t.user_id as i64,
        token_id: t.token_id as i64,
        provider_id: t.provider_id.clone(),
        provider_model_id: t.provider_model_id.clone(),
        protocol: t.protocol.clone(),
        upstream_status: t.upstream_status.map(|v| v as i64),
        upstream_request_id: t.upstream_request_id.clone(),
        estimated_tokens: t.estimated_tokens,
        reasoning_tokens: t.reasoning_tokens.map(|v| v as i64),
        first_chunk_at: t.first_chunk_at.map(|ts| ts.as_second()),
        completed_at: t.completed_at.map(|ts| ts.as_second()),
        request_messages: t.request_messages.map(|j| j.0),
        response_parts: t.response_parts.map(|j| j.0),
    }))
}
