use std::env;
use std::io::{self, Write};

use tokio::sync::mpsc;

use llm_bridge::actors::provider::adapters::openai_chat_completions as openai;
use llm_bridge::actors::provider::{ProviderChatRequest, ProviderState};
use llm_bridge::config::models::ProviderCompatibility;
use llm_bridge::types::{
    LMResponsePart, LanguageModelChatMessage, LanguageModelInputPart, LanguageModelTextPart,
    LanguageModelThinkingValue,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!("Usage: cargo run --example openai_stream_cli -- <url> <api_key> <model>");
        eprintln!(
            "Example: cargo run --example openai_stream_cli -- https://api.openai.com/v1 sk-xxx gpt-5-mini"
        );
        std::process::exit(2);
    }

    let url = args[0].clone();
    let api_key = args[1].clone();
    let model = args[2].clone();

    let prompt = "请先输出简短思考，再回答：用一句话解释什么是 Rust 的所有权。".to_string();

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
        provider_id: "openai-example".to_string(),
        compatibility: ProviderCompatibility::OpenAiChatCompletions,
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
        if let Err(error) = openai::stream_chat(&state, request, tx.clone()).await {
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
