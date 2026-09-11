use axum::{
    Json, Router,
    body::Body,
    extract::State,
    response::{IntoResponse, Response},
    routing::post,
};
use futures_util::{SinkExt, StreamExt};
use llm_bridge::{
    actors::gateway_manager::{GatewayManagerActor, GatewayManagerArgs},
    auth::token::{CreateTokenRequest, create_token},
    config::models::{ApiKeyEntry, ProviderCompatibility, RuntimeSettings},
    db::{
        self,
        models::{LlmRequestTrace, TraceStatus, UsageRecord, User, UserRole},
    },
    observability::trace_writer::TraceWriter,
    server::{self, AppState},
    store::{ProtocolInput, Store},
};
use ractor::Actor;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Clone)]
struct Upstream {
    calls: Arc<AtomicUsize>,
    streams: Arc<AtomicUsize>,
}
struct StreamLease(Arc<AtomicUsize>);
impl Drop for StreamLease {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
async fn upstream(State(state): State<Upstream>, Json(request): Json<Value>) -> Response {
    state.calls.fetch_add(1, Ordering::SeqCst);
    let slow = request["messages"][0]["content"].as_str() == Some("slow");
    let flood = request["messages"][0]["content"].as_str() == Some("flood");
    state.streams.fetch_add(1, Ordering::SeqCst);
    let lease = StreamLease(state.streams);
    let stream = futures_util::stream::unfold((0, lease), move |(step, lease)| async move {
        let data = if flood {
            if step >= 4096 {
                return None;
            }
            json!({"choices":[{"delta":{"content":"x".repeat(65536)},"index":0}]}).to_string()
        } else {
            match step {
            0 => json!({"id":"upstream-id","choices":[{"delta":{"content":"hello"},"index":0}],"model":"upstream-model"}).to_string(),
            1 => {
                tokio::time::sleep(if slow { Duration::from_secs(60) } else { Duration::from_millis(100) }).await;
                json!({"choices":[],"usage":{"prompt_tokens":40,"completion_tokens":60,"total_tokens":100}}).to_string()
            }
            2 => json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}).to_string(),
            3 => "[DONE]".into(),
            _ => return None,
        }
        };
        Some((
            Ok::<_, std::io::Error>(format!("data: {data}\n\n")),
            (step + 1, lease),
        ))
    });
    (
        [("content-type", "text/event-stream")],
        Body::from_stream(stream),
    )
        .into_response()
}

struct Fixture {
    db: db::Db,
    url: String,
    token: String,
    upstream: Upstream,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    actor: ractor::ActorRef<llm_bridge::actors::gateway_manager::GatewayManagerMessage>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        self.actor.stop(None);
    }
}
impl Fixture {
    async fn new() -> Self {
        let db = db::init(db::all_models(), "sqlite::memory:").await.unwrap();
        let store = Arc::new(Store::new(db.clone()));
        let upstream_state = Upstream {
            calls: Arc::new(AtomicUsize::new(0)),
            streams: Arc::new(AtomicUsize::new(0)),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let upstream_router = Router::new()
            .route("/v1/chat/completions", post(upstream))
            .with_state(upstream_state.clone());
        let upstream_task = tokio::spawn(async move {
            axum::serve(listener, upstream_router).await.unwrap();
        });
        let provider = store
            .upsert_provider(
                "fixture".into(),
                "Fixture".into(),
                vec![ApiKeyEntry {
                    label: "test".into(),
                    key: "upstream-key".into(),
                    weight: 1,
                }],
                true,
                100,
                None,
                None,
            )
            .await
            .unwrap();
        let protocol = store
            .create_provider_protocol(
                provider.id,
                ProtocolInput {
                    id: None,
                    protocol: ProviderCompatibility::OpenAiChatCompletions,
                    base_url: upstream_url,
                    compat_settings: None,
                    enabled: true,
                    priority: 100,
                },
            )
            .await
            .unwrap();
        store
            .add_provider_model(
                provider.id,
                "fixture/model".into(),
                "upstream-model".into(),
                protocol.id,
                "Fixture".into(),
                None,
                8192,
                4096,
                true,
                false,
                false,
                false,
                Some(1.0),
                Some(2.0),
                Some(0.5),
            )
            .await
            .unwrap();
        let user = toasty::create!(User {
            oidc_sub: "fixture-user",
            name: "Fixture",
            role: UserRole::Member,
            active: true
        })
        .exec(&mut db.clone())
        .await
        .unwrap();
        let token = create_token(
            &db,
            user.id,
            CreateTokenRequest {
                name: "test".into(),
                allowed_models: vec![],
                request_quota: 0,
                token_quota: 0,
                quota_period: "unlimited".into(),
            },
        )
        .await
        .unwrap()
        .token;
        let settings = RuntimeSettings::from_env().unwrap();
        let (actor, _) = Actor::spawn(
            None,
            GatewayManagerActor,
            GatewayManagerArgs {
                settings,
                store: store.clone(),
                db: db.clone(),
            },
        )
        .await
        .unwrap();
        let state = AppState {
            gateway_manager: actor.clone(),
            store,
            auth_token: None,
            auth: None,
            db: db.clone(),
            trace_writer: TraceWriter::spawn(db.clone()),
            capture_content: true,
            public_base_url: "http://localhost".into(),
            catalog: Arc::new(server::models_dev::CatalogService::new(
                "http://localhost/catalog.json".into(),
            )),
        };
        let (router, _) = server::all_api_routes().build();
        let router = router.with_state(state).layer(axum::middleware::from_fn(
            llm_bridge::middleware::request_id::request_id_middleware,
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/v1/ws", listener.local_addr().unwrap());
        let server_task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            db,
            url,
            token,
            upstream: upstream_state,
            tasks: vec![upstream_task, server_task],
            actor,
        }
    }
    async fn socket(&self) -> Socket {
        let mut request = self.url.as_str().into_client_request().unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", self.token).parse().unwrap(),
        );
        connect_async(request).await.unwrap().0
    }
    async fn traces(&self, expected: usize) -> Vec<LlmRequestTrace> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let rows = LlmRequestTrace::all()
                    .exec(&mut self.db.clone())
                    .await
                    .unwrap();
                if rows.len() == expected && rows.iter().all(|row| row.status.is_final()) {
                    return rows;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("traces must reach a final state")
    }
}
async fn send(socket: &mut Socket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}
async fn next(socket: &mut Socket) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match socket.next().await.expect("connection alive").unwrap() {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(data) => socket.send(Message::Pong(data)).await.unwrap(),
                other => panic!("unexpected websocket frame {other:?}"),
            }
        }
    })
    .await
    .expect("response deadline")
}
fn chat(id: &str, text: &str) -> Value {
    json!({"method":"chat","id":id,"params":{"model":"fixture/model","messages":[{"role":"user","content":[{"type":"text","value":text}]}]}})
}

#[tokio::test]
async fn handshake_rejects_missing_bearer_and_bad_frames_do_not_break_connection() {
    let fixture = Fixture::new().await;
    let error = connect_async(fixture.url.as_str()).await.unwrap_err();
    assert!(
        matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 401)
    );
    let mut socket = fixture.socket().await;
    socket.send(Message::Text("{broken".into())).await.unwrap();
    assert_eq!(next(&mut socket).await["error"]["code"], "invalid_request");
    send(&mut socket, json!({"method":"listModels","id":"models"})).await;
    let response = next(&mut socket).await;
    assert_eq!(response["id"], "models");
    assert_eq!(response["result"][0]["name"], "fixture/model");
}

#[tokio::test]
async fn multiplexed_chats_settle_delayed_usage_before_done() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.socket().await;
    send(&mut socket, chat("a", "fast")).await;
    send(&mut socket, chat("b", "fast")).await;
    let mut done = std::collections::HashSet::new();
    let mut text = HashMap::<String, String>::new();
    while done.len() < 2 {
        let response = next(&mut socket).await;
        assert!(response.get("error").is_none(), "{response}");
        let id = response["id"].as_str().unwrap();
        assert!(matches!(id, "a" | "b"));
        if let Some(value) = response["chunk"].get("value").and_then(Value::as_str) {
            text.entry(id.into()).or_default().push_str(value);
        }
        if response.get("done").is_some() {
            done.insert(id.to_string());
        }
    }
    assert_eq!(text.get("a").unwrap(), "hello");
    assert_eq!(text.get("b").unwrap(), "hello");
    let usage = UsageRecord::all()
        .exec(&mut fixture.db.clone())
        .await
        .unwrap();
    assert_eq!((usage[0].request_count, usage[0].token_count), (2, 200));
    let traces = fixture.traces(2).await;
    for trace in traces {
        assert_eq!(trace.status, TraceStatus::Success);
        assert_eq!(trace.total_tokens, Some(100));
        assert!((trace.cost_usd.unwrap() - 0.00016).abs() < 1e-10);
        assert!(trace.response_parts.is_some());
    }
}

use std::collections::HashMap;
#[tokio::test]
async fn cancel_is_connection_scoped_and_stops_upstream() {
    let fixture = Fixture::new().await;
    let mut first = fixture.socket().await;
    let mut second = fixture.socket().await;
    send(&mut first, chat("slow", "slow")).await;
    assert!(next(&mut first).await.get("chunk").is_some());
    send(
        &mut second,
        json!({"method":"cancel","id":"foreign","targetId":"slow"}),
    )
    .await;
    assert_eq!(
        next(&mut second).await["error"]["code"],
        "request_not_found"
    );
    send(
        &mut first,
        json!({"method":"cancel","id":"cancel","targetId":"slow"}),
    )
    .await;
    let mut cancelled = false;
    let mut acknowledged = false;
    while !cancelled || !acknowledged {
        let response = next(&mut first).await;
        if response["id"] == "cancel" {
            acknowledged = response["result"]["cancelled"] == true;
        }
        if response["id"] == "slow" && response.get("done").is_some() {
            cancelled = response["done"]["cancelled"] == true;
        }
    }
    assert_eq!(fixture.traces(1).await[0].status, TraceStatus::Cancelled);
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.upstream.streams.load(Ordering::SeqCst) != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancel must close upstream response");
}

#[tokio::test]
async fn eight_active_chats_reject_ninth_and_disconnect_cancels_all() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.socket().await;
    for index in 0..8 {
        send(&mut socket, chat(&format!("slow-{index}"), "slow")).await;
    }
    let mut active = std::collections::HashSet::new();
    while active.len() < 8 {
        let message = next(&mut socket).await;
        assert!(message.get("error").is_none(), "{message}");
        if message.get("chunk").is_some() {
            active.insert(message["id"].as_str().unwrap().to_owned());
        }
    }
    send(&mut socket, chat("ninth", "slow")).await;
    let rejected = next(&mut socket).await;
    assert_eq!(rejected["id"], "ninth");
    assert_eq!(rejected["error"]["code"], "invalid_request");
    drop(socket);
    let traces = fixture.traces(8).await;
    assert!(
        traces
            .iter()
            .all(|trace| trace.status == TraceStatus::Cancelled)
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.upstream.streams.load(Ordering::SeqCst) != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let usage = UsageRecord::all()
        .exec(&mut fixture.db.clone())
        .await
        .unwrap();
    assert_eq!(usage[0].request_count, 8);
}

#[tokio::test]
async fn slow_consumer_closes_with_policy_code_and_cancels_upstream() {
    let fixture = Fixture::new().await;
    let mut socket = fixture.socket().await;
    send(&mut socket, chat("flood", "flood")).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let code = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match socket.next().await.expect("policy close frame").unwrap() {
                Message::Close(Some(frame)) => break u16::from(frame.code),
                Message::Ping(data) => socket.send(Message::Pong(data)).await.unwrap(),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(code, 1008);
    assert_eq!(fixture.traces(1).await[0].status, TraceStatus::Cancelled);
}

#[tokio::test]
async fn http_tool_history_and_model_contract_survive_route_fallback() {
    let fixture = Fixture::new().await;
    let failures = Arc::new(AtomicUsize::new(0));
    let calls = failures.clone();
    let router = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let failing = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let store = Store::new(fixture.db.clone());
    let provider = store
        .upsert_provider(
            "first".into(),
            "First".into(),
            vec![ApiKeyEntry {
                label: "fixture".into(),
                key: "fixture".into(),
                weight: 1,
            }],
            true,
            1,
            None,
            None,
        )
        .await
        .unwrap();
    let protocol = store
        .create_provider_protocol(
            provider.id,
            ProtocolInput {
                id: None,
                protocol: ProviderCompatibility::OpenAiChatCompletions,
                base_url,
                compat_settings: None,
                enabled: true,
                priority: 1,
            },
        )
        .await
        .unwrap();
    let link = store
        .add_provider_model(
            provider.id,
            "fixture/model".into(),
            "first-model".into(),
            protocol.id,
            "Fixture".into(),
            None,
            8192,
            4096,
            true,
            false,
            false,
            false,
            Some(1.0),
            Some(2.0),
            Some(0.5),
        )
        .await
        .unwrap();
    llm_bridge::db::models::ModelProvider::filter(
        llm_bridge::db::models::ModelProvider::fields()
            .id()
            .eq(link.id),
    )
    .update()
    .priority(1)
    .exec(&mut fixture.db.clone())
    .await
    .unwrap();
    let base = fixture
        .url
        .replace("ws://", "http://")
        .replace("/v1/ws", "");
    let client = llm_bridge::http::client_builder().build().unwrap();
    let models: Value = client
        .get(format!("{base}/v1/models"))
        .bearer_auth(&fixture.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(models["data"][0]["owned_by"].is_string());
    assert!(models["data"][0].get("ownedBy").is_none());
    let response = client.post(format!("{base}/v1/chat/completions")).bearer_auth(&fixture.token).json(&json!({
        "model":"fixture/model","messages":[
            {"role":"user","content":"fast"},
            {"role":"assistant","content":null,"tool_calls":[{"id":"tool-1","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
            {"role":"tool","tool_call_id":"tool-1","content":"result"}
        ]
    })).send().await.unwrap().error_for_status().unwrap();
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["choices"][0]["message"]["content"], "hello");
    assert_eq!(response["usage"]["total_tokens"], 100);
    assert_eq!(failures.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.upstream.calls.load(Ordering::SeqCst), 1);
    let usage = UsageRecord::all()
        .exec(&mut fixture.db.clone())
        .await
        .unwrap();
    assert_eq!((usage[0].request_count, usage[0].token_count), (1, 100));
    assert_eq!(fixture.traces(1).await[0].provider_id, "fixture");
    failing.abort();
}
