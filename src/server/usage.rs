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
use crate::middleware::session_auth::SessionAuth;
use crate::server::AppState;
use crate::types::{LMResponsePart, LanguageModelChatMessage};

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

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SummaryQuery {
    /// 聚合最近 N 天（含今天），默认 14，上限 90。
    pub days: Option<u32>,
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
    SessionAuth(_user): SessionAuth,
    Query(q): Query<SummaryQuery>,
) -> Result<Json<UsageSummaryResponse>, Response> {
    let days = q.days.unwrap_or(14).clamp(1, 90);
    let mut db = state.db.clone();

    // 起始日（UTC，YYYY-MM-DD 字典序即时间序）
    let start_zoned = jiff::Zoned::now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * (days as i64 - 1)))
        .map_err(db_err)?;
    let start_day = start_zoned.strftime("%Y-%m-%d").to_string();

    // usage_daily 按 day 索引范围查询（day 有 #[index]，字典序 >= 即日期 >=）
    let rows: Vec<UsageDaily> = UsageDaily::filter(UsageDaily::fields().day().ge(start_day))
        .exec(&mut db)
        .await
        .map_err(db_err)?;

    // ── 汇总（内存求和）──
    let total_requests: i64 = rows.iter().map(|r| r.request_count).sum();
    let total_tokens: i64 = rows.iter().map(|r| r.total_tokens).sum();
    let total_cost_usd: f64 = rows.iter().map(|r| r.cost_usd).sum();

    // 按日聚合
    let mut by_day: std::collections::BTreeMap<String, DailyPoint> = Default::default();
    for r in &rows {
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
    let daily: Vec<DailyPoint> = by_day.into_values().collect();

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
    model_ranking.sort_by_key(|m| std::cmp::Reverse(m.total_tokens));

    // 错误率与平均 TTFT 需查 trace 表（窗口内终态行）。
    // 这两列不在 usage_daily 中（rollup 无状态维度），但数据量有限（retention 默认 30 天）。
    let window_start = jiff::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * days as i64))
        .map_err(db_err)?;
    let traces: Vec<LlmRequestTrace> =
        LlmRequestTrace::filter(LlmRequestTrace::fields().created_at().ge(window_start))
            .exec(&mut db)
            .await
            .map_err(db_err)?;

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
    }))
}

// ── Trace list ──

#[derive(Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TracesQuery {
    pub status: Option<String>,
    pub model: Option<String>,
    pub token_id: Option<u64>,
    pub interface: Option<String>,
    /// 模糊匹配 request_id 前缀 / error_message
    pub search: Option<String>,
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
    SessionAuth(_user): SessionAuth,
    Query(q): Query<TracesQuery>,
) -> Result<Json<TraceListResponse>, Response> {
    let page = q.page.unwrap_or(0);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);
    let mut db = state.db.clone();

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
    if let Some(token_id) = q.token_id {
        query = query.filter(LlmRequestTrace::fields().token_id().eq(token_id));
    }
    if let Some(interface) = &q.interface
        && let Some(i) = parse_interface(interface)
    {
        query = query.filter(LlmRequestTrace::fields().interface().eq(i));
    }

    // 全量查出后内存筛选 search（request_id 前缀 / error_message 模糊）+ 分页。
    // trace 表有 retention（默认 30 天），窗口内行数有限；待数据量增长后再下沉为 SQL LIKE。
    let mut rows: Vec<LlmRequestTrace> = query
        .order_by(LlmRequestTrace::fields().id().desc())
        .exec(&mut db)
        .await
        .map_err(db_err)?;

    if let Some(search) = &q.search {
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
    SessionAuth(_user): SessionAuth,
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
