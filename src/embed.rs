//! 前端静态文件嵌入（feature = "embed-frontend"）。
//!
//! 使用 [`rust_embed`] 在编译时将 `frontend/dist/` 目录打包进二进制。
//! 提供 axum handler 用于 Serve 嵌入文件，并处理 SPA 回退（fallback 到 index.html）。

use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};
use rust_embed::RustEmbed;

/// 嵌入 `frontend/dist/` 下的所有构建产物。
///
/// 启用 `embed-frontend` feature 后，`cargo build` 时必须确保 `frontend/dist/` 存在。
/// 建议在 build 脚本或 Makefile 中先执行 `pnpm build --filter llm-bridge-frontend`。
#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct FrontendAssets;

/// Serve 一个嵌入的静态文件。
///
/// - 如果路径对应嵌入文件存在 → 返回文件内容 + 正确的 Content-Type
/// - 如果路径以 `/api/` 开头 → 返回 404（API 路由不应 fallback）
/// - 否则 → SPA fallback：返回 `index.html`（Content-Type: text/html）
pub async fn serve(path: &str) -> Response<Body> {
    // 规范化路径：去掉前导 /
    let clean_path = path.trim_start_matches('/');

    // API 路由不 fallback
    if let Some(file) = FrontendAssets::get(clean_path) {
        let mime = mime_guess_str(clean_path);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(file.data))
            .unwrap();
    }

    // SPA fallback: 返回 index.html
    if let Some(index) = FrontendAssets::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(index.data))
            .unwrap();
    }

    // 连 index.html 都没有（构建产物不完整）
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from("404 Not Found"))
        .unwrap()
}

/// 简单 MIME 类型推断（基于文件扩展名）。
fn mime_guess_str(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "eot" => "application/vnd.ms-fontobject",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}
