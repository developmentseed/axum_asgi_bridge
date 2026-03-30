use axum::routing::get;
use axum::{Json, Router};
use axum_asgi_bridge::{AxumAsgiBridge, RouteRegistry};
use serde_json::json;

#[cfg(feature = "utoipa")]
use utoipa::OpenApi;

#[tokio::test]
async fn dispatch_structured_returns_bytes() {
    async fn handler() -> Json<serde_json::Value> {
        Json(json!({"ok": true}))
    }

    let router = Router::new().route("/", get(handler));
    let bridge = AxumAsgiBridge::new(router).with_route_patterns(["/".to_string()]);

    let result = bridge
        .dispatch("GET".into(), "/".into(), String::new(), vec![], Vec::new())
        .await
        .expect("dispatch succeeds");

    assert_eq!(result.status, 200);
    assert!(!result.headers.is_empty());
    assert!(
        String::from_utf8(result.body)
            .unwrap_or_default()
            .contains("ok")
    );
}

#[tokio::test]
async fn dispatch_structured_avoids_json() {
    async fn handler() -> Json<serde_json::Value> {
        Json(json!({"ok": true}))
    }

    let router = Router::new().route("/", get(handler));
    let bridge = AxumAsgiBridge::new(router).with_route_patterns(["/".to_string()]);

    let result = bridge
        .dispatch("GET".into(), "/".into(), String::new(), vec![], Vec::new())
        .await
        .expect("dispatch succeeds");

    assert_eq!(result.status, 200);
    assert!(
        String::from_utf8(result.body)
            .unwrap_or_default()
            .contains("ok")
    );
}

#[tokio::test]
async fn route_registry_tracks_patterns() {
    async fn handler() -> Json<serde_json::Value> {
        Json(json!({"ok": true}))
    }

    let bridge = RouteRegistry::new()
        .route("/", get(handler))
        .route("/items", get(handler))
        .into_bridge();

    let routes_json = bridge
        .provided_route_patterns_json()
        .expect("routes should serialize");
    let routes: Vec<String> = serde_json::from_str(&routes_json).expect("valid json");
    assert!(routes.contains(&"/".to_string()));
    assert!(routes.contains(&"/items".to_string()));
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn middleware_builders_dispatch() {
    async fn handler() -> Json<serde_json::Value> {
        Json(json!({"ok": true}))
    }

    let router = Router::new().route("/", get(handler));
    let bridge = AxumAsgiBridge::new(router)
        .with_compression()
        .with_cors_permissive()
        .with_timeout(std::time::Duration::from_secs(1))
        .with_trace_http()
        .with_route_patterns(["/".to_string()]);

    let result = bridge
        .dispatch("GET".into(), "/".into(), String::new(), vec![], Vec::new())
        .await
        .expect("dispatch succeeds");
    assert_eq!(result.status, 200);
}

#[cfg(feature = "utoipa")]
#[derive(OpenApi)]
#[openapi()]
struct EmptyDoc;

#[cfg(feature = "utoipa")]
#[tokio::test]
async fn utoipa_schema_builder_populates_schema() {
    let bridge = RouteRegistry::new()
        .route("/", get(|| async { "ok" }))
        .into_bridge()
        .with_utoipa_schema::<EmptyDoc>();

    let schema = bridge
        .openapi_schema_json()
        .expect("schema serialization should succeed");
    assert!(schema.is_some());
}
