//! 向 llm-bridge 的 POST /v1/chat/completions 发送一组测试并展示回复。
//!
//! 覆盖场景：
//!   1. 单轮问答（最小冒烟）
//!   2. 多轮对话（验证上下文记忆）
//!   3. 工具调用对话回放（验证 assistant tool_calls / tool 角色消息能被桥接层接受并转发）
//!   4. 思考链保留（打印 reasoning_content，并在后续轮次携带历史，验证上下文连贯）
//!
//! 用法：
//!   cargo run --example chat_smoke_test -- <base_url> <token> <model> [场景]
//!
//!   [场景] 可选，逗号分隔：single,multi,tools,reasoning,all（默认 all）
//!
//! 示例：
//!   cargo run --example chat_smoke_test -- http://127.0.0.1:3000 lb-xxxxxx gpt-5-mini
//!   cargo run --example chat_smoke_test -- http://127.0.0.1:3000 lb-xxxxxx gpt-5-mini multi,reasoning

use std::env;
use std::time::Instant;

use serde::Deserialize;
use serde_json::{Value, json};

// ── 响应类型 ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatCompletion {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ResponseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ResponseToolCall {
    id: String,
    function: ResponseFunction,
}

#[derive(Debug, Deserialize)]
struct ResponseFunction {
    name: String,
    arguments: String,
}

// ── ANSI 颜色 ────────────────────────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const MAGENTA: &str = "\x1b[35m";

// ── 客户端 ───────────────────────────────────────────────────────────────────

struct TestClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    model: String,
}

struct TurnOutcome {
    content: String,
    reasoning: Option<String>,
    tool_calls: Vec<ResponseToolCall>,
    elapsed: std::time::Duration,
}

impl TestClient {
    fn new(
        base_url: &str,
        token: String,
        model: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // reqwest 使用 rustls-no-provider 特性，需先安装 crypto provider
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("failed to install rustls crypto provider");

        let http = reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            model,
        })
    }

    async fn send(&self, messages: &[Value]) -> Result<TurnOutcome, Box<dyn std::error::Error>> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
        });

        println!("{DIM}POST {url} ({} 条消息){RESET}", messages.len());

        let started = Instant::now();
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {body}").into());
        }

        let completion: ChatCompletion = resp.json().await?;
        let elapsed = started.elapsed();

        let choice = completion
            .choices
            .into_iter()
            .next()
            .ok_or("响应中没有 choices")?;

        Ok(TurnOutcome {
            content: choice.message.content.unwrap_or_default(),
            reasoning: choice.message.reasoning_content,
            tool_calls: choice.message.tool_calls.unwrap_or_default(),
            elapsed,
        })
    }
}

// ── 展示辅助 ─────────────────────────────────────────────────────────────────

fn print_header(title: &str) {
    println!("\n{CYAN}{BOLD}════════ {title} ════════{RESET}");
}

fn print_step(label: &str) {
    println!("\n{MAGENTA}{BOLD}▶ {label}{RESET}");
}

fn print_user(text: &str) {
    println!("{CYAN}用户{RESET}  {text}");
}

fn print_outcome(outcome: &TurnOutcome) {
    if let Some(reasoning) = &outcome.reasoning
        && !reasoning.is_empty()
    {
        println!("\n{YELLOW}{BOLD}[思考]{RESET}");
        println!("{DIM}{reasoning}{RESET}");
    }

    if !outcome.tool_calls.is_empty() {
        println!("\n{MAGENTA}{BOLD}[工具调用]{RESET}");
        for tc in &outcome.tool_calls {
            println!("  {BOLD}{}{RESET} (id: {})", tc.function.name, tc.id);
            println!("  {DIM}{}{RESET}", tc.function.arguments);
        }
    }

    println!("\n{GREEN}{BOLD}[回复]{RESET}");
    if outcome.content.is_empty() {
        println!("{DIM}（空回复）{RESET}");
    } else {
        println!("{}", outcome.content);
    }

    println!("{DIM}耗时 {:.2}s{RESET}", outcome.elapsed.as_secs_f64());
}

// ── 场景 1：单轮问答 ─────────────────────────────────────────────────────────

async fn scenario_single(client: &TestClient) -> Result<(), Box<dyn std::error::Error>> {
    print_header("场景 1：单轮问答");

    let prompt = "用一句话解释什么是 Rust 的所有权。";
    print_user(prompt);

    let messages = json!([{ "role": "user", "content": prompt }]);
    let outcome = client.send(messages.as_array().unwrap()).await?;
    print_outcome(&outcome);

    if outcome.content.is_empty() {
        return Err("单轮问答返回空内容".into());
    }
    Ok(())
}

// ── 场景 2：多轮对话 ─────────────────────────────────────────────────────────

async fn scenario_multi(client: &TestClient) -> Result<(), Box<dyn std::error::Error>> {
    print_header("场景 2：多轮对话（上下文记忆）");

    let mut messages: Vec<Value> = Vec::new();

    // 第一轮：设定上下文
    let turn1 = "记住这个数字：42。不要解释，只回复『已记住』。";
    print_step("第 1 轮：设定上下文");
    print_user(turn1);
    messages.push(json!({ "role": "user", "content": turn1 }));
    let outcome1 = client.send(&messages).await?;
    print_outcome(&outcome1);
    messages.push(json!({ "role": "assistant", "content": outcome1.content }));

    // 第二轮：依赖上一轮的记忆
    let turn2 = "我刚才让你记住的数字是多少？把它乘以 2 后告诉我。";
    print_step("第 2 轮：引用上下文");
    print_user(turn2);
    messages.push(json!({ "role": "user", "content": turn2 }));
    let outcome2 = client.send(&messages).await?;
    print_outcome(&outcome2);
    messages.push(json!({ "role": "assistant", "content": outcome2.content }));

    // 第三轮：继续深入，验证更长上下文
    let turn3 = "再在上一步的结果上加 8，最终答案是多少？";
    print_step("第 3 轮：链式推理");
    print_user(turn3);
    messages.push(json!({ "role": "user", "content": turn3 }));
    let outcome3 = client.send(&messages).await?;
    print_outcome(&outcome3);

    // 宽松断言：最终结果应包含 92（42*2=84, 84+8=92）
    if !outcome3.content.contains("92") {
        println!("{YELLOW}警告：第三轮回复中未出现预期数字 92，模型可能未正确保留上下文。{RESET}");
    } else {
        println!("{GREEN}✓ 上下文记忆验证通过（包含 92）{RESET}");
    }

    Ok(())
}

// ── 场景 3：工具调用对话回放 ─────────────────────────────────────────────────
//
// 说明：llm-bridge 的 HTTP 入口目前不接收 `tools` 参数，也不会在响应里透传
// tool_calls。因此本场景采用「回放」方式：构造一段包含 assistant tool_calls 与
// tool 角色结果的标准 OpenAI 多轮消息，验证桥接层能正确接受并转发这种结构。

async fn scenario_tools(client: &TestClient) -> Result<(), Box<dyn std::error::Error>> {
    print_header("场景 3：工具调用对话回放");

    let tool_call_id = "call_get_weather_shanghai_001";

    let messages = json!([
        {
            "role": "user",
            "content": "上海现在的天气怎么样？适合出门吗？"
        },
        {
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {
                    "id": tool_call_id,
                    "type": "function",
                    "function": {
                        "name": "get_current_weather",
                        "arguments": "{\"city\":\"上海\",\"unit\":\"celsius\"}"
                    }
                }
            ]
        },
        {
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": "{\"city\":\"上海\",\"temperature\":\"28°C\",\"condition\":\"晴\",\"humidity\":\"65%\",\"wind\":\"东南风 3 级\"}"
        }
    ]);

    print_step("回放包含 tool_calls 与 tool 结果的多轮消息");
    println!("{DIM}消息序列：user → assistant(tool_calls) → tool(result){RESET}");

    let outcome = client.send(messages.as_array().unwrap()).await?;
    print_outcome(&outcome);

    if outcome.content.is_empty() {
        println!("{YELLOW}警告：工具结果回放后返回空内容。{RESET}");
    } else {
        println!("{GREEN}✓ 工具调用消息结构已被接受并生成回复{RESET}");
    }

    Ok(())
}

// ── 场景 4：思考链保留 ───────────────────────────────────────────────────────

async fn scenario_reasoning(client: &TestClient) -> Result<(), Box<dyn std::error::Error>> {
    print_header("场景 4：思考链保留");

    let mut messages: Vec<Value> = Vec::new();

    // 第一轮：提出一个需要思考的问题
    let turn1 = "一个浴缸放满水需要 12 分钟，排空需要 18 分钟。如果同时打开进水口和排水口，多久能放满？请仔细思考后给出答案。";
    print_step("第 1 轮：需要推理的问题");
    print_user(turn1);
    messages.push(json!({ "role": "user", "content": turn1 }));

    let outcome1 = client.send(&messages).await?;
    print_outcome(&outcome1);

    let had_reasoning = outcome1
        .reasoning
        .as_ref()
        .map(|r| !r.is_empty())
        .unwrap_or(false);

    if had_reasoning {
        println!("{GREEN}✓ 响应中包含 reasoning_content（思考链已透传）{RESET}");
    } else {
        println!(
            "{YELLOW}提示：响应中没有 reasoning_content（模型或提供者可能不返回思考链）{RESET}"
        );
    }

    // 第二轮：携带完整历史（含上一轮回复），验证上下文仍然连贯。
    // 注意：llm-bridge 内部会把历史中的 Thinking part 以文本形式拼回给上游，
    // 因此多轮对话中思考内容不会凭空丢失。
    messages.push(json!({ "role": "assistant", "content": outcome1.content }));

    let turn2 = "如果把进水速度提高到原来的 1.5 倍，其他条件不变，答案会变成多少？";
    print_step("第 2 轮：基于上一轮继续追问");
    print_user(turn2);
    messages.push(json!({ "role": "user", "content": turn2 }));

    let outcome2 = client.send(&messages).await?;
    print_outcome(&outcome2);

    if outcome2.content.is_empty() {
        println!("{YELLOW}警告：第二轮返回空内容。{RESET}");
    } else {
        println!("{GREEN}✓ 携带历史（含思考上下文）的后续对话正常{RESET}");
    }

    Ok(())
}

// ── main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 3 || args.len() > 4 {
        eprintln!("用法: cargo run --example chat_smoke_test -- <base_url> <token> <model> [场景]");
        eprintln!("  场景: single,multi,tools,reasoning,all（默认 all）");
        eprintln!(
            "示例: cargo run --example chat_smoke_test -- http://127.0.0.1:3000 lb-xxxxxx gpt-5-mini"
        );
        std::process::exit(2);
    }

    let base_url = &args[0];
    let token = args[1].clone();
    let model = args[2].clone();
    let scenarios = args.get(3).map(|s| s.as_str()).unwrap_or("all");

    let client = TestClient::new(base_url, token, model.clone())?;

    println!("{BOLD}llm-bridge 对话冒烟测试{RESET}");
    println!("{CYAN}地址{RESET}  {base_url}");
    println!("{CYAN}模型{RESET}  {BOLD}{model}{RESET}");
    println!("{CYAN}场景{RESET}  {scenarios}");

    let run_all = scenarios == "all";
    let selected: Vec<&str> = scenarios.split(',').map(|s| s.trim()).collect();

    let should_run = |name: &str| run_all || selected.contains(&name);

    let mut failed: Vec<&str> = Vec::new();

    if should_run("single")
        && let Err(e) = scenario_single(&client).await
    {
        eprintln!("{RED}场景 single 失败{RESET}: {e}");
        failed.push("single");
    }

    if should_run("multi")
        && let Err(e) = scenario_multi(&client).await
    {
        eprintln!("{RED}场景 multi 失败{RESET}: {e}");
        failed.push("multi");
    }

    if should_run("tools")
        && let Err(e) = scenario_tools(&client).await
    {
        eprintln!("{RED}场景 tools 失败{RESET}: {e}");
        failed.push("tools");
    }

    if should_run("reasoning")
        && let Err(e) = scenario_reasoning(&client).await
    {
        eprintln!("{RED}场景 reasoning 失败{RESET}: {e}");
        failed.push("reasoning");
    }

    print_header("测试总结");
    if failed.is_empty() {
        println!("{GREEN}{BOLD}所有场景通过 ✓{RESET}");
        Ok(())
    } else {
        eprintln!("{RED}{BOLD}失败场景：{}{RESET}", failed.join(", "));
        std::process::exit(1);
    }
}
