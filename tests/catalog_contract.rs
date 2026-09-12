use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use llm_bridge::server::models_dev::{Catalog, CatalogService};
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

#[test]
fn invalid_nominal_and_override_limits_report_the_affected_field() {
    let mut input = catalog();
    input.models[0].max_output_tokens = 0;
    let error = input.validate().unwrap_err();
    assert!(error.contains("acme/base.maxOutputTokens"), "{error}");
    assert!(error.contains("0"), "{error}");
    input.models[0].max_output_tokens = u32::MAX as i64;
    input.links[0].max_output_tokens = Some(u32::MAX as i64 + 1);
    let error = input.validate().unwrap_err();
    assert!(
        error.contains("alpha") && error.contains("maxOutputTokens"),
        "{error}"
    );
    input.links[0].max_output_tokens = None;
    input.validate().unwrap();
}
