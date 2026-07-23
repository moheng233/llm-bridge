use tracing_subscriber::EnvFilter;

#[cfg(feature = "otel")]
use tracing_subscriber::prelude::*;

pub mod genai;
pub mod retention;
pub mod trace_writer;

/// Holds observability resources for graceful shutdown.
///
/// Without the `otel` feature, this is a unit struct — `shutdown()` is a no-op.
#[cfg(feature = "otel")]
pub struct ObservabilityGuard {
    logger_provider: opentelemetry_sdk::logs::SdkLoggerProvider,
    tracer_provider: opentelemetry_sdk::trace::SdkTracerProvider,
    meter_provider: opentelemetry_sdk::metrics::SdkMeterProvider,
}

/// Holds observability resources for graceful shutdown.
///
/// Without the `otel` feature, this is a unit struct — `shutdown()` is a no-op.
#[cfg(not(feature = "otel"))]
pub struct ObservabilityGuard;

/// Initialise tracing and optional OpenTelemetry OTLP export.
///
/// - **Without `otel`**: only stdout/err formatted logging via `tracing-subscriber`.
/// - **With `otel`**: adds OTLP HTTP exporters for logs and spans (sends to
///   the collector configured via standard `OTEL_EXPORTER_OTLP_*` env vars).
pub fn init(_service_name: &str) -> Result<ObservabilityGuard, Box<dyn std::error::Error>> {
    #[cfg(feature = "otel")]
    {
        use opentelemetry::trace::TracerProvider;
        use opentelemetry_sdk::Resource;

        let service_name = _service_name.to_owned();
        let resource = Resource::builder()
            .with_service_name(service_name.clone())
            .build();

        let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(
                opentelemetry_otlp::LogExporter::builder()
                    .with_http()
                    .build()?,
            )
            .build();

        let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(
                opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .build()?,
            )
            .build();

        // Meter（PLAN.md §5 O2）：GenAI metrics 经 OTLP 周期推送。
        // record 走 `opentelemetry::global::meter`，故须注册为全局 provider。
        let meter_provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
            .with_resource(resource)
            .with_reader(
                opentelemetry_sdk::metrics::periodic_reader_with_async_runtime::PeriodicReader::builder(
                    opentelemetry_otlp::MetricExporter::builder()
                        .with_http()
                        .build()?,
                    opentelemetry_sdk::runtime::Tokio,
                )
                .build(),
            )
            .build();
        opentelemetry::global::set_meter_provider(meter_provider.clone());

        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_target(true)
            .with_timer(tracing_subscriber::fmt::time::uptime())
            .with_level(true)
            .finish()
            .with(
                tracing_opentelemetry::layer()
                    .with_tracer(tracer_provider.tracer(service_name.to_owned())),
            )
            .with(
                opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
                    &logger_provider,
                ),
            );

        tracing::subscriber::set_global_default(subscriber)?;

        return Ok(ObservabilityGuard {
            logger_provider,
            tracer_provider,
            meter_provider,
        });
    }

    #[cfg(not(feature = "otel"))]
    {
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_target(true)
            .with_timer(tracing_subscriber::fmt::time::uptime())
            .with_level(true)
            .finish();

        tracing::subscriber::set_global_default(subscriber)?;

        Ok(ObservabilityGuard)
    }
}

impl ObservabilityGuard {
    pub fn shutdown(self) {
        #[cfg(feature = "otel")]
        {
            if let Err(error) = self.tracer_provider.shutdown() {
                tracing::warn!(error = %error, "failed to shut down tracer provider");
            }

            if let Err(error) = self.meter_provider.shutdown() {
                tracing::warn!(error = %error, "failed to shut down meter provider");
            }

            if let Err(error) = self.logger_provider.shutdown() {
                tracing::warn!(error = %error, "failed to shut down logger provider");
            }
        }
    }
}
