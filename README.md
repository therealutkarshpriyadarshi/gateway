# API Gateway

A production-grade API Gateway built in Rust, providing high-performance request routing, proxying, and traffic management for microservices architectures.

## Features

### Phase 1: Foundation & Core Routing ✅

- **Path-Based Routing**: Route requests based on URL paths with support for parameters and wildcards
- **Method-Based Routing**: Filter routes by HTTP methods (GET, POST, PUT, DELETE, etc.)
- **Request Proxying**: Forward requests to backend services with configurable timeouts
- **YAML Configuration**: Easy-to-read configuration files for defining routes
- **Structured Logging**: Built-in request/response logging with `tracing`
- **Error Handling**: Comprehensive error handling with meaningful error messages
- **High Performance**: Built on Axum and Tokio for async, non-blocking operations

### Phase 2: Authentication & Authorization ✅

- **JWT Validation**: Full JWT support with HS256 and RS256 algorithms
- **API Key Authentication**: Header-based API keys with in-memory or Redis-backed storage
- **Per-Route Authentication**: Configure authentication requirements per route
- **Multiple Auth Methods**: Support JWT and API keys simultaneously
- **Health Check Bypass**: Automatic authentication bypass for health check endpoints
- **Flexible Configuration**: Allow specific auth methods or accept any configured method
- **Comprehensive Error Responses**: Clear 401 Unauthorized responses with detailed error messages

📖 **See [AUTH.md](AUTH.md) for detailed authentication documentation and examples.**

### Phase 3: Rate Limiting ✅

- **Multiple Dimensions**: Rate limit by IP, User (JWT), API Key, or Route
- **Multiple Algorithms**: Token bucket (local), sliding window, fixed window, token bucket (Redis)
- **Local & Distributed**: In-memory rate limiting or Redis-backed for distributed scenarios
- **Graceful Fallback**: Automatically falls back to local rate limiting if Redis is unavailable
- **Per-Route Configuration**: Override global limits for specific endpoints
- **Rate Limit Headers**: Comprehensive `X-RateLimit-*` headers in all responses
- **Atomic Operations**: Redis Lua scripts ensure consistency in distributed environments

📖 **See [RATE_LIMITING.md](RATE_LIMITING.md) for detailed rate limiting documentation and examples.**

### Coming Soon

- Circuit Breaking & Resilience
- Load Balancing & Health Checks
- Observability & Monitoring (Prometheus, OpenTelemetry)
- Request/Response Transformation
- CORS Support
- Service Discovery

## Quick Start

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs))

### Installation

```bash
# Clone the repository
git clone https://github.com/therealutkarshpriyadarshi/gateway.git
cd gateway

# Build the project
cargo build --release

# Run tests
cargo test
```

### Configuration

Create a configuration file (e.g., `config/gateway.yaml`):

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  timeout_secs: 30

routes:
  - path: "/api/users"
    backend: "http://localhost:3000"
    methods: ["GET", "POST"]
    description: "User service"

  - path: "/api/users/:id"
    backend: "http://localhost:3000"
    methods: ["GET", "PUT", "DELETE"]
    description: "User operations by ID"

  - path: "/api/orders"
    backend: "http://localhost:3001"
    methods: ["GET", "POST"]
    description: "Order service"
```

### Running the Gateway

```bash
# Run with default config (config/gateway.yaml)
cargo run --release

# Run with custom config
cargo run --release -- /path/to/config.yaml

# Enable debug logging
RUST_LOG=debug cargo run --release
```

## Configuration Reference

### Server Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `"0.0.0.0"` | Host address to bind to |
| `port` | number | `8080` | Port to listen on |
| `timeout_secs` | number | `30` | Request timeout in seconds |

### Route Configuration

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | URL path pattern (supports `:param` and `*wildcard`) |
| `backend` | string | Yes | Backend service URL (must start with http:// or https://) |
| `methods` | array | No | Allowed HTTP methods (empty = all methods) |
| `strip_prefix` | boolean | No | Strip matched path before forwarding |
| `description` | string | No | Human-readable route description |

## Path Patterns

The gateway supports several path pattern types:

### Exact Match
```yaml
- path: "/api/users"
  backend: "http://localhost:3000"
```
Matches: `/api/users`

### Path Parameters
```yaml
- path: "/api/users/:id"
  backend: "http://localhost:3000"
```
Matches: `/api/users/123`, `/api/users/abc`

### Wildcards
```yaml
- path: "/api/*path"
  backend: "http://localhost:3000"
```
Matches: `/api/anything`, `/api/nested/path`

### Prefix Stripping

When `strip_prefix: true`, the matched portion is removed before forwarding:

```yaml
- path: "/v1/products/*path"
  backend: "http://localhost:3002"
  strip_prefix: true
```

Request: `GET /v1/products/electronics/phones`
Forwarded to: `http://localhost:3002/electronics/phones`

## Examples

### Simple Proxy

See `examples/simple.yaml` for a minimal configuration that proxies all `/api/*` requests to a single backend.

```bash
cargo run --release -- examples/simple.yaml
```

### Microservices Architecture

See `examples/microservices.yaml` for a complete microservices setup with multiple backend services.

```bash
cargo run --release -- examples/microservices.yaml
```

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_basic_proxy

# Run tests with coverage
cargo tarpaulin --out Html
```

## Usage Examples

### Start Backend Services (for testing)

```bash
# Terminal 1 - User service
python3 -m http.server 3000

# Terminal 2 - Order service
python3 -m http.server 3001

# Terminal 3 - Gateway
cargo run --release
```

### Make Requests

```bash
# Route to user service
curl http://localhost:8080/api/users

# Route with path parameter
curl http://localhost:8080/api/users/123

# POST request
curl -X POST http://localhost:8080/api/users \
  -H "Content-Type: application/json" \
  -d '{"name":"Alice"}'

# Route to order service
curl http://localhost:8080/api/orders
```

## Architecture

```
┌─────────┐
│ Client  │
└────┬────┘
     │ HTTP Request
     ▼
┌────────────────────────────────────┐
│         API Gateway                │
│                                    │
│  ┌──────────────────────────────┐ │
│  │   Router (matchit)           │ │
│  │   - Path matching            │ │
│  │   - Method validation        │ │
│  └──────────────────────────────┘ │
│                                    │
│  ┌──────────────────────────────┐ │
│  │   Proxy Handler              │ │
│  │   - Request forwarding       │ │
│  │   - Header management        │ │
│  │   - Timeout handling         │ │
│  └──────────────────────────────┘ │
└────────────────────────────────────┘
     │ HTTP Request
     ▼
┌────────────────────────────────────┐
│       Backend Services             │
│                                    │
│  ┌──────────┐  ┌──────────┐      │
│  │  User    │  │  Order   │  ...  │
│  │ Service  │  │ Service  │      │
│  └──────────┘  └──────────┘      │
└────────────────────────────────────┘
```

## Project Structure

```
gateway/
├── src/
│   ├── config/         # Configuration loading and validation
│   ├── error/          # Error types and handling
│   ├── router/         # Path-based routing with matchit
│   ├── proxy/          # Request proxying logic
│   ├── lib.rs          # Library entry point
│   └── main.rs         # Binary entry point
├── tests/              # Integration tests
├── config/             # Default configuration
├── examples/           # Example configurations
├── .github/workflows/  # CI/CD pipelines
├── Cargo.toml          # Dependencies and metadata
├── ROADMAP.md          # Project roadmap
└── README.md           # This file
```

## Performance

### Current Metrics

Built on high-performance Rust libraries:
- **Axum**: Web framework built on Tokio
- **Tokio**: Async runtime
- **matchit**: Fast path matching
- **reqwest**: HTTP client with connection pooling

### Expected Performance (Phase 1)

- Latency: p50 < 5ms, p95 < 15ms
- Throughput: >5,000 req/s (single instance)
- Memory: <50MB base usage

## Development

### Running in Development

```bash
# Watch mode with auto-reload
cargo watch -x run

# Run with debug logging
RUST_LOG=debug cargo run

# Format code
cargo fmt

# Lint code
cargo clippy -- -D warnings
```

### Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes and add tests
4. Run tests: `cargo test`
5. Run lints: `cargo clippy`
6. Format code: `cargo fmt`
7. Commit: `git commit -am 'Add new feature'`
8. Push: `git push origin feature/my-feature`
9. Create a Pull Request

## Logging

The gateway uses `tracing` for structured logging. Control log levels with the `RUST_LOG` environment variable:

```bash
# Debug level for gateway, info for dependencies
RUST_LOG=gateway=debug,tower_http=info cargo run

# Trace everything
RUST_LOG=trace cargo run

# Specific module
RUST_LOG=gateway::router=debug cargo run
```

## Error Handling

The gateway provides detailed error responses:

```json
{
  "error": "Route not found: /api/invalid",
  "status": 404
}
```

Error types:
- `404 Not Found`: Route doesn't exist
- `405 Method Not Allowed`: HTTP method not allowed for route
- `502 Bad Gateway`: Backend service error
- `504 Gateway Timeout`: Backend service timeout
- `500 Internal Server Error`: Gateway error

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the complete development plan.

- [x] Phase 1: Foundation & Core Routing (Weeks 1-2)
- [x] Phase 2: Authentication & Authorization (Week 3)
- [x] Phase 3: Rate Limiting (Week 4)
- [ ] Phase 4: Circuit Breaking & Resilience (Week 5)
- [ ] Phase 5: Load Balancing & Health Checks (Week 6)
- [ ] Phase 6: Observability & Monitoring (Week 7)
- [ ] Phase 7: Advanced Features (Week 8)
- [ ] Phase 8: Production Hardening (Week 8+)

## License

MIT License - see LICENSE file for details

## Support

- Issues: [GitHub Issues](https://github.com/therealutkarshpriyadarshi/gateway/issues)
- Discussions: [GitHub Discussions](https://github.com/therealutkarshpriyadarshi/gateway/discussions)

## Acknowledgments

Built with:
- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [Tokio](https://tokio.rs) - Async runtime
- [matchit](https://github.com/ibraheemdev/matchit) - Path routing
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP client
- [tracing](https://github.com/tokio-rs/tracing) - Logging
