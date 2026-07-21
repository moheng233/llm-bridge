//! 调用 llm-bridge 的 GET /v1/models 接口并美观展示模型列表。
//!
//! 用法：
//!   cargo run --example list_models -- <base_url> <token>
//!
//! 示例：
//!   cargo run --example list_models -- http://127.0.0.1:3000 lb-xxxxxx

use std::env;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelList {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelEntry {
    id: String,
    owned_by: String,
    capabilities: ModelCapabilities,
    #[serde(default)]
    providers: Vec<ProviderInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelCapabilities {
    max_input_tokens: u32,
    max_output_tokens: u32,
    tool_calling: bool,
    vision: bool,
    thinking: Option<bool>,
    adaptive_thinking: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderInfo {
    provider_id: String,
    provider_display_name: String,
    input_price_per_1m: Option<f64>,
    output_price_per_1m: Option<f64>,
    cache_read_price_per_1m: Option<f64>,
    enabled: bool,
    priority: i64,
}

// ANSI 颜色（无额外依赖）
mod color {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const CYAN: &str = "\x1b[36m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const RED: &str = "\x1b[31m";
}

use color::*;

fn fmt_price(p: Option<f64>) -> String {
    match p {
        Some(v) => format!("${v:.2}"),
        None => format!("{DIM}-{RESET}"),
    }
}

fn fmt_tokens(n: u32) -> String {
    if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

fn fmt_bool(b: bool) -> String {
    if b {
        format!("{GREEN}✓{RESET}")
    } else {
        format!("{DIM}✗{RESET}")
    }
}

fn fmt_opt_bool(b: Option<bool>) -> String {
    match b {
        Some(v) => fmt_bool(v),
        None => format!("{DIM}~{RESET}"),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("用法: cargo run --example list_models -- <base_url> <token>");
        eprintln!("示例: cargo run --example list_models -- http://127.0.0.1:3000 lb-xxxxxx");
        std::process::exit(2);
    }

    let base_url = args[0].trim_end_matches('/');
    let token = &args[1];

    // reqwest 使用 rustls-no-provider 特性，需先安装 crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            env!("CARGO_PKG_NAME"),
            "/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;

    let url = format!("{base_url}/v1/models");
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!("{RED}请求失败{RESET}: HTTP {status}");
        eprintln!("{body}");
        std::process::exit(1);
    }

    let list: ModelList = resp.json().await?;

    println!(
        "\n{BOLD}{CYAN}模型列表{RESET}  {DIM}({} 个模型 · {base_url}){RESET}",
        list.data.len()
    );

    for (i, m) in list.data.iter().enumerate() {
        println!(
            "\n{BOLD}{YELLOW}{}{RESET}  {DIM}owned by {}{RESET}",
            m.id, m.owned_by
        );

        let c = &m.capabilities;
        println!(
            "  {MAGENTA}能力{RESET}   输入 {BOLD}{}{RESET} / 输出 {BOLD}{}{RESET} tokens · 工具 {} · 视觉 {} · 思考 {} · 自适应思考 {}",
            fmt_tokens(c.max_input_tokens),
            fmt_tokens(c.max_output_tokens),
            fmt_bool(c.tool_calling),
            fmt_bool(c.vision),
            fmt_opt_bool(c.thinking),
            fmt_opt_bool(c.adaptive_thinking),
        );

        if m.providers.is_empty() {
            println!("  {DIM}（无提供者信息）{RESET}");
            continue;
        }

        println!("  {MAGENTA}提供者{RESET}");
        for p in &m.providers {
            let enabled = if p.enabled {
                format!("{GREEN}启用{RESET}")
            } else {
                format!("{RED}禁用{RESET}")
            };
            println!(
                "    {BOLD}{}{RESET}（{}）· 输入 {} · 输出 {} · 缓存 {} · {} · 优先级 {}",
                p.provider_id,
                p.provider_display_name,
                fmt_price(p.input_price_per_1m),
                fmt_price(p.output_price_per_1m),
                fmt_price(p.cache_read_price_per_1m),
                enabled,
                p.priority,
            );
        }

        if i + 1 < list.data.len() {
            println!("  {DIM}{} {RESET}", "─".repeat(72));
        }
    }

    println!();
    Ok(())
}
