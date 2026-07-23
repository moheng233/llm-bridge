//! Trace 保留策略后台任务（PLAN.md §5 O5）。
//!
//! 定期删除 `llm_request_traces` 中 `created_at` 早于保留窗口的记录：
//! `DELETE WHERE created_at < now - retention_days`。
//! `usage_daily` 已聚合无 PII，永久保留（不受此任务影响）。
//!
//! 任务每 6 小时执行一次，启动后先立即执行一轮（清理历史数据不等待首个周期）。

use std::time::Duration;

use jiff::Timestamp;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::db::Db;
use crate::db::models::LlmRequestTrace;

/// 清理周期（每 6 小时一轮）。
const SWEEP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// 启动保留策略后台任务，返回句柄（随 server shutdown 一并 abort）。
///
/// `retention_days = 0` 时任务不启动（保留全部）。
pub fn spawn_retention_task(db: Db, retention_days: u32) -> Option<JoinHandle<()>> {
    if retention_days == 0 {
        info!("trace retention: disabled (retention_days = 0), keeping all traces");
        return None;
    }

    info!(
        retention_days,
        interval_hours = SWEEP_INTERVAL.as_secs() / 3600,
        "trace retention: task started"
    );

    Some(tokio::spawn(async move {
        // 启动后立即执行一轮（清理可能积压的过期数据）
        sweep(&db, retention_days).await;

        let mut interval = tokio::time::interval(SWEEP_INTERVAL);
        interval.tick().await; // 跳过 interval 的首次立即触发（上面已执行）
        loop {
            interval.tick().await;
            sweep(&db, retention_days).await;
        }
    }))
}

/// 执行一轮清理。
async fn sweep(db: &Db, retention_days: u32) {
    let cutoff = match Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * retention_days as i64))
    {
        Ok(ts) => ts,
        Err(e) => {
            warn!(error = %e, "trace retention: failed to compute cutoff, skipping sweep");
            return;
        }
    };

    // 范围删除一步完成（toasty Query::delete() 生成单条 DELETE 语句）。
    match LlmRequestTrace::filter(LlmRequestTrace::fields().created_at().lt(cutoff))
        .delete()
        .exec(&mut db.clone())
        .await
    {
        Ok(_) => {
            info!(cutoff = %cutoff, "trace retention: sweep completed");
        }
        Err(e) => {
            warn!(error = %e, "trace retention: delete failed");
        }
    }
}
