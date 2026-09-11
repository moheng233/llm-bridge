//! cargo run --example ws_chat_client -- ws://127.0.0.1:3000/v1/ws MODEL [PROMPT]
//! Set LLM_BRIDGE_API_KEY to a gateway token; use wss:// outside a trusted local network.
use anyhow::{Context, bail};
use futures_util::{SinkExt, StreamExt};
use llm_bridge::types::WsServerMessage;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    llm_bridge::http::ensure_crypto_provider();
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .context("usage: ws_chat_client URL MODEL [PROMPT]; set LLM_BRIDGE_API_KEY")?;
    let model = args.next().context("missing model")?;
    let prompt = args.next().unwrap_or_else(|| "Hello".into());
    let token = std::env::var("LLM_BRIDGE_API_KEY").context("set LLM_BRIDGE_API_KEY")?;
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert("Authorization", format!("Bearer {token}").parse()?);
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "lm-bridge.v1".parse()?);
    let (mut socket, _) = connect_async(request).await?;
    let chat = serde_json::json!({"id":"chat-1", "method":"chat", "params":{
        "model":model, "messages":[{"role":"user", "content":[{"value":prompt}]}]
    }});
    socket.send(Message::Text(chat.to_string().into())).await?;
    while let Some(message) = socket.next().await {
        match message? {
            Message::Text(text) => {
                println!("{text}");
                match serde_json::from_str::<WsServerMessage>(&text)? {
                    WsServerMessage::Done { id, .. } if id == "chat-1" => {
                        socket.close(None).await?;
                        return Ok(());
                    }
                    WsServerMessage::Error { error, .. } => {
                        bail!("{:?}: {}", error.code, error.message)
                    }
                    _ => {}
                }
            }
            // tungstenite queues the matching Pong when it reads a Ping.
            Message::Ping(_) => socket.flush().await?,
            Message::Close(_) => break,
            _ => {}
        }
    }
    bail!("connection ended before chat completion")
}
