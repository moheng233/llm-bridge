use axfetchum::GeneratorConfig;
use llm_bridge::server::admin::all_routes;

fn config() -> GeneratorConfig {
    GeneratorConfig {
        bindings_dir: "frontend/src/bindings/".into(),
        output_path: "frontend/src/bindings/client.ts".into(),
        factory_name: "createApiClient".into(),
        error_class_name: "ApiError".into(),
        options_interface_name: "ApiClientOptions".into(),
        type_import_prefix: "./".into(),
        ..Default::default()
    }
}

#[test]
fn generate_ts_client() {
    let (_router, routes) = all_routes();
    axfetchum::generate_to_file(&routes, &config()).unwrap();
}

// #[test]
// fn check_ts_client_up_to_date() {
//     let (_router, routes) = all_routes();
//     axfetchum::check(&routes, &config())
//         .expect("Generated TypeScript client is out of date! Run: cargo test generate_ts_client");
// }
