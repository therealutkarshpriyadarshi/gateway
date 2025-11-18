use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use gateway::config::{RouteConfig, ServerConfig, GatewayConfig};
use gateway::router::Router;
use http::Method;

fn benchmark_router_exact_match(c: &mut Criterion) {
    let routes = vec![
        RouteConfig {
            path: "/api/users".to_string(),
            backend: Some("http://localhost:3000".to_string()),
            backends: vec![],
            load_balancer: None,
            health_check: None,
            methods: vec![],
            strip_prefix: false,
            description: "User service".to_string(),
            auth: None,
            rate_limit: None,
            transform: None,
            cors: None,
            ip_filter: None,
            cache: None,
        },
        RouteConfig {
            path: "/api/orders".to_string(),
            backend: Some("http://localhost:3001".to_string()),
            backends: vec![],
            load_balancer: None,
            health_check: None,
            methods: vec![],
            strip_prefix: false,
            description: "Order service".to_string(),
            auth: None,
            rate_limit: None,
            transform: None,
            cors: None,
            ip_filter: None,
            cache: None,
        },
        RouteConfig {
            path: "/api/products".to_string(),
            backend: Some("http://localhost:3002".to_string()),
            backends: vec![],
            load_balancer: None,
            health_check: None,
            methods: vec![],
            strip_prefix: false,
            description: "Product service".to_string(),
            auth: None,
            rate_limit: None,
            transform: None,
            cors: None,
            ip_filter: None,
            cache: None,
        },
    ];

    let router = Router::new(routes).expect("Failed to create router");

    c.bench_function("router_exact_match", |b| {
        b.iter(|| {
            black_box(router.match_route(&Method::GET, "/api/users"))
        })
    });
}

fn benchmark_router_param_match(c: &mut Criterion) {
    let routes = vec![
        RouteConfig {
            path: "/api/users/:id".to_string(),
            backend: Some("http://localhost:3000".to_string()),
            backends: vec![],
            load_balancer: None,
            health_check: None,
            methods: vec![],
            strip_prefix: false,
            description: "User by ID".to_string(),
            auth: None,
            rate_limit: None,
            transform: None,
            cors: None,
            ip_filter: None,
            cache: None,
        },
    ];

    let router = Router::new(routes).expect("Failed to create router");

    c.bench_function("router_param_match", |b| {
        b.iter(|| {
            black_box(router.match_route(&Method::GET, "/api/users/12345"))
        })
    });
}

fn benchmark_router_wildcard_match(c: &mut Criterion) {
    let routes = vec![
        RouteConfig {
            path: "/api/*path".to_string(),
            backend: Some("http://localhost:3000".to_string()),
            backends: vec![],
            load_balancer: None,
            health_check: None,
            methods: vec![],
            strip_prefix: false,
            description: "Catch-all".to_string(),
            auth: None,
            rate_limit: None,
            transform: None,
            cors: None,
            ip_filter: None,
            cache: None,
        },
    ];

    let router = Router::new(routes).expect("Failed to create router");

    c.bench_function("router_wildcard_match", |b| {
        b.iter(|| {
            black_box(router.match_route(&Method::GET, "/api/deeply/nested/path/to/resource"))
        })
    });
}

fn benchmark_router_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("router_scale");

    for num_routes in [10, 50, 100, 500].iter() {
        let mut routes = Vec::new();
        for i in 0..*num_routes {
            routes.push(RouteConfig {
                path: format!("/api/service{}", i),
                backend: Some(format!("http://localhost:{}", 3000 + i)),
                backends: vec![],
                load_balancer: None,
                health_check: None,
                methods: vec![],
                strip_prefix: false,
                description: format!("Service {}", i),
                auth: None,
                rate_limit: None,
                transform: None,
                cors: None,
                ip_filter: None,
                cache: None,
            });
        }

        let router = Router::new(routes).expect("Failed to create router");

        group.bench_with_input(
            BenchmarkId::from_parameter(num_routes),
            num_routes,
            |b, &_num| {
                b.iter(|| {
                    black_box(router.match_route(&Method::GET, "/api/service50"))
                })
            },
        );
    }
    group.finish();
}

fn benchmark_config_parsing(c: &mut Criterion) {
    let yaml = r#"
server:
  host: "0.0.0.0"
  port: 8080
  timeout_secs: 30

routes:
  - path: "/api/users"
    backend: "http://localhost:3000"
    methods: ["GET", "POST"]
    description: "User service"
"#;

    c.bench_function("config_parsing", |b| {
        b.iter(|| {
            black_box(serde_yaml::from_str::<GatewayConfig>(yaml))
        })
    });
}

criterion_group!(
    benches,
    benchmark_router_exact_match,
    benchmark_router_param_match,
    benchmark_router_wildcard_match,
    benchmark_router_scale,
    benchmark_config_parsing
);
criterion_main!(benches);
