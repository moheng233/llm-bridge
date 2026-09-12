use axum::{
    Router,
    body::{Body, to_bytes},
    extract::Path,
    http::{Request, StatusCode},
    routing::get,
};
use llm_bridge::{
    actors::gateway_manager::{GatewayManagerActor, GatewayManagerArgs},
    auth::session::SessionUser,
    config::models::RuntimeSettings,
    db::{
        self,
        models::{LLMModel, ModelProvider, Provider, ProviderProtocol, User, UserRole},
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

struct Fixture {
    router: Router,
    db: db::Db,
    admin: String,
    member: String,
    actor: ractor::ActorRef<llm_bridge::actors::gateway_manager::GatewayManagerMessage>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.actor.stop(None);
    }
}
async fn request(
    router: &Router,
    method: &str,
    path: &str,
    cookie: &str,
    input: Value,
) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    )
}
async fn fixture() -> Fixture {
    let db = db::init(db::all_models(), "sqlite::memory:").await.unwrap();
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
    let router = routes
        .route(
            "/fixture/login/{id}",
            get(|Path(id): Path<u64>, session: Session| async move {
                session
                    .insert(
                        "user",
                        SessionUser {
                            user_id: id,
                            name: "fixture".into(),
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
    for (name, role) in [("admin", UserRole::Admin), ("member", UserRole::Member)] {
        let user = toasty::create!(User {
            oidc_sub: name,
            name,
            role,
            active: true
        })
        .exec(&mut db.clone())
        .await
        .unwrap();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/fixture/login/{}", user.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
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
    Fixture {
        router,
        db,
        admin: cookies.remove(0),
        member: cookies.remove(0),
        actor,
    }
}
fn provider_input(id: &str) -> Value {
    json!({"providerId":id,"displayName":"Acme","apiKeys":[{"label":"key-1","key":"fake-original-key","weight":1}],"protocols":[{"protocol":"openAiChatCompletions","baseUrl":"http://127.0.0.1:5303/v1","enabled":true,"priority":100}],"enabled":true,"priority":100,"quotaAdapter":null,"quotaAdapterConfig":null})
}
fn model_input(name: &str) -> Value {
    json!({"modelName":name,"displayName":"Base","maxInputTokens":100000,"maxOutputTokens":40000,"toolCalling":true,"vision":false,"thinking":false,"adaptiveThinking":false})
}
fn batch(provider: &Value, model: Value) -> Value {
    let base = json!({"providerId":provider["id"],"protocolId":provider["protocols"][0]["id"],"enabled":true,"priority":100});
    let mut alpha = base.clone();
    alpha["providerModelId"] = json!("alpha");
    alpha["maxOutputTokens"] = json!(8000);
    alpha["inputPricePer1m"] = json!(1.5);
    let mut beta = base;
    beta["providerModelId"] = json!("beta");
    beta["vision"] = json!(true);
    json!({"items":[{"model":model,"link":alpha},{"model":model,"link":beta}]})
}
async fn create_provider(f: &Fixture, id: &str) -> Value {
    let (status, body) = request(
        &f.router,
        "POST",
        "/api/v1/admin/providers",
        &f.admin,
        provider_input(id),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

#[tokio::test]
async fn provider_conflict_and_protocol_failure_preserve_atomic_configuration() {
    let f = fixture().await;
    let provider = create_provider(&f, "acme").await;
    let mut duplicate = provider_input("acme");
    duplicate["displayName"] = json!("Overwrite");
    duplicate["apiKeys"][0]["key"] = json!("different");
    duplicate["protocols"][0]["baseUrl"] = json!("http://different/v1");
    let (status, error) = request(
        &f.router,
        "POST",
        "/api/v1/admin/providers",
        &f.admin,
        duplicate,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"], "provider_id_exists");
    let (_, current) = request(
        &f.router,
        "GET",
        &format!("/api/v1/admin/providers/{}", provider["id"]),
        &f.admin,
        Value::Null,
    )
    .await;
    assert_eq!(current, provider);
    let stored = Provider::all().exec(&mut f.db.clone()).await.unwrap();
    assert_eq!(stored[0].api_keys[0].key, "fake-original-key");
    toasty::sql::statement("CREATE TRIGGER reject_protocol BEFORE INSERT ON provider_protocols WHEN NEW.base_url = 'http://reject/v1' BEGIN SELECT RAISE(ABORT, 'fixture protocol failure'); END").exec(&mut f.db.clone()).await.unwrap();
    let mut rejected = provider_input("rejected");
    rejected["protocols"][0]["baseUrl"] = json!("http://reject/v1");
    assert_eq!(
        request(
            &f.router,
            "POST",
            "/api/v1/admin/providers",
            &f.admin,
            rejected
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        Provider::all().exec(&mut f.db.clone()).await.unwrap().len(),
        1
    );
    let mut update = provider_input("acme");
    update["displayName"] = json!("Rejected update");
    update["protocols"][0]["baseUrl"] = json!("http://reject/v1");
    assert_eq!(
        request(
            &f.router,
            "PUT",
            &format!("/api/v1/admin/providers/{}", provider["id"]),
            &f.admin,
            update
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let (_, current) = request(
        &f.router,
        "GET",
        &format!("/api/v1/admin/providers/{}", provider["id"]),
        &f.admin,
        Value::Null,
    )
    .await;
    assert_eq!(current, provider);
    let mut update = provider_input("acme");
    update["protocols"][0]["id"] = provider["protocols"][0]["id"].clone();
    update["protocols"][0]["baseUrl"] = json!("http://changed/v1");
    update["apiKeys"][0]["key"] = json!("");
    let (status, current) = request(
        &f.router,
        "PUT",
        &format!("/api/v1/admin/providers/{}", provider["id"]),
        &f.admin,
        update,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        current["protocols"][0]["id"],
        provider["protocols"][0]["id"]
    );
    assert_eq!(
        ProviderProtocol::all()
            .exec(&mut f.db.clone())
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        Provider::all().exec(&mut f.db.clone()).await.unwrap()[0].api_keys[0].key,
        "fake-original-key"
    );
}

#[tokio::test]
async fn aliases_share_nominal_definition_and_reuse_never_overwrites_local_fields() {
    let f = fixture().await;
    let provider = create_provider(&f, "acme").await;
    let input = batch(
        &provider,
        json!({"kind":"new","model":model_input("acme/base")}),
    );
    let (status, result) = request(
        &f.router,
        "POST",
        "/api/v1/admin/model-connections",
        &f.admin,
        input.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["items"][0]["modelCreated"], true);
    assert_eq!(result["items"][1]["modelCreated"], false);
    assert_eq!(result["items"][0]["modelId"], result["items"][1]["modelId"]);
    assert_eq!(result["items"][0]["link"]["providerModelId"], "alpha");
    assert_eq!(result["items"][0]["link"]["maxOutputTokens"], 8000);
    assert_eq!(result["items"][1]["link"]["vision"], true);
    let models = LLMModel::all().exec(&mut f.db.clone()).await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].max_output_tokens, 40000);
    assert!(!models[0].vision);
    let model_id = result["items"][0]["modelId"].as_u64().unwrap();
    let link_id = result["items"][0]["link"]["id"].as_u64().unwrap();
    ModelProvider::filter(ModelProvider::fields().id().eq(link_id))
        .update()
        .enabled(false)
        .input_price_per_1m(Some(9.0))
        .exec(&mut f.db.clone())
        .await
        .unwrap();
    let mut nominal = model_input("acme/base");
    nominal["displayName"] = json!("Local");
    nominal["maxOutputTokens"] = json!(41001);
    nominal["vision"] = json!(true);
    nominal["description"] = json!("Local description");
    assert_eq!(
        request(
            &f.router,
            "PUT",
            &format!("/api/v1/admin/models/{model_id}"),
            &f.admin,
            nominal.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, reused) = request(
        &f.router,
        "POST",
        "/api/v1/admin/model-connections",
        &f.admin,
        batch(&provider, json!({"kind":"existing","id":model_id})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reused["items"][0]["linkCreated"], false);
    assert_eq!(reused["items"][0]["link"]["enabled"], false);
    assert_eq!(reused["items"][0]["link"]["inputPricePer1m"], 9.0);
    assert_eq!(
        ModelProvider::all()
            .exec(&mut f.db.clone())
            .await
            .unwrap()
            .len(),
        2
    );
    let (_, current) = request(
        &f.router,
        "GET",
        &format!("/api/v1/admin/models/{model_id}"),
        &f.admin,
        Value::Null,
    )
    .await;
    assert_eq!(current["displayName"], "Local");
    assert_eq!(current["maxOutputTokens"], 41001);
    assert_eq!(current["vision"], true);
    let mut rename = nominal;
    rename["modelName"] = json!("renamed");
    let (status, error) = request(
        &f.router,
        "PUT",
        &format!("/api/v1/admin/models/{model_id}"),
        &f.admin,
        rename,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "model_name_is_immutable");
    assert_eq!(
        request(
            &f.router,
            "POST",
            "/api/v1/admin/models",
            &f.admin,
            model_input("acme/base")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let mut conflict = input.clone();
    conflict["items"][0]["model"]["model"]["modelName"] = json!("new-before-conflict");
    assert_eq!(
        request(
            &f.router,
            "POST",
            "/api/v1/admin/model-connections",
            &f.admin,
            conflict
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        LLMModel::all().exec(&mut f.db.clone()).await.unwrap().len(),
        1
    );
    let mut conflict = input;
    conflict["items"][0]["model"]["model"]["displayName"] = json!("different draft");
    let (status, error) = request(
        &f.router,
        "POST",
        "/api/v1/admin/model-connections",
        &f.admin,
        conflict,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "conflicting_model_definitions");
}

#[tokio::test]
async fn second_alias_failure_rolls_back_model_and_first_link_but_preserves_provider() {
    let f = fixture().await;
    let provider = create_provider(&f, "acme").await;
    toasty::sql::statement("CREATE TRIGGER reject_beta BEFORE INSERT ON model_providers WHEN NEW.provider_model_id = 'beta' BEGIN SELECT RAISE(ABORT, 'fixture rejects beta'); END").exec(&mut f.db.clone()).await.unwrap();
    let (status, _) = request(
        &f.router,
        "POST",
        "/api/v1/admin/model-connections",
        &f.admin,
        batch(
            &provider,
            json!({"kind":"new","model":model_input("acme/base")}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        LLMModel::all()
            .exec(&mut f.db.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ModelProvider::all()
            .exec(&mut f.db.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        Provider::all().exec(&mut f.db.clone()).await.unwrap().len(),
        1
    );
    assert_eq!(
        ProviderProtocol::all()
            .exec(&mut f.db.clone())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn batch_requires_admin_and_valid_local_ownership_without_partial_writes() {
    let f = fixture().await;
    let provider = create_provider(&f, "acme").await;
    let other = create_provider(&f, "other").await;
    let input = batch(
        &provider,
        json!({"kind":"new","model":model_input("acme/base")}),
    );
    assert_eq!(
        request(
            &f.router,
            "POST",
            "/api/v1/admin/model-connections",
            &f.member,
            input.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, error) = request(
        &f.router,
        "POST",
        "/api/v1/admin/model-connections",
        &f.admin,
        json!({"items":[]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "empty_selection");
    let mut mismatch = input.clone();
    mismatch["items"][1]["link"]["protocolId"] = other["protocols"][0]["id"].clone();
    let (status, error) = request(
        &f.router,
        "POST",
        "/api/v1/admin/model-connections",
        &f.admin,
        mismatch,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["itemIndex"], 1);
    assert_eq!(error["field"], "protocolId");
    let mut missing = input.clone();
    missing["items"][1]["model"] = json!({"kind":"existing","id":999999});
    assert_eq!(
        request(
            &f.router,
            "POST",
            "/api/v1/admin/model-connections",
            &f.admin,
            missing
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let mut invalid = input;
    invalid["items"][1]["link"]["inputPricePer1m"] = json!(-1);
    let (status, error) = request(
        &f.router,
        "POST",
        "/api/v1/admin/model-connections",
        &f.admin,
        invalid,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["field"], "inputPricePer1m");
    assert!(
        LLMModel::all()
            .exec(&mut f.db.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ModelProvider::all()
            .exec(&mut f.db.clone())
            .await
            .unwrap()
            .is_empty()
    );
}
