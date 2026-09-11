use axum::{
    Router,
    body::{Body, to_bytes},
    extract::Path,
    http::{Request, StatusCode},
    routing::get,
};
use llm_bridge::{
    actors::gateway_manager::{GatewayManagerActor, GatewayManagerArgs},
    auth::{
        session::SessionUser,
        token::{CreateTokenRequest, create_token},
    },
    config::models::RuntimeSettings,
    db::{
        self,
        models::{LlmRequestTrace, TraceInterface, TraceStatus, UsageDaily, User, UserRole},
    },
    observability::trace_writer::TraceWriter,
    server::{self, AppState},
    store::Store,
};
use ractor::Actor;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

async fn request(router: &Router, path: &str, cookie: Option<&str>) -> axum::response::Response {
    let mut builder = Request::builder().uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    router
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
}
async fn body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn member_ownership_and_live_demotion_apply_to_existing_session_cookies() {
    let db = db::init(db::all_models(), "sqlite::memory:").await.unwrap();
    let mut users = Vec::new();
    for (name, role) in [
        ("alice", UserRole::Member),
        ("bob", UserRole::Member),
        ("admin", UserRole::Admin),
    ] {
        users.push(
            toasty::create!(User {
                oidc_sub: name,
                name,
                role,
                active: true
            })
            .exec(&mut db.clone())
            .await
            .unwrap(),
        );
    }
    for (index, user) in users.iter().take(2).enumerate() {
        let token = create_token(
            &db,
            user.id,
            CreateTokenRequest {
                name: "fixture".into(),
                allowed_models: vec![],
                request_quota: 0,
                token_quota: 0,
                quota_period: "unlimited".into(),
            },
        )
        .await
        .unwrap();
        toasty::create!(LlmRequestTrace {
            request_id: user.name.clone(),
            interface: TraceInterface::OpenAiHttp,
            token_id: token.id,
            user_id: user.id,
            token_prefix: "fixture",
            model: "fixture/model",
            provider_id: "fixture",
            provider_model_id: "upstream",
            protocol: "openai",
            status: TraceStatus::Success,
            estimated_tokens: 1,
            total_tokens: Some((index as u64 + 1) * 100),
            response_parts: Some(toasty::Json(
                serde_json::from_value::<Vec<llm_bridge::types::LMResponsePart>>(
                    json!([{"type":"text","value":format!("secret-{}", user.name)}])
                )
                .unwrap()
            )),
        })
        .exec(&mut db.clone())
        .await
        .unwrap();
        toasty::create!(UsageDaily {
            day: llm_bridge::observability::trace_writer::current_day(),
            token_id: token.id,
            model: "fixture/model",
            request_count: 1,
            input_tokens: 40,
            output_tokens: 60,
            reasoning_tokens: 0,
            cached_tokens: 0,
            total_tokens: (index as i64 + 1) * 100,
            cost_usd: index as f64 + 1.0,
        })
        .exec(&mut db.clone())
        .await
        .unwrap();
    }
    let store = Arc::new(Store::new(db.clone()));
    let (actor, _) = Actor::spawn(
        None,
        GatewayManagerActor,
        GatewayManagerArgs {
            settings: RuntimeSettings::from_env().unwrap(),
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
    let (routes, _) = server::all_api_routes().build();
    // 仅测试夹具路由签入已有用户，并故意缓存 admin 来验证提取器不信任 Session 角色。
    let router = routes
        .route(
            "/fixture/login/{id}",
            get(|Path(id): Path<u64>, session: Session| async move {
                session
                    .insert(
                        "user",
                        SessionUser {
                            user_id: id,
                            name: "stale".into(),
                            role: "admin".into(),
                        },
                    )
                    .await
                    .unwrap();
                StatusCode::NO_CONTENT
            }),
        )
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false));
    let mut cookies = Vec::new();
    for user in &users {
        let response = request(&router, &format!("/fixture/login/{}", user.id), None).await;
        cookies.push(
            response.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned(),
        );
    }
    for (index, name) in ["alice", "bob"].iter().enumerate() {
        let cookie = Some(cookies[index].as_str());
        assert_eq!(
            request(&router, "/api/v1/admin/models", cookie)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        let me = body(request(&router, "/auth/me", cookie).await).await;
        assert_eq!(me["role"], "member");
        let summary = body(request(&router, "/api/v1/usage/summary", cookie).await).await;
        assert_eq!(summary["totalTokens"], (index + 1) * 100);
        let traces = body(request(&router, "/api/v1/usage/traces?pageSize=1", cookie).await).await;
        assert_eq!(traces["total"], 1);
        assert_eq!(traces["items"][0]["requestId"], *name);
        let own =
            body(request(&router, &format!("/api/v1/usage/traces/{name}"), cookie).await).await;
        assert_eq!(own["responseParts"][0]["value"], format!("secret-{name}"));
        let foreign = if index == 0 { "bob" } else { "alice" };
        assert_eq!(
            request(&router, &format!("/api/v1/usage/traces/{foreign}"), cookie)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let admin = Some(cookies[2].as_str());
    assert_eq!(
        request(&router, "/api/v1/admin/models", admin)
            .await
            .status(),
        StatusCode::OK
    );
    let summary = body(request(&router, "/api/v1/usage/summary", admin).await).await;
    assert_eq!(summary["totalTokens"], 300);
    let page = body(request(&router, "/api/v1/usage/traces?pageSize=1&page=1", admin).await).await;
    assert_eq!(page["total"], 2);
    assert_eq!(page["items"][0]["requestId"], "alice");
    User::filter(User::fields().id().eq(users[2].id))
        .update()
        .role(UserRole::Member)
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert_eq!(
        request(&router, "/api/v1/admin/models", admin)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    User::filter(User::fields().id().eq(users[0].id))
        .update()
        .active(false)
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert_eq!(
        request(&router, "/api/v1/usage/summary", Some(&cookies[0]))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    actor.stop(None);
}
