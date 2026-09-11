use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use llm_bridge::{
    db::{
        self,
        models::{LLMModel, ModelProvider, Provider},
    },
    server::models_dev::{
        Catalog, CatalogSelection, CatalogService, import_catalog, preview_catalog,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn catalog() -> Catalog {
    serde_json::from_value(serde_json::json!({
        "schemaVersion":1,"sourceRev":"fixture-revision","generatedAt":"2026-09-11T00:00:00Z",
        "models":[{"modelName":"acme/base","displayName":"Base","maxInputTokens":100000,"maxOutputTokens":40000,"toolCalling":true}],
        "providers":[{"providerId":"acme","displayName":"Acme","baseUrl":"https://acme.example/v1","compat":"openAiChatCompletions"}],
        "links":[
            {"providerId":"acme","protocolKey":"openAiChatCompletions|https://acme.example/v1","modelName":"acme/base","providerModelId":"alpha","maxOutputTokens":8000,"inputPricePer1m":1.5,"enabled":true},
            {"providerId":"acme","protocolKey":"openAiChatCompletions|https://acme.example/v1","modelName":"acme/base","providerModelId":"beta","vision":true,"enabled":true}
        ]
    })).unwrap()
}
fn selection(catalog: &Catalog) -> CatalogSelection {
    CatalogSelection {
        models: vec![],
        providers: vec![],
        links: catalog.links.iter().map(|link| link.key()).collect(),
    }
}

#[tokio::test]
async fn reimport_preserves_aliases_keys_priorities_and_nominal_capabilities() {
    let db = db::init(db::all_models(), "sqlite::memory:").await.unwrap();
    let mut catalog = catalog();
    let first = import_catalog(&db, &catalog, &selection(&catalog))
        .await
        .unwrap();
    assert_eq!(first.created, 5);
    let mut provider = Provider::all()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .remove(0);
    let key = llm_bridge::config::models::ApiKeyEntry {
        label: "private".into(),
        key: "sk-do-not-overwrite".into(),
        weight: 7,
    };
    Provider::filter(Provider::fields().id().eq(provider.id))
        .update()
        .api_keys(toasty::Json(vec![key]))
        .priority(9)
        .enabled(false)
        .exec(&mut db.clone())
        .await
        .unwrap();
    catalog.links[0].input_price_per_1m = Some(2.0);
    let second = import_catalog(&db, &catalog, &selection(&catalog))
        .await
        .unwrap();
    assert_eq!((second.created, second.updated), (0, 5));
    provider = Provider::get_by_id(&mut db.clone(), &provider.id)
        .await
        .unwrap();
    assert_eq!(provider.api_keys.0[0].key, "sk-do-not-overwrite");
    assert_eq!(provider.api_keys.0[0].weight, 7);
    assert_eq!(provider.priority, 9);
    assert!(!provider.enabled);
    let links = ModelProvider::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(
        links
            .iter()
            .find(|row| row.provider_model_id == "alpha")
            .unwrap()
            .input_price_per_1m,
        Some(2.0)
    );
    assert_eq!(
        links
            .iter()
            .find(|row| row.provider_model_id == "alpha")
            .unwrap()
            .max_output_tokens,
        Some(8000)
    );
    assert_eq!(
        links
            .iter()
            .find(|row| row.provider_model_id == "beta")
            .unwrap()
            .vision,
        Some(true)
    );
    let models = LLMModel::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].max_output_tokens, 40000);
    assert!(!models[0].vision);
    let preview = preview_catalog(&db, &catalog).await.unwrap();
    assert!(preview.models.iter().all(|row| row.exists));
    assert!(preview.providers.iter().all(|row| row.exists));
    assert!(preview.links.iter().all(|row| row.exists));
}

#[tokio::test]
async fn failed_link_insert_rolls_back_every_catalog_layer() {
    let db = db::init(db::all_models(), "sqlite::memory:").await.unwrap();
    toasty::sql::statement("CREATE TRIGGER reject_beta BEFORE INSERT ON model_providers WHEN NEW.provider_model_id = 'beta' BEGIN SELECT RAISE(ABORT, 'fixture rejects beta'); END").exec(&mut db.clone()).await.unwrap();
    let catalog = catalog();
    assert!(
        import_catalog(&db, &catalog, &selection(&catalog))
            .await
            .is_err()
    );
    assert!(
        Provider::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        LLMModel::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ModelProvider::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn conditional_fetch_reuses_validated_body() {
    let conditional = Arc::new(AtomicUsize::new(0));
    let observed = conditional.clone();
    let app = Router::new()
        .route(
            "/contract.json",
            get(|| async { Json(serde_json::json!({"schemaVersion":1})) }),
        )
        .route(
            "/catalog.json",
            get(move |headers: HeaderMap| {
                let observed = observed.clone();
                async move {
                    if headers
                        .get("if-none-match")
                        .is_some_and(|value| value == "\"fixture\"")
                    {
                        observed.fetch_add(1, Ordering::SeqCst);
                        StatusCode::NOT_MODIFIED.into_response()
                    } else {
                        let mut response: Response = Json(catalog()).into_response();
                        response
                            .headers_mut()
                            .insert("etag", "\"fixture\"".parse().unwrap());
                        response
                    }
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = CatalogService::new(format!(
        "http://{}/catalog.json",
        listener.local_addr().unwrap()
    ));
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let first = service.load().await.unwrap();
    let second = service.load().await.unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(conditional.load(Ordering::SeqCst), 1);
    server.abort();
}
