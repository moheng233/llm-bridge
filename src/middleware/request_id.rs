//! 请求标识中间件 — 为每个请求生成唯一 `request_id` 并贯穿全链路。
//!
//! 对齐 PLAN.md §5 O1 阶段：
//! - 生成 UUID → `request.extensions_mut().insert(RequestId)`
//! - 记录到当前 span 的 `request_id` 字段（stdout 日志可见）
//! - otel feature 下同时记录 OTel `trace_id` 字段（stdout ↔ OTLP ↔ DB 三方互查）
//! - 响应头回写 `x-request-id`
//!
//! 设计决策：手写 `from_fn` 中间件而非 tower-http `SetRequestId`——
//! axum 的 Extensions 在 WS upgrade 前已就位，天然覆盖 §4 的 WS 路径；
//! tower-http 在 WS 路径拿不到 extensions，否决。

use axum::{
    extract::FromRequestParts,
    http::{HeaderValue, Request, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::Span;

/// 请求唯一标识（UUID v4），由网关生成。
///
/// 作为 request extension 注入，handler 可通过提取器获取；
/// 同时回写到响应头 `x-request-id`，贯穿 stdout 日志 / OTel / DB。
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    /// 生成新的 UUID v4 请求 ID。
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 获取 ID 字符串切片。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// axum 提取器：从 request extensions 中获取 `RequestId`。
///
/// 中间件已保证注入，不存在时返回 500（属于框架接线错误）。
///
/// # 使用示例
///
/// ```ignore
/// async fn handler(RequestId(request_id): RequestId) -> impl IntoResponse {
///     tracing::info!(%request_id, "handling request");
/// }
/// ```
impl<S> FromRequestParts<S> for RequestId
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<RequestId>().cloned().ok_or_else(|| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "request id middleware not installed",
            )
                .into_response()
        })
    }
}

/// `x-request-id` 响应头名。
pub const X_REQUEST_ID: &str = "x-request-id";

/// Axum 中间件：为每个请求生成 `RequestId` 并贯穿全链路。
///
/// 流程：生成 UUID → 注入 extensions → 记录到当前 span → 执行 handler →
/// 响应头回写 `x-request-id`。
///
/// otel feature 下额外记录 `trace_id` 字段到当前 span，使 stdout fmt 层
/// 输出与 OTLP trace 可互查（`tracing_opentelemetry::OpenTelemetrySpanExt`）。
pub async fn request_id_middleware(request: Request<axum::body::Body>, next: Next) -> Response {
    let request_id = RequestId::new();

    // 记录到当前 span（handler 的 #[instrument] span 在中间件之后创建，
    // 此处记录的是路由匹配前的连接级 span；handler 内应再次 record）。
    let span = Span::current();
    span.record("request_id", request_id.as_str());

    // otel feature：记录 OTel trace_id，stdout ↔ OTLP 互查
    #[cfg(feature = "otel")]
    {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let otel_context = span.context();
        let otel_span = otel_context.span();
        let trace_id = otel_span.span_context().trace_id();
        if trace_id != opentelemetry::trace::TraceId::INVALID {
            span.record("trace_id", trace_id.to_string().as_str());
        }
    }

    // 注入 request extensions（WS upgrade 前已就位，天然覆盖 §4 WS 路径）
    let mut request = request;
    request.extensions_mut().insert(request_id.clone());

    // 执行 handler
    let mut response = next.run(request).await;

    // 响应头回写
    if let Ok(value) = HeaderValue::from_str(request_id.as_str()) {
        response.headers_mut().insert(X_REQUEST_ID, value);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_is_valid_uuid() {
        let id = RequestId::new();
        assert!(uuid::Uuid::parse_str(id.as_str()).is_ok());
    }

    #[test]
    fn request_id_display() {
        let id = RequestId("test-123".to_string());
        assert_eq!(id.to_string(), "test-123");
        assert_eq!(id.as_str(), "test-123");
    }

    /// 端到端验证：中间件注入 extension + 响应头回写 `x-request-id`，
    /// 且 handler 内可通过提取器拿到同一个 ID。
    #[tokio::test]
    async fn middleware_injects_extension_and_writes_header() {
        use axum::{Router, body::Body, http::Request, response::Json, routing::get};
        use tower::ServiceExt;

        async fn handler(RequestId(id): RequestId) -> Json<serde_json::Value> {
            Json(serde_json::json!({ "request_id": id.as_str() }))
        }

        let app: Router = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        // 响应头必须回写 x-request-id 且为合法 UUID（先拷贝为 owned，释放 response 借用）
        let header_value = response
            .headers()
            .get(X_REQUEST_ID)
            .expect("x-request-id header must be present")
            .to_str()
            .unwrap()
            .to_string();
        assert!(uuid::Uuid::parse_str(&header_value).is_ok());

        // handler 提取器拿到的 ID 与响应头一致
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["request_id"].as_str().unwrap(), header_value);
    }
}
