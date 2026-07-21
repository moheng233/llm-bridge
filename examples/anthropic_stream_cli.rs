use std::env;
use std::io::{self, Write};

use tokio::sync::mpsc;

use llm_bridge::actors::provider::adapters::anthropic_messages as anthropic;
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
        tools: None,
        tool_choice: None,
        temperature: None,
        max_tokens: None,
        top_p: None,
        stop: None,
        response_format: None,
        reasoning: None,
        seed: None,
        frequency_penalty: None,
        presence_penalty: None,
        logit_bias: None,
        max_completion_tokens: None,
    };

    let state = ProviderState {
        provider_id: "anthropic-example".to_string(),
        compatibility: ProviderCompatibility::AnthropicMessages,
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
        let (metadata_tx, _metadata_rx) =
            tokio::sync::oneshot::channel::<llm_bridge::actors::provider::ProviderResponseMetadata>();
        let (started_tx, _started_rx) =
            tokio::sync::oneshot::channel::<llm_bridge::actors::provider::ProviderStartSignal>();
        if let Err(error) =
            anthropic::stream_chat(&state, request, tx.clone(), metadata_tx, started_tx).await
        {
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
