use axum::Router;
use gateway::{
    config::{GatewayConfig, RouteConfig, ServerConfig},
    proxy::ProxyState,
    router::Router as GatewayRouter,
};
use http::{Request, StatusCode};
use std::time::Duration;
use tower::ServiceExt;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

/// Helper function to create a test gateway with mock backends
async fn setup_test_gateway() -> (ProxyState, MockServer) {
    let mock_server = MockServer::start().await;

    // Setup mock responses
    Mock::given(method("GET"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "users": ["Alice", "Bob"]
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/users/123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 123,
            "name": "Alice"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/users"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 456,
            "name": "Charlie"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    // Create routes pointing to mock server
    let routes = vec![
        RouteConfig {
            path: "/api/users".to_string(),
            backend: mock_server.uri(),
            methods: vec!["GET".to_string(), "POST".to_string()],
            strip_prefix: false,
            description: "User service".to_string(),
        },
        RouteConfig {
            path: "/api/users/:id".to_string(),
            backend: mock_server.uri(),
            methods: vec!["GET".to_string()],
            strip_prefix: false,
            description: "Get user by ID".to_string(),
        },
        RouteConfig {
            path: "/health".to_string(),
            backend: mock_server.uri(),
            methods: vec![],
            strip_prefix: false,
            description: "Health check".to_string(),
        },
    ];

    let router = GatewayRouter::new(routes).unwrap();
    let proxy_state = ProxyState::new(router, Duration::from_secs(30));

    (proxy_state, mock_server)
}

#[tokio::test]
async fn test_basic_proxy() {
    let (proxy_state, _mock_server) = setup_test_gateway().await;

    let app = Router::new()
        .route("/*path", axum::routing::any(gateway::proxy::proxy_handler))
        .with_state(proxy_state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .method("GET")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Alice"));
    assert!(body_str.contains("Bob"));
}

#[tokio::test]
async fn test_path_parameters() {
    let (proxy_state, _mock_server) = setup_test_gateway().await;

    let app = Router::new()
        .route("/*path", axum::routing::any(gateway::proxy::proxy_handler))
        .with_state(proxy_state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users/123")
                .method("GET")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Alice"));
    assert!(body_str.contains("123"));
}

#[tokio::test]
async fn test_post_request() {
    let (proxy_state, _mock_server) = setup_test_gateway().await;

    let app = Router::new()
        .route("/*path", axum::routing::any(gateway::proxy::proxy_handler))
        .with_state(proxy_state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .method("POST")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"name":"Charlie"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Charlie"));
}

#[tokio::test]
async fn test_route_not_found() {
    let (proxy_state, _mock_server) = setup_test_gateway().await;

    let app = Router::new()
        .route("/*path", axum::routing::any(gateway::proxy::proxy_handler))
        .with_state(proxy_state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .method("GET")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_method_not_allowed() {
    let (proxy_state, _mock_server) = setup_test_gateway().await;

    let app = Router::new()
        .route("/*path", axum::routing::any(gateway::proxy::proxy_handler))
        .with_state(proxy_state);

    // DELETE is not allowed for /api/users
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/users")
                .method("DELETE")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_health_check() {
    let (proxy_state, _mock_server) = setup_test_gateway().await;

    let app = Router::new()
        .route("/*path", axum::routing::any(gateway::proxy::proxy_handler))
        .with_state(proxy_state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .method("GET")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(body_str, "OK");
}

#[test]
fn test_config_validation() {
    let config = GatewayConfig {
        server: ServerConfig::default(),
        routes: vec![
            RouteConfig {
                path: "/api/test".to_string(),
                backend: "http://localhost:3000".to_string(),
                methods: vec!["GET".to_string()],
                strip_prefix: false,
                description: "Test route".to_string(),
            },
        ],
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_config_invalid_backend() {
    let config = GatewayConfig {
        server: ServerConfig::default(),
        routes: vec![
            RouteConfig {
                path: "/api/test".to_string(),
                backend: "invalid-url".to_string(),
                methods: vec!["GET".to_string()],
                strip_prefix: false,
                description: "Test route".to_string(),
            },
        ],
    };

    assert!(config.validate().is_err());
}
