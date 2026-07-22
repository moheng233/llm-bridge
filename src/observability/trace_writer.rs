//! 请求追踪异步写入器（PLAN.md §5 O3）。
//!
//! handler 热路径只发 mpsc 事件，专用后台任务逐条落盘；mpsc 满则**丢弃并计数**
//! （观察性数据可丢，业务请求不可阻塞）。
//!
//! 生命周期状态机 `pending → finalized`（success / error）：
//! - [`TraceEvent::Begin`]：请求受理时 INSERT 一行 `pending`（含 Opt-In 的
//!   `request_messages` 快照）——中途崩溃可见「卡住」的请求而非丢记录。
//! - [`TraceEvent::Finalize`]：请求结束时 UPDATE 为终态，并在**同一事务**内
//!   upsert `usage_daily` rollup（read-then-write，复合唯一约束兜底）。
//!
//! `stream_to_sse` 的首 chunk 时间经 [`TraceEvent::Finalize`] 的 `first_chunk_at`
//! 字段带回（pending → streaming 的中间态由 `first_chunk_at IS NOT NULL` 表达，
//! 不单独发事件——避免热路径多一次发送）。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use jiff::Timestamp;
use tokio::sync::mpsc;

use crate::db::models::{LlmRequestTrace, TraceInterface, TraceStatus, UsageDaily};
use crate::db::{self, Db};
use crate::types::{LMResponsePart, LanguageModelChatMessage};

/// mpsc 通道容量。满则丢弃（背压不阻塞业务）。
const CHANNEL_CAPACITY: usize = 1024;

/// 写入器句柄（克隆廉价，内部为 mpsc sender + 共享计数器）。
///
/// handler 通过 [`AppState`](crate::server::AppState) 持有，热路径仅 `try_send`。
#[derive(Clone)]
pub struct TraceWriter {
    tx: mpsc::Sender<TraceEvent>,
    dropped: Arc<AtomicU64>,
}

/// 写入器事件。
pub enum TraceEvent {
    /// 请求受理：INSERT pending 行。
    Begin(Box<BeginTrace>),
    /// 请求结束：UPDATE 终态 + 事务内 upsert usage_daily。
    Finalize(Box<FinalizeTrace>),
}

/// [`TraceEvent::Begin`] 载荷。
pub struct BeginTrace {
    pub request_id: String,
    pub trace_id: Option<String>,
    pub interface: TraceInterface,
    pub token_id: u64,
    pub user_id: u64,
    pub token_prefix: String,
    pub model: String,
    pub provider_id: String,
    pub provider_model_id: String,
    pub protocol: String,
    pub estimated_tokens: i64,
    /// Opt-In 内容快照（`capture_content` 关闭时为 None）。
    pub request_messages: Option<Vec<LanguageModelChatMessage>>,
}

/// [`TraceEvent::Finalize`] 载荷。
pub struct FinalizeTrace {
    pub request_id: String,
    pub status: TraceStatus,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
    pub upstream_status: Option<u16>,
    pub finish_reason: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub upstream_request_id: Option<String>,
    pub first_chunk_at: Option<Timestamp>,
    pub completed_at: Timestamp,
    pub ttft_ms: Option<i64>,
    pub latency_ms: Option<i64>,
    /// Opt-In 响应快照。
    pub response_parts: Option<Vec<LMResponsePart>>,
    // ── usage_daily rollup 维度（finalize 时由 handler 一并提供）──
    pub day: String,
    pub token_id: u64,
    pub model: String,
}

impl TraceWriter {
    /// 启动写入器后台任务，返回句柄。
    ///
    /// 后台任务持有 `Db` 独立克隆（共享连接池），逐条消费事件落盘。
    pub fn spawn(db: Db) -> Self {
        let (tx, mut rx) = mpsc::channel::<TraceEvent>(CHANNEL_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let dropped_clone = Arc::clone(&dropped);

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Err(error) = handle_event(&db, event).await {
                    tracing::warn!(error = %error, "trace writer: failed to persist event");
                }
            }
            tracing::debug!(
                dropped = dropped_clone.load(Ordering::Relaxed),
                "trace writer: channel closed, shutting down"
            );
        });

        Self { tx, dropped }
    }

    /// 热路径发送事件（非阻塞）。mpsc 满则丢弃并计数。
    pub fn send(&self, event: TraceEvent) {
        if self.tx.try_send(event).is_err() {
            let n = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            // 丢弃计数也投影到 OTel（otel feature 下）。
            crate::observability::genai::record_dropped_trace(n);
            if n % 100 == 1 {
                tracing::warn!(
                    dropped = n,
                    "trace writer: channel full, dropping trace events"
                );
            }
        }
    }

    /// 累计被丢弃的事件数（背压指标）。
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// 处理单个事件（后台任务上下文）。
async fn handle_event(db: &Db, event: TraceEvent) -> Result<(), String> {
    match event {
        TraceEvent::Begin(b) => insert_pending(db, *b).await,
        TraceEvent::Finalize(f) => finalize_trace(db, *f).await,
    }
}

/// INSERT pending 行。
async fn insert_pending(db: &Db, b: BeginTrace) -> Result<(), String> {
    let request_messages = b.request_messages.map(toasty::Json);
    toasty::create!(LlmRequestTrace {
        request_id: b.request_id,
        trace_id: b.trace_id,
        interface: b.interface,
        token_id: b.token_id,
        user_id: b.user_id,
        token_prefix: b.token_prefix,
        model: b.model,
        provider_id: b.provider_id,
        provider_model_id: b.provider_model_id,
        protocol: b.protocol,
        status: TraceStatus::Pending,
        estimated_tokens: b.estimated_tokens,
        request_messages: request_messages,
    })
    .exec(&mut db.clone())
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// UPDATE 终态 + 同事务 upsert usage_daily。
async fn finalize_trace(db: &Db, f: FinalizeTrace) -> Result<(), String> {
    let mut db = db.clone();
    let mut tx = db.transaction().await.map_err(|e| e.to_string())?;

    // usage_daily rollup 所需标量先拷贝（response_parts 随后被 move，不能再用整个 `f`）。
    let usage_dims = UsageDailyDims {
        day: f.day.clone(),
        token_id: f.token_id,
        model: f.model.clone(),
        input_tokens: f.input_tokens,
        output_tokens: f.output_tokens,
        reasoning_tokens: f.reasoning_tokens,
        cached_tokens: f.cached_tokens,
        total_tokens: f.total_tokens,
        cost_usd: f.cost_usd,
    };

    // 1) UPDATE trace 终态（按 request_id 唯一索引定位）。
    let response_parts = f.response_parts.map(toasty::Json);
    LlmRequestTrace::update_by_request_id(&f.request_id)
        .status(f.status)
        .error_type(f.error_type.clone())
        .error_message(f.error_message.clone())
        .upstream_status(f.upstream_status)
        .finish_reason(f.finish_reason.clone())
        .input_tokens(f.input_tokens)
        .output_tokens(f.output_tokens)
        .reasoning_tokens(f.reasoning_tokens)
        .cached_tokens(f.cached_tokens)
        .total_tokens(f.total_tokens)
        .cost_usd(f.cost_usd)
        .upstream_request_id(f.upstream_request_id.clone())
        .first_chunk_at(f.first_chunk_at)
        .completed_at(Some(f.completed_at))
        .ttft_ms(f.ttft_ms)
        .latency_ms(f.latency_ms)
        .response_parts(response_parts)
        .exec(&mut tx)
        .await
        .map_err(|e| e.to_string())?;

    // 2) upsert usage_daily（read-then-write，同事务）。
    upsert_usage_daily(&mut tx, &usage_dims).await?;

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// usage_daily rollup 所需的维度与用量标量（从 [`FinalizeTrace`] 摘出，
/// 避免与 `response_parts` 的 move 冲突）。
struct UsageDailyDims {
    day: String,
    token_id: u64,
    model: String,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    reasoning_tokens: Option<u64>,
    cached_tokens: Option<u64>,
    total_tokens: Option<u64>,
    cost_usd: Option<f64>,
}

/// 在事务内 upsert `usage_daily`：存在则累加，不存在则插入。
///
/// 复合唯一约束 `(day, token_id, model)` 在 DB 层兜底防重；
/// 单写入器串行化使 read-then-write 无竞争。
async fn upsert_usage_daily(
    tx: &mut toasty::Transaction<'_>,
    f: &UsageDailyDims,
) -> Result<(), String> {
    let existing: Vec<UsageDaily> = UsageDaily::filter_by_day_and_token_id_and_model(
        f.day.clone(),
        f.token_id,
        f.model.clone(),
    )
    .exec(tx)
    .await
    .map_err(|e| e.to_string())?;

    let input = f.input_tokens.unwrap_or(0) as i64;
    let output = f.output_tokens.unwrap_or(0) as i64;
    let reasoning = f.reasoning_tokens.unwrap_or(0) as i64;
    let cached = f.cached_tokens.unwrap_or(0) as i64;
    let total = f.total_tokens.unwrap_or(0) as i64;
    let cost = f.cost_usd.unwrap_or(0.0);

    if let Some(row) = existing.into_iter().next() {
        UsageDaily::update_by_id(row.id)
            .request_count(row.request_count + 1)
            .input_tokens(row.input_tokens + input)
            .output_tokens(row.output_tokens + output)
            .reasoning_tokens(row.reasoning_tokens + reasoning)
            .cached_tokens(row.cached_tokens + cached)
            .total_tokens(row.total_tokens + total)
            .cost_usd(row.cost_usd + cost)
            .exec(tx)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        toasty::create!(UsageDaily {
            day: f.day.clone(),
            token_id: f.token_id,
            model: f.model.clone(),
            request_count: 1,
            input_tokens: input,
            output_tokens: output,
            reasoning_tokens: reasoning,
            cached_tokens: cached,
            total_tokens: total,
            cost_usd: cost,
        })
        .exec(tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 当前 UTC 日期（`"YYYY-MM-DD"`），用于 usage_daily 维度。
pub fn current_day() -> String {
    Timestamp::now().strftime("%Y-%m-%d").to_string()
}

// 让模块内引用 db 的意图显式化（避免未使用告警）。
#[allow(unused_imports)]
use db as _db;

#[cfg(test)]
mod tests {
    use super::*;

    fn begin(req_id: &str) -> BeginTrace {
        BeginTrace {
            request_id: req_id.to_string(),
            trace_id: None,
            interface: TraceInterface::OpenAiHttp,
            token_id: 1,
            user_id: 1,
            token_prefix: "lb_test".to_string(),
            model: "openai/gpt-4o".to_string(),
            provider_id: "openai".to_string(),
            provider_model_id: "gpt-4o".to_string(),
            protocol: "openai".to_string(),
            estimated_tokens: 100,
            request_messages: None,
        }
    }

    fn finalize(req_id: &str, day: &str) -> Box<FinalizeTrace> {
        Box::new(FinalizeTrace {
            request_id: req_id.to_string(),
            status: TraceStatus::Success,
            error_type: None,
            error_message: None,
            upstream_status: None,
            finish_reason: Some("stop".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(20),
            reasoning_tokens: None,
            cached_tokens: None,
            total_tokens: Some(30),
            cost_usd: Some(0.001),
            upstream_request_id: Some("chatcmpl-1".to_string()),
            first_chunk_at: Some(Timestamp::now()),
            completed_at: Timestamp::now(),
            ttft_ms: Some(120),
            latency_ms: Some(450),
            response_parts: None,
            day: day.to_string(),
            token_id: 1,
            model: "openai/gpt-4o".to_string(),
        })
    }

    #[tokio::test]
    async fn begin_then_finalize_roundtrip_and_rollup() {
        let db = db::init(db::all_models(), "sqlite::memory:")
            .await
            .expect("init");

        // Begin → pending 行存在
        insert_pending(&db, begin("req-a")).await.expect("begin");
        let t = LlmRequestTrace::get_by_request_id(&mut db.clone(), &"req-a".to_string())
            .await
            .expect("get");
        assert_eq!(t.status, TraceStatus::Pending);
        assert!(!t.status.is_final());

        // Finalize → 终态 + usage_daily rollup
        finalize_trace(&db, *finalize("req-a", "2026-07-22"))
            .await
            .expect("finalize");
        let t = LlmRequestTrace::get_by_request_id(&mut db.clone(), &"req-a".to_string())
            .await
            .expect("get");
        assert_eq!(t.status, TraceStatus::Success);
        assert!(t.status.is_final());
        assert_eq!(t.input_tokens, Some(10));
        assert_eq!(t.finish_reason.as_deref(), Some("stop"));
        assert!(t.first_chunk_at.is_some());
        assert!(t.completed_at.is_some());

        // 再次 finalize 同维度另一请求 → usage_daily 累加为 2 行? 否——同一行累加
        insert_pending(&db, begin("req-b")).await.expect("begin b");
        finalize_trace(&db, *finalize("req-b", "2026-07-22"))
            .await
            .expect("finalize b");

        let rows: Vec<UsageDaily> = UsageDaily::filter_by_day("2026-07-22")
            .exec(&mut db.clone())
            .await
            .expect("query daily");
        assert_eq!(rows.len(), 1, "same (day,token_id,model) must rollup to one row");
        assert_eq!(rows[0].request_count, 2);
        assert_eq!(rows[0].input_tokens, 20);
        assert_eq!(rows[0].total_tokens, 60);
    }

    #[tokio::test]
    async fn writer_drops_when_channel_full() {
        // 不调用 spawn（无消费者），直接构造满 channel 验证丢弃计数。
        let (tx, _rx) = mpsc::channel::<TraceEvent>(1);
        let writer = TraceWriter {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
        };
        // 容量 1：第一条成功，第二条起被丢弃。
        writer.send(TraceEvent::Begin(Box::new(begin("x1"))));
        writer.send(TraceEvent::Begin(Box::new(begin("x2"))));
        writer.send(TraceEvent::Begin(Box::new(begin("x3"))));
        assert_eq!(writer.dropped_count(), 2);
    }

    #[tokio::test]
    async fn writer_spawn_end_to_end() {
        let db = db::init(db::all_models(), "sqlite::memory:")
            .await
            .expect("init");
        let writer = TraceWriter::spawn(db.clone());

        writer.send(TraceEvent::Begin(Box::new(begin("req-e2e"))));
        writer.send(TraceEvent::Finalize(finalize("req-e2e", "2026-07-22")));

        // 后台任务异步落盘：轮询直到可见（有界等待）。
        let mut ok = false;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            if let Ok(t) =
                LlmRequestTrace::get_by_request_id(&mut db.clone(), &"req-e2e".to_string()).await
                && t.status == TraceStatus::Success
            {
                ok = true;
                break;
            }
        }
        assert!(ok, "trace should be persisted by background writer");
        assert_eq!(writer.dropped_count(), 0);
    }
}
