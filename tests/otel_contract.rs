#![cfg(feature = "otel")]
use axum::{Router, extract::Path, http::StatusCode, routing::post};
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[tokio::test]
async fn telemetry_exports_without_aborting_tokio_runtime() {
    const CHILD: &str = "LLM_BRIDGE_OTEL_CONTRACT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        llm_bridge::http::ensure_crypto_provider();
        let guard = llm_bridge::observability::init("otel-contract").unwrap();
        tracing::info_span!("acceptance", request_id = "otel-contract-request").in_scope(|| {
            tracing::info!("export all three signals");
            llm_bridge::observability::genai::record_finalize(
                &llm_bridge::observability::genai::GenAiFinalize {
                    provider_name: "openai",
                    request_model: "fixture".into(),
                    response_model: "fixture".into(),
                    input_tokens: Some(40),
                    output_tokens: Some(60),
                    duration_s: 0.2,
                    ttft_s: Some(0.01),
                },
            );
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
        tokio::task::spawn_blocking(move || guard.shutdown())
            .await
            .unwrap();
        return;
    }
    let counts = Arc::new([
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    ]);
    let observed = counts.clone();
    let router = Router::new().route(
        "/v1/{signal}",
        post(move |Path(signal): Path<String>| {
            let observed = observed.clone();
            async move {
                let index = match signal.as_str() {
                    "traces" => 0,
                    "logs" => 1,
                    "metrics" => 2,
                    _ => return StatusCode::NOT_FOUND,
                };
                observed[index].fetch_add(1, Ordering::SeqCst);
                StatusCode::OK
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "telemetry_exports_without_aborting_tokio_runtime",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint)
        .env("OTEL_BSP_SCHEDULE_DELAY", "50")
        .env("OTEL_BLRP_SCHEDULE_DELAY", "50")
        .env("OTEL_METRIC_EXPORT_INTERVAL", "50")
        .env("RUST_LOG", "info")
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("telemetry child failed to shut down");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    server.abort();
    assert!(
        status.success(),
        "OTLP export/shutdown must not abort the runtime: {status}"
    );
    for (signal, count) in ["traces", "logs", "metrics"].into_iter().zip(counts.iter()) {
        assert!(
            count.load(Ordering::SeqCst) > 0,
            "collector must receive {signal}"
        );
    }
}
