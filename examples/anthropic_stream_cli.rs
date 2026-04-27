use std::env;
use std::io::{self, Write};

use tokio::sync::mpsc;

#[path = "../src/types.rs"]
mod types;

mod actors {
    pub mod provider {
        use tokio::sync::mpsc;

        use crate::types::{LMResponsePart, LanguageModelChatMessage};

        pub type ProviderStreamItem = Result<LMResponsePart, String>;
        pub type ProviderResponseSender = mpsc::Sender<ProviderStreamItem>;

        #[derive(Debug, Clone)]
        pub struct ProviderState {
            pub provider_id: String,
            pub compatibility: String,
            pub api_key: String,
            pub base_url: Option<String>,
            pub compat_settings: Option<CompatSettings>,
            pub client: reqwest::Client,
        }

        #[derive(Debug, Clone)]
        pub struct CompatSettings {
            pub path_suffix: Option<String>,
            pub custom_headers: std::collections::HashMap<String, String>,
            pub custom_params: std::collections::HashMap<String, String>,
        }

        #[derive(Debug, Clone)]
        pub struct ProviderChatRequest {
            pub model: String,
            pub messages: Vec<LanguageModelChatMessage>,
        }

        pub mod adapters {
            pub mod anthropic {
                include!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/src/actors/provider/adapters/anthropic_messages.rs"
                ));
            }
        }
    }
}

use actors::provider::adapters::anthropic;
use actors::provider::{ProviderChatRequest, ProviderState};
use types::{
    LMResponsePart, LanguageModelChatMessage, LanguageModelInputPart, LanguageModelTextPart,
    LanguageModelThinkingValue,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!("Usage: cargo run --example anthropic_stream_cli -- <url> <api_key> <model>");
        eprintln!(
            "Example: cargo run --example anthropic_stream_cli -- https://api.anthropic.com/v1 sk-ant-xxx claude-opus-4-5"
        );
        std::process::exit(2);
    }

    let url = args[0].clone();
    let api_key = args[1].clone();
    let model = args[2].clone();

    let prompt = "用一句话解释 Rust 的所有权机制。".to_string();

    let request = ProviderChatRequest {
        model,
        messages: vec![LanguageModelChatMessage::user(
            vec![LanguageModelInputPart::Text(LanguageModelTextPart {
                value: prompt,
            })],
            None,
        )],
    };

    let state = ProviderState {
        provider_id: "anthropic-example".to_string(),
        compatibility: "anthropic".to_string(),
        api_key,
        base_url: Some(url),
        compat_settings: None,
        client: reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?,
    };

    let (tx, mut rx) = mpsc::channel(64);
    tokio::spawn(async move {
        if let Err(error) = anthropic::stream_chat(&state, request, tx.clone()).await {
            let _ = tx.send(Err(error)).await;
        }
    });

    let mut saw_text = false;
    let mut saw_thinking = false;

    while let Some(item) = rx.recv().await {
        match item {
            Ok(LMResponsePart::Text(part)) => {
                if !saw_text {
                    println!("\n[TEXT]");
                    saw_text = true;
                }
                print!("{}", part.value);
                io::stdout().flush()?;
            }
            Ok(LMResponsePart::Thinking(part)) => {
                if !saw_thinking {
                    println!("\n[THINK]");
                    saw_thinking = true;
                }
                match part.value {
                    LanguageModelThinkingValue::String(value) => {
                        print!("{}", value);
                    }
                    LanguageModelThinkingValue::Array(values) => {
                        for value in values {
                            println!("{}", value);
                        }
                    }
                }
                io::stdout().flush()?;
            }
            Ok(LMResponsePart::ToolCall(call)) => {
                println!(
                    "\n[TOOL CALL] {} ({}): {}",
                    call.name, call.call_id, call.input
                );
            }
            Ok(other) => {
                println!("\n[OTHER] {:?}", other);
            }
            Err(error) => {
                return Err(error.into());
            }
        }
    }

    if saw_text || saw_thinking {
        println!();
    }

    Ok(())
}
