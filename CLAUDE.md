# CLAUDE.md - AI Assistant Guide for API Gateway

> **Purpose**: This document provides AI assistants with comprehensive information about the Rust API Gateway codebase structure, development workflows, and conventions to enable efficient and accurate assistance.

**Last Updated**: 2025-11-17
**Project Version**: 0.1.0
**Current Phase**: Phase 5 Complete (Load Balancing & Health Checks)

---

## Table of Contents

1. [Project Overview](#project-overview)
2. [Codebase Architecture](#codebase-architecture)
3. [Module Reference](#module-reference)
4. [Development Workflows](#development-workflows)
5. [Code Conventions](#code-conventions)
6. [Configuration System](#configuration-system)
7. [Authentication System](#authentication-system)
8. [Testing Strategy](#testing-strategy)
9. [Common Tasks](#common-tasks)
10. [Troubleshooting](#troubleshooting)
11. [Project Roadmap](#project-roadmap)

---

## Project Overview

### What is this project?

A production-grade API Gateway built in Rust that provides:
- High-performance request routing and proxying
- JWT and API key authentication
- Flexible per-route configuration
- YAML-based configuration system
- Comprehensive error handling and logging

### Tech Stack

| Component | Technology | Version | Purpose |
|-----------|------------|---------|---------|
| Language | Rust | Edition 2021 | Core implementation |
| Web Framework | Axum | 0.7 | HTTP server and routing |
| Async Runtime | Tokio | 1.x | Async task execution |
| HTTP Client | reqwest | 0.12 | Backend request proxying |
| Path Router | matchit | 0.8 | Efficient path matching |
| Config Format | YAML | serde_yaml 0.9 | Configuration files |
| Auth (JWT) | jsonwebtoken | 9.3 | Token validation |
| Auth (API Keys) | Redis | 0.25 | Distributed key storage |
| Logging | tracing | 0.1 | Structured logging |
| Testing | tokio-test, wiremock | - | Test infrastructure |

### Current State

**Completed Phases**:
- ✅ Phase 1: Foundation & Core Routing
  - Path-based routing with parameters and wildcards
  - Method-based routing
  - Request proxying with timeout handling
  - YAML configuration
  - Error handling and logging

- ✅ Phase 2: Authentication & Authorization
  - JWT validation (HS256, RS256)
  - API key authentication (in-memory + Redis)
  - Per-route authentication configuration
  - Health check bypass

- ✅ Phase 3: Rate Limiting
  - Token bucket algorithm with `governor` crate
  - In-memory and Redis-backed rate limiting
  - Per-IP, per-user, per-API-key, and per-route limiting
  - Rate limit headers in responses

- ✅ Phase 4: Circuit Breaking & Resilience
  - Circuit breaker states (Closed, Open, Half-Open)
  - Per-backend circuit breakers
  - Configurable failure thresholds
  - Retry logic with exponential backoff
  - Fallback responses for open circuits

- ✅ Phase 5: Load Balancing & Health Checks
  - Load balancing strategies (Round Robin, Least Connections, Weighted, IP Hash)
  - Active health checks (HTTP, configurable intervals)
  - Passive health checks (failure-based)
  - Automatic backend removal/recovery
  - Connection tracking for least connections strategy
  - Client IP-based routing for session affinity

**Next Phases** (see ROADMAP.md):
- Phase 6: Observability & Monitoring
- Phase 7: Advanced Features
- Phase 8: Production Hardening

---

## Codebase Architecture

### Directory Structure

```
gateway/
├── src/                          # Source code (~3,500+ lines)
│   ├── main.rs                   # Binary entry point
│   ├── lib.rs                    # Library exports
│   ├── auth/                     # Authentication modules
│   │   ├── mod.rs                # AuthService orchestration
│   │   ├── jwt.rs                # JWT validation (HS256/RS256)
│   │   ├── api_key.rs            # API key validation
│   │   └── middleware.rs         # Auth extension types
│   ├── circuit_breaker/          # Circuit breaker & resilience
│   │   ├── mod.rs                # Module exports
│   │   ├── types.rs              # Types and configuration
│   │   ├── breaker.rs            # Circuit breaker logic
│   │   ├── retry.rs              # Retry executor
│   │   └── service.rs            # Circuit breaker service
│   ├── config/                   # Configuration system
│   │   └── mod.rs                # YAML parsing & validation
│   ├── error/                    # Error handling
│   │   └── mod.rs                # GatewayError enum
│   ├── healthcheck/              # Health check system
│   │   └── mod.rs                # Active & passive health checks
│   ├── loadbalancer/             # Load balancing
│   │   ├── mod.rs                # Load balancer core
│   │   ├── backend.rs            # Backend management & tracking
│   │   └── strategies.rs         # LB strategies (RR, LC, Weighted, IP Hash)
│   ├── rate_limit/               # Rate limiting
│   │   ├── mod.rs                # Module exports
│   │   ├── types.rs              # Types and configuration
│   │   ├── local.rs              # In-memory rate limiting
│   │   ├── redis.rs              # Redis-backed rate limiting
│   │   ├── lua_scripts.rs        # Lua scripts for Redis
│   │   ├── middleware.rs         # Rate limit middleware
│   │   └── service.rs            # Rate limit service
│   ├── router/                   # HTTP routing
│   │   └── mod.rs                # Path matching with matchit
│   └── proxy/                    # Request proxying
│       └── mod.rs                # Proxy handler & forwarding
├── tests/                        # Integration tests
│   └── integration_test.rs       # End-to-end tests with WireMock
├── config/                       # Default configurations
│   └── gateway.yaml              # Default gateway config
├── examples/                     # Example configurations
│   ├── simple.yaml               # Basic proxy setup
│   ├── microservices.yaml        # Multi-backend routing
│   ├── auth-jwt.yaml             # JWT authentication
│   ├── auth-apikey.yaml          # API key authentication
│   ├── auth-redis.yaml           # Redis-backed keys
│   ├── auth-rs256.yaml           # RSA JWT validation
│   ├── auth-mixed.yaml           # Multiple auth methods
│   ├── load-balancer-round-robin.yaml      # Round robin LB
│   ├── load-balancer-weighted.yaml         # Weighted LB
│   ├── load-balancer-least-connections.yaml # Least connections LB
│   ├── load-balancer-ip-hash.yaml          # IP hash LB
│   └── load-balancer-full-featured.yaml    # Complete example
├── .github/workflows/            # CI/CD pipelines
│   └── ci.yml                    # GitHub Actions workflow
├── Cargo.toml                    # Dependencies & metadata
├── Cargo.lock                    # Dependency lock file
├── README.md                     # User-facing documentation
├── ROADMAP.md                    # Development roadmap
├── AUTH.md                       # Authentication documentation
├── CONTRIBUTING.md               # Contribution guidelines
└── LICENSE                       # MIT License

Total: 20+ Rust files, ~97 tests
```

### Request Flow

```
Client Request
    ↓
┌─────────────────────────────────────────┐
│ Axum HTTP Server (proxy_handler)       │
│ - Extract method, path, query, headers │
└───────────────┬─────────────────────────┘
                ↓
        ┌───────────────┐
        │ Router Module │ ──→ Not found? → 404 Error
        │ (matchit)     │
        └───────┬───────┘
                ↓
        Route Match (with params)
                ↓
        ┌───────────────────┐
        │ Health Check?     │ ──→ Yes → Skip Auth
        │ (/health, /ping)  │
        └────────┬──────────┘
                 ↓ No
        ┌────────────────────┐
        │ Auth Required?     │ ──→ No → Skip Auth
        └────────┬───────────┘
                 ↓ Yes
        ┌────────────────────┐
        │ AuthService        │
        │ - Try JWT          │ ──→ Invalid → 401 Error
        │ - Try API Key      │
        └────────┬───────────┘
                 ↓
        Auth Success
                 ↓
        ┌────────────────────┐
        │ Build Backend URL  │
        │ - Apply params     │
        │ - Strip prefix?    │
        │ - Add query string │
        └────────┬───────────┘
                 ↓
        ┌────────────────────┐
        │ Forward Request    │
        │ (reqwest client)   │
        │ - Filter headers   │ ──→ Timeout → 504 Error
        │ - Apply timeout    │ ──→ Connection fail → 502 Error
        └────────┬───────────┘
                 ↓
        Backend Response
                 ↓
        ┌────────────────────┐
        │ Filter Response    │
        │ Headers            │
        └────────┬───────────┘
                 ↓
        Return to Client
```

### Component Relationships

```
┌─────────────────────────────────────────────────────────────┐
│                         main.rs                             │
│  1. Load config from file (GatewayConfig::from_file)       │
│  2. Validate configuration                                  │
│  3. Initialize tracing                                      │
│  4. Call init_gateway()                                     │
└────────────────────────────┬────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────┐
│                      lib.rs - init_gateway()                │
│  1. Create AuthService (if auth configured)                │
│  2. Create Router from routes                               │
│  3. Create reqwest Client                                   │
│  4. Build ProxyState (Arc-wrapped shared state)            │
│  5. Create Axum app with proxy_handler                      │
│  6. Start TCP listener                                      │
└────────────────────────────┬────────────────────────────────┘
                             ↓
                     Running Gateway
                             ↓
┌─────────────────────────────────────────────────────────────┐
│                  Shared State (ProxyState)                  │
│  - router: Arc<Router>                                      │
│  - client: reqwest::Client                                  │
│  - auth_service: Option<Arc<AuthService>>                  │
│  - timeout: Duration                                        │
└─────────────────────────────┬───────────────────────────────┘
                              ↓
              Per-Request Processing (proxy_handler)
```

---

## Module Reference

### 1. Router Module (`src/router/mod.rs`)

**Purpose**: Matches HTTP requests to configured routes using efficient path patterns.

**Key Types**:
```rust
pub struct Router {
    inner: matchit::Router<Route>,  // Path matching engine
}

pub struct Route {
    pub backend: String,              // Backend URL
    pub methods: Vec<Method>,         // Allowed methods (empty = all)
    pub strip_prefix: bool,           // Remove matched prefix?
    pub auth: Option<RouteAuthConfig>, // Auth requirements
}

pub struct RouteMatch {
    pub route: Route,
    pub params: HashMap<String, String>, // Extracted path params
}
```

**Key Functions**:
- `Router::new(routes)` - Create router from route configs
- `match_route(method, path)` - Find matching route
- `build_backend_url(route, params, query)` - Construct backend URL
- `convert_path_syntax(path)` - Convert Express-style to matchit format

**Path Pattern Support**:
```
Exact match:     "/api/users"           → "/api/users"
Parameter:       "/api/users/:id"       → "/api/users/123"
Wildcard:        "/api/*path"           → "/api/anything/here"
Strip prefix:    "/v1/*path" (strip)    → "/v1/products" → backend "/products"
```

**Error Cases**:
- Route not found → `GatewayError::RouteNotFound` (404)
- Method not allowed → `GatewayError::InvalidMethod` (405)

**Tests**: 13 unit tests covering all pattern types and edge cases

---

### 2. Proxy Module (`src/proxy/mod.rs`)

**Purpose**: Forwards requests to backend services with proper header handling and timeout enforcement.

**Key Types**:
```rust
pub struct ProxyState {
    pub router: Arc<Router>,
    pub client: reqwest::Client,
    pub auth_service: Option<Arc<AuthService>>,
    pub timeout: Duration,
}
```

**Key Functions**:
- `proxy_handler(State, Request)` - Main request handler (Axum)
- `forward_request(client, method, url, headers, body)` - HTTP proxying

**Header Filtering** (RFC 7230 hop-by-hop headers):
```rust
// These headers are NEVER forwarded:
- connection, keep-alive, proxy-authenticate, proxy-authorization
- te, trailers, transfer-encoding, upgrade
```

**Health Check Paths** (bypass auth):
```
/health, /healthz, /ready, /readiness, /ping
```

**Error Mapping**:
```rust
Timeout              → 504 GATEWAY_TIMEOUT
Connection errors    → 502 BAD_GATEWAY
Other proxy errors   → 502 BAD_GATEWAY
Route not found      → 404 NOT_FOUND
Method not allowed   → 405 METHOD_NOT_ALLOWED
Auth failures        → 401 UNAUTHORIZED
```

**Tests**: 4 unit tests + integration tests with WireMock

---

### 3. Auth Module (`src/auth/`)

**Purpose**: Provides authentication services for JWT and API keys.

#### a) AuthService (`src/auth/mod.rs`)

**Key Types**:
```rust
pub struct AuthService {
    jwt_validator: Option<JwtValidator>,
    api_key_validator: Option<ApiKeyValidator>,
}

pub struct AuthResult {
    pub user_id: String,
    pub auth_method: AuthMethodType,
    pub metadata: HashMap<String, serde_json::Value>,
}

pub enum AuthMethodType {
    Jwt,
    ApiKey,
}
```

**Key Functions**:
- `AuthService::new(config)` - Initialize validators
- `authenticate(headers, allowed_methods)` - Try authentication
  - Empty allowed_methods = try all configured validators
  - Specific methods = try only those validators

**Authentication Order**:
1. If JWT configured and allowed → try JWT first
2. If API Key configured and allowed → try API Key
3. Return first success OR combined error

#### b) JWT Validator (`src/auth/jwt.rs`)

**Supported Algorithms**:
- Symmetric: HS256, HS384, HS512 (shared secret)
- Asymmetric: RS256, RS384, RS512 (RSA public key)

**Configuration**:
```rust
pub struct JwtConfig {
    pub secret: Option<String>,         // For HS* algorithms
    pub public_key: Option<String>,     // For RS* algorithms (PEM format)
    pub algorithm: String,              // "HS256", "RS256", etc.
    pub issuer: Option<String>,         // Optional iss claim validation
    pub audience: Option<String>,       // Optional aud claim validation
}
```

**Claims Structure**:
```rust
pub struct Claims {
    pub sub: String,                    // Subject (user ID)
    pub iss: Option<String>,            // Issuer
    pub aud: Option<String>,            // Audience
    pub exp: usize,                     // Expiration (Unix timestamp)
    pub iat: Option<usize>,             // Issued at
    pub extra: HashMap<String, Value>,  // Custom claims
}
```

**Token Format**: `Authorization: Bearer <token>`

**Validation Steps**:
1. Extract token from Authorization header
2. Decode and verify signature
3. Check expiration (always enforced)
4. Validate issuer if configured
5. Validate audience if configured
6. Extract claims to metadata

**Tests**: 5 async tests

#### c) API Key Validator (`src/auth/api_key.rs`)

**Dual Storage Model**:
```rust
pub struct ApiKeyValidator {
    header_name: String,
    in_memory_keys: Arc<RwLock<HashMap<String, ApiKeyInfo>>>,
    redis_client: Option<redis::Client>,
    redis_prefix: String,
}

pub struct ApiKeyInfo {
    pub description: String,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

**Configuration**:
```rust
pub struct ApiKeyConfig {
    pub header: String,                  // Default: "X-API-Key"
    pub keys: HashMap<String, String>,   // In-memory keys
    pub redis: Option<RedisConfig>,      // Optional Redis backend
}

pub struct RedisConfig {
    pub url: String,                     // Redis connection URL
    pub prefix: String,                  // Key prefix (default: "gateway:apikey:")
}
```

**Validation Flow**:
1. Extract key from configured header
2. Check in-memory HashMap (fast)
3. If not found, check Redis (if configured)
4. Return success with key as user_id

**Key Management**:
- `add_key(key, description)` - Add to in-memory
- `remove_key(key)` - Remove from in-memory
- `set_key(key, metadata, ttl)` - Store in Redis
- `delete_key(key)` - Remove from Redis

**Tests**: 6 async tests

---

### 4. Config Module (`src/config/mod.rs`)

**Purpose**: Load, parse, and validate YAML configuration files.

**Configuration Hierarchy**:
```rust
pub struct GatewayConfig {
    pub server: ServerConfig,
    pub routes: Vec<RouteConfig>,
    pub auth: Option<AuthConfig>,
}

pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,              // Default: "0.0.0.0"

    #[serde(default = "default_port")]
    pub port: u16,                 // Default: 8080

    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,         // Default: 30
}

pub struct RouteConfig {
    pub path: String,              // Required (e.g., "/api/users/:id")
    pub backend: String,           // Required (must be http/https)
    pub methods: Vec<String>,      // Empty = all methods
    pub strip_prefix: bool,        // Default: false
    pub description: Option<String>,
    pub auth: Option<RouteAuthConfig>,
}

pub struct RouteAuthConfig {
    pub required: bool,            // Default: true
    pub methods: Vec<String>,      // Empty = try all
}

pub struct AuthConfig {
    pub jwt: Option<JwtConfig>,
    pub api_key: Option<ApiKeyConfig>,
}
```

**Loading Functions**:
```rust
GatewayConfig::from_file(path) -> Result<GatewayConfig>
GatewayConfig::from_yaml(yaml_str) -> Result<GatewayConfig>
config.validate() -> Result<()>
```

**Validation Rules**:
- ✓ Paths cannot be empty
- ✓ Backend URLs must start with `http://` or `https://`
- ✓ Methods must be valid: GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS
- ✓ All validation occurs before gateway starts

**Tests**: 7 unit tests

---

### 5. Error Module (`src/error/mod.rs`)

**Purpose**: Centralized error handling with HTTP status code mapping.

**Error Types**:
```rust
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Configuration error: {0}")]
    Config(String),                          // → 500

    #[error("Route not found: {0}")]
    RouteNotFound(String),                   // → 404

    #[error("Invalid route: {0}")]
    InvalidRoute(String),                    // → 400

    #[error("Method {0} not allowed for this route")]
    InvalidMethod(String),                   // → 405

    #[error("Backend error: {0}")]
    Backend(String),                         // → 502

    #[error("Proxy error: {0}")]
    Proxy(String),                           // → 502

    #[error("Gateway timeout")]
    Timeout,                                 // → 504

    #[error("Unauthorized: {0}")]
    Unauthorized(String),                    // → 401

    #[error("Invalid token: {0}")]
    InvalidToken(String),                    // → 401

    #[error("Missing credentials")]
    MissingCredentials,                      // → 401

    #[error("Invalid API key")]
    InvalidApiKey,                           // → 401

    #[error("Internal error: {0}")]
    Internal(String),                        // → 500

    // ... other variants
}
```

**HTTP Response**:
```rust
impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = json!({
            "error": self.to_string(),
            "status": status.as_u16()
        });
        (status, Json(body)).into_response()
    }
}
```

**Response Format**:
```json
{
  "error": "Error message here",
  "status": 401
}
```

**Tests**: 2 unit tests

---

## Development Workflows

### Local Development

**Prerequisites**:
- Rust 1.70+ (install via [rustup.rs](https://rustup.rs))
- Git
- Optional: Redis (for API key testing)

**Setup**:
```bash
# Clone repository
git clone https://github.com/therealutkarshpriyadarshi/gateway.git
cd gateway

# Build project
cargo build

# Run tests
cargo test

# Run with default config
cargo run

# Run with custom config
cargo run -- /path/to/config.yaml

# Enable debug logging
RUST_LOG=debug cargo run
RUST_LOG=gateway=debug,tower_http=info cargo run
```

**Development Tools**:
```bash
# Format code (required before commit)
cargo fmt

# Check formatting
cargo fmt --all -- --check

# Run linter (required before commit)
cargo clippy --all-targets --all-features -- -D warnings

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_basic_proxy

# Build release binary
cargo build --release

# Watch mode (requires cargo-watch)
cargo watch -x run
cargo watch -x test
```

### Git Workflow

**Branch Naming**:
```
feature/feature-name      - New features
fix/bug-description       - Bug fixes
docs/what-changed         - Documentation
test/what-testing         - Test additions
refactor/what-refactored  - Code refactoring
claude/*                  - AI-assisted branches (CI enabled)
```

**Commit Messages**:
```
feat: Add rate limiting support
fix: Correct path parameter matching in router
docs: Update authentication examples
test: Add integration tests for JWT validation
refactor: Simplify error handling in proxy module
chore: Update dependencies
```

**Pull Request Process**:
1. Create feature branch from `main` or `develop`
2. Make changes with tests
3. Run `cargo fmt`, `cargo clippy`, `cargo test`
4. Commit changes
5. Push to fork/branch
6. Create PR with description
7. Wait for CI checks (all must pass)
8. Address review feedback
9. Merge when approved

### CI/CD Pipeline

**GitHub Actions Workflow** (`.github/workflows/ci.yml`):

**Triggers**:
- Push to: `main`, `develop`, `claude/**`
- Pull requests to: `main`, `develop`

**Jobs**:

1. **Test** (runs on ubuntu-latest)
   - Checkout code
   - Install Rust stable
   - Cache Cargo registry, index, and build artifacts
   - Run `cargo test --verbose`
   - Run `cargo test --all-features --verbose`

2. **Rustfmt** (runs on ubuntu-latest)
   - Checkout code
   - Install Rust with rustfmt component
   - Run `cargo fmt --all -- --check`
   - **Fails if code is not formatted**

3. **Clippy** (runs on ubuntu-latest)
   - Checkout code
   - Install Rust with clippy component
   - Run `cargo clippy --all-targets --all-features -- -D warnings`
   - **Fails on any clippy warnings**

4. **Build** (runs on ubuntu-latest)
   - Checkout code
   - Install Rust stable
   - Build release binary: `cargo build --verbose --release`
   - Upload `target/release/gateway` as artifact

5. **Security Audit** (runs on ubuntu-latest)
   - Checkout code
   - Run `rustsec/audit-check@v2`
   - Checks for known vulnerabilities in dependencies

**All jobs must pass for PR to be mergeable.**

### Release Process

**Build Optimization** (Cargo.toml):
```toml
[profile.release]
opt-level = 3         # Maximum optimization
lto = true            # Link-time optimization
codegen-units = 1     # Single codegen unit
strip = true          # Strip debug symbols
```

**Binary Size**: Typically 5-10 MB after optimization and stripping.

**Performance**: See README.md for expected metrics (p50 < 5ms latency, >5k req/s).

---

## Code Conventions

### Rust Style Guidelines

**Follow**:
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Enforced by `rustfmt` (standard config)
- Enforced by `clippy` (all warnings = errors in CI)

**Code Organization**:
```rust
// 1. Imports (grouped: std, external crates, internal modules)
use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::error::{GatewayError, Result};

// 2. Constants
const MAX_RETRIES: u32 = 3;
const DEFAULT_TIMEOUT: u64 = 30;

// 3. Type definitions
pub struct MyStruct {
    field: String,
}

// 4. Implementations
impl MyStruct {
    pub fn new(field: String) -> Self {
        Self { field }
    }

    pub fn method(&self) -> Result<()> {
        // implementation
    }
}

// 5. Trait implementations
impl Default for MyStruct {
    fn default() -> Self {
        Self {
            field: String::new(),
        }
    }
}

// 6. Tests (at end of file)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // test code
    }
}
```

### Documentation

**Public APIs require doc comments**:
```rust
/// Validates a JWT token from the Authorization header.
///
/// # Arguments
///
/// * `headers` - HTTP headers containing the Authorization header
///
/// # Returns
///
/// * `Ok(AuthResult)` - Authentication succeeded with user info
/// * `Err(GatewayError)` - Authentication failed
///
/// # Errors
///
/// Returns `GatewayError::MissingCredentials` if Authorization header is missing.
/// Returns `GatewayError::InvalidToken` if token is invalid or expired.
pub async fn validate(&self, headers: &HeaderMap) -> Result<AuthResult> {
    // implementation
}
```

### Error Handling Patterns

**Use `Result<T>` for fallible operations**:
```rust
pub type Result<T> = std::result::Result<T, GatewayError>;

// Function signatures
pub fn load_config(path: &str) -> Result<GatewayConfig> { }
pub async fn forward_request(...) -> Result<Response> { }

// Question mark operator for propagation
let config = GatewayConfig::from_file(&path)?;
config.validate()?;

// Error context with map_err
.map_err(|e| GatewayError::Config(format!("Failed to load: {}", e)))?

// Pattern matching for different error handling
match result {
    Ok(value) => { /* use value */ },
    Err(GatewayError::RouteNotFound(_)) => { /* specific handling */ },
    Err(e) => { /* general error */ },
}
```

### Async Patterns

**Async functions**:
```rust
// Async function declaration
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    req: Request<Body>,
) -> Result<impl IntoResponse> {
    // async code with .await
    let result = some_async_call().await?;
    Ok(result)
}

// Async trait (requires async-trait crate)
#[async_trait]
pub trait AsyncValidator {
    async fn validate(&self, headers: &HeaderMap) -> Result<AuthResult>;
}
```

**Shared state with Arc**:
```rust
// Thread-safe reference counting
pub struct ProxyState {
    pub router: Arc<Router>,
    pub auth_service: Option<Arc<AuthService>>,
}

// Clone is cheap (just increments counter)
let state = ProxyState { /* ... */ };
let state_clone = state.clone();  // Arc makes this efficient
```

**Concurrent read/write with RwLock**:
```rust
use tokio::sync::RwLock;

// Read access (multiple readers allowed)
let keys = self.in_memory_keys.read().await;
let value = keys.get(&key);

// Write access (exclusive)
let mut keys = self.in_memory_keys.write().await;
keys.insert(key, value);
```

### Logging with Tracing

**Use structured logging**:
```rust
use tracing::{info, debug, warn, error, instrument};

// Info level for important events
info!(
    method = %method,
    path = %path,
    backend = %backend_url,
    "Proxying request"
);

// Debug level for detailed information
debug!(
    params = ?route_match.params,
    "Route matched with parameters"
);

// Warning for recoverable errors
warn!(
    error = %e,
    "Authentication failed, trying next method"
);

// Error for serious problems
error!(
    error = %e,
    "Failed to connect to Redis"
);

// Instrument function for automatic span creation
#[instrument(skip(self))]
pub async fn validate(&self, headers: &HeaderMap) -> Result<AuthResult> {
    // Automatically creates a tracing span with function name
}
```

**Control log levels**:
```bash
# All debug
RUST_LOG=debug cargo run

# Module-specific levels
RUST_LOG=gateway=debug,tower_http=info cargo run

# Trace everything (very verbose)
RUST_LOG=trace cargo run
```

### Testing Patterns

**Unit tests** (in same file):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_exact_match() {
        let routes = vec![/* ... */];
        let router = Router::new(&routes).unwrap();

        let result = router.match_route(&Method::GET, "/api/users");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_jwt_validation() {
        let config = JwtConfig { /* ... */ };
        let validator = JwtValidator::new(&config).unwrap();

        let result = validator.validate(&headers).await;
        assert!(result.is_ok());
    }
}
```

**Integration tests** (in `tests/` directory):
```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

#[tokio::test]
async fn test_basic_proxy() {
    // Setup mock backend
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    // Create gateway state
    let state = setup_test_gateway(mock_server.uri()).await;

    // Make request
    let request = Request::builder()
        .uri("/test")
        .body(Body::empty())
        .unwrap();

    let response = proxy_handler(State(state), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

---

## Configuration System

### Configuration File Format

**YAML structure**:
```yaml
# Server configuration
server:
  host: "0.0.0.0"        # Bind address (default: "0.0.0.0")
  port: 8080             # Listen port (default: 8080)
  timeout_secs: 30       # Request timeout (default: 30)

# Route definitions
routes:
  - path: "/api/users"              # Route path (Express-style)
    backend: "http://localhost:3000" # Backend URL (http/https required)
    methods: ["GET", "POST"]        # Allowed methods (empty = all)
    strip_prefix: false             # Remove matched prefix (default: false)
    description: "User service"     # Optional description
    auth:                           # Optional auth config
      required: true                # Require auth (default: true)
      methods: ["jwt"]              # Allowed methods (empty = all)

# Global authentication configuration
auth:
  jwt:                              # JWT authentication
    secret: "your-secret-key"       # For HS256/384/512
    # OR
    public_key: |                   # For RS256/384/512
      -----BEGIN PUBLIC KEY-----
      MIIBIjANBgkqhkiG9w0BAQEFA...
      -----END PUBLIC KEY-----
    algorithm: "HS256"              # Algorithm (default: "HS256")
    issuer: "https://auth.com"      # Optional issuer validation
    audience: "https://api.com"     # Optional audience validation

  api_key:                          # API key authentication
    header: "X-API-Key"             # Header name (default: "X-API-Key")
    keys:                           # In-memory keys
      "sk_test_123": "Development key"
      "sk_prod_456": "Production key"
    redis:                          # Optional Redis backend
      url: "redis://localhost:6379"
      prefix: "gateway:apikey:"     # Key prefix (default shown)
```

### Path Pattern Syntax

**Supported patterns**:
```
Exact match:
  path: "/api/users"
  Matches: /api/users
  Does NOT match: /api/users/123, /api/users/

Parameter (single segment):
  path: "/api/users/:id"
  Matches: /api/users/123, /api/users/abc
  Params: { "id": "123" }

Multiple parameters:
  path: "/api/:resource/:id"
  Matches: /api/users/123, /api/products/456
  Params: { "resource": "users", "id": "123" }

Wildcard (catch-all):
  path: "/api/*path"
  Matches: /api/anything, /api/users/123/profile
  Params: { "path": "anything" } or { "path": "users/123/profile" }

Prefix stripping:
  path: "/v1/products/*path"
  backend: "http://localhost:3000"
  strip_prefix: true

  Request: GET /v1/products/electronics/phones
  Forwarded to: http://localhost:3000/electronics/phones
```

### Configuration Examples

All examples are in the `examples/` directory:

**1. Simple Proxy** (`examples/simple.yaml`):
```yaml
server:
  port: 8080

routes:
  - path: "/api/*path"
    backend: "http://localhost:3000"
```

**2. Microservices** (`examples/microservices.yaml`):
```yaml
routes:
  - path: "/auth/*path"
    backend: "http://auth-service:4000"
  - path: "/users/*path"
    backend: "http://user-service:4001"
  - path: "/products/*path"
    backend: "http://catalog-service:4002"
```

**3. JWT Authentication** (`examples/auth-jwt.yaml`):
```yaml
auth:
  jwt:
    secret: "your-256-bit-secret"
    algorithm: "HS256"
    issuer: "https://auth.example.com"

routes:
  - path: "/api/public"
    backend: "http://localhost:3000"
    # No auth field = no auth required

  - path: "/api/protected"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: ["jwt"]
```

**4. Mixed Authentication** (`examples/auth-mixed.yaml`):
```yaml
auth:
  jwt:
    secret: "jwt-secret"
    algorithm: "HS256"
  api_key:
    header: "X-API-Key"
    keys:
      "partner_key_123": "Partner API access"

routes:
  - path: "/api/flexible"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: []  # Try JWT first, then API key

  - path: "/api/jwt-only"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: ["jwt"]

  - path: "/api/apikey-only"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: ["apikey"]
```

### Configuration Loading

**Default config path**: `config/gateway.yaml`

**Custom config**:
```bash
./gateway /path/to/custom.yaml
cargo run -- examples/auth-jwt.yaml
```

**Validation on startup**:
- All routes validated before gateway starts
- Invalid configuration = immediate error with details
- No requests processed until config is valid

---

## Authentication System

### Overview

The gateway supports two authentication methods:
1. **JWT (JSON Web Tokens)** - Industry-standard token validation
2. **API Keys** - Simple header-based authentication

Both can be enabled simultaneously. Per-route configuration determines which methods are accepted.

### JWT Authentication

**Algorithms Supported**:
- **HS256, HS384, HS512** - HMAC with SHA (symmetric, shared secret)
- **RS256, RS384, RS512** - RSA Signature (asymmetric, public/private keys)

**Token Format**:
```
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIiwiZXhwIjoxNjk5NTY0ODAwfQ.signature
```

**Configuration**:

For HS256 (symmetric):
```yaml
auth:
  jwt:
    secret: "your-256-bit-secret-key-here"
    algorithm: "HS256"
    issuer: "https://auth.example.com"    # Optional
    audience: "https://api.example.com"   # Optional
```

For RS256 (asymmetric):
```yaml
auth:
  jwt:
    public_key: |
      -----BEGIN PUBLIC KEY-----
      MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA...
      -----END PUBLIC KEY-----
    algorithm: "RS256"
    issuer: "https://auth.example.com"
    audience: "https://api.example.com"
```

**Validation Process**:
1. Extract token from `Authorization: Bearer <token>` header
2. Decode JWT and verify signature using configured key
3. Validate expiration (`exp` claim must be future timestamp)
4. Validate issuer (`iss` claim must match if configured)
5. Validate audience (`aud` claim must match if configured)
6. Extract user ID from `sub` claim
7. Extract all claims to metadata HashMap

**Standard Claims**:
- `sub` (subject) - Used as user_id in AuthResult
- `exp` (expiration) - Always validated
- `iss` (issuer) - Validated if configured
- `aud` (audience) - Validated if configured
- `iat` (issued at) - Extracted but not validated
- Custom claims - Extracted to metadata

**Testing JWT**:

Generate test token (Python):
```python
import jwt
import time

payload = {
    'sub': 'user123',
    'exp': int(time.time()) + 3600,  # 1 hour
    'iat': int(time.time()),
    'role': 'admin'  # Custom claim
}

token = jwt.encode(payload, 'your-secret-key', algorithm='HS256')
print(token)
```

Make authenticated request:
```bash
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/users
```

### API Key Authentication

**Header Format**:
```
X-API-Key: sk_test_1234567890
```

**Configuration**:

In-memory only:
```yaml
auth:
  api_key:
    header: "X-API-Key"
    keys:
      "sk_test_123": "Development key"
      "sk_prod_456": "Production key"
      "partner_xyz": "Partner XYZ access"
```

With Redis backend:
```yaml
auth:
  api_key:
    header: "X-API-Key"
    keys:
      "emergency_key": "Fallback if Redis fails"
    redis:
      url: "redis://localhost:6379"
      prefix: "gateway:apikey:"
```

**Dual Storage Model**:
1. **In-memory HashMap** - Fast, local-only, defined in config
2. **Redis** - Distributed, shared across gateway instances, dynamic

**Validation Process**:
1. Extract key from configured header (default: `X-API-Key`)
2. Check in-memory HashMap first (fast path)
3. If not found and Redis configured, check Redis
4. If found in either, return success with key as user_id
5. If not found anywhere, return `InvalidApiKey` error

**Redis Key Format**:
```
Key: {prefix}{api_key}
Value: JSON metadata
Example: "gateway:apikey:sk_test_123" → {"description": "Dev key", ...}
```

**Testing API Keys**:
```bash
# With valid key
curl -H "X-API-Key: sk_test_123" http://localhost:8080/api/data

# With invalid key (should return 401)
curl -H "X-API-Key: invalid_key" http://localhost:8080/api/data

# Missing key (should return 401)
curl http://localhost:8080/api/data
```

### Per-Route Authentication

**Route-level auth config**:
```yaml
routes:
  # Public route (no auth)
  - path: "/api/public"
    backend: "http://localhost:3000"

  # Auth required, any method accepted
  - path: "/api/protected"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: []  # Empty = try all configured methods

  # JWT only
  - path: "/api/admin"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: ["jwt"]

  # API key only
  - path: "/api/webhook"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: ["apikey"]

  # Either JWT or API key
  - path: "/api/flexible"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: ["jwt", "apikey"]
```

### Health Check Bypass

These paths **always** bypass authentication:
- `/health`
- `/healthz`
- `/ready`
- `/readiness`
- `/ping`

This ensures monitoring and health checks work without credentials.

### Authentication Error Responses

**401 Unauthorized responses**:
```json
{
  "error": "Authentication failed: Missing authentication credentials",
  "status": 401
}

{
  "error": "Authentication failed: Invalid JWT token: ExpiredSignature",
  "status": 401
}

{
  "error": "Authentication failed: Invalid API key",
  "status": 401
}
```

---

## Testing Strategy

### Test Organization

**Total Coverage**: ~43 tests across all modules

```
Unit Tests (in source files):
├── src/error/mod.rs          (2 tests)
├── src/config/mod.rs         (7 tests)
├── src/router/mod.rs         (13 tests)
├── src/proxy/mod.rs          (4 tests)
├── src/auth/jwt.rs           (5 async tests)
├── src/auth/api_key.rs       (6 async tests)
└── src/auth/mod.rs           (1 async test)

Integration Tests:
└── tests/integration_test.rs (5 async tests)
```

### Running Tests

**All tests**:
```bash
cargo test
```

**Specific test**:
```bash
cargo test test_router_exact_match
cargo test test_jwt_validation
```

**With output**:
```bash
cargo test -- --nocapture
```

**Async tests only**:
```bash
cargo test --lib  # Library tests (mostly async)
```

**Integration tests only**:
```bash
cargo test --test integration_test
```

### Unit Test Patterns

**Synchronous test**:
```rust
#[test]
fn test_router_exact_match() {
    let routes = vec![
        RouteConfig {
            path: "/api/users".to_string(),
            backend: "http://localhost:3000".to_string(),
            methods: vec![],
            strip_prefix: false,
            auth: None,
            description: None,
        },
    ];

    let router = Router::new(&routes).unwrap();
    let result = router.match_route(&Method::GET, "/api/users");

    assert!(result.is_ok());
    let route_match = result.unwrap();
    assert_eq!(route_match.route.backend, "http://localhost:3000");
}
```

**Async test** (requires `#[tokio::test]`):
```rust
#[tokio::test]
async fn test_jwt_validation() {
    let config = JwtConfig {
        secret: Some("test-secret".to_string()),
        public_key: None,
        algorithm: "HS256".to_string(),
        issuer: None,
        audience: None,
    };

    let validator = JwtValidator::new(&config).unwrap();

    // Create test token
    let token = create_test_token("user123", "test-secret").await;

    // Create headers
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );

    let result = validator.validate(&headers).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().user_id, "user123");
}
```

### Integration Test Patterns

**Using WireMock for backend simulation**:
```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

async fn setup_test_gateway() -> (ProxyState, MockServer) {
    let mock_server = MockServer::start().await;

    // Setup mock responses
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
        .mount(&mock_server)
        .await;

    // Create routes pointing to mock server
    let routes = vec![
        RouteConfig {
            path: "/test".to_string(),
            backend: mock_server.uri(),
            methods: vec![],
            strip_prefix: false,
            auth: None,
            description: None,
        },
    ];

    let router = Router::new(&routes).unwrap();
    let client = reqwest::Client::new();

    let state = ProxyState {
        router: Arc::new(router),
        client,
        auth_service: None,
        timeout: Duration::from_secs(30),
    };

    (state, mock_server)
}

#[tokio::test]
async fn test_basic_proxy() {
    let (state, _mock) = setup_test_gateway().await;

    let app = Router::new()
        .route("/*path", any(proxy_handler))
        .with_state(state);

    let request = Request::builder()
        .method("GET")
        .uri("/test")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

### Test Coverage Goals

**Current Coverage**: Good coverage of core functionality

**Areas to expand**:
- Edge cases in path matching
- More authentication scenarios
- Error recovery paths
- Concurrent request handling
- Timeout scenarios

**Running with coverage** (requires `cargo-tarpaulin`):
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
open tarpaulin-report.html
```

---

## Common Tasks

### Adding a New Route

**1. Update configuration**:
```yaml
routes:
  - path: "/api/newservice/*path"
    backend: "http://newservice:4000"
    methods: ["GET", "POST"]
    description: "New service routes"
```

**2. Restart gateway**:
```bash
cargo run -- config/gateway.yaml
```

**3. Test new route**:
```bash
curl http://localhost:8080/api/newservice/test
```

### Adding Authentication to a Route

**1. Configure global auth** (if not already):
```yaml
auth:
  jwt:
    secret: "your-secret"
    algorithm: "HS256"
```

**2. Add auth to route**:
```yaml
routes:
  - path: "/api/protected"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: ["jwt"]  # or ["apikey"] or []
```

**3. Test authentication**:
```bash
# Without token (should fail)
curl http://localhost:8080/api/protected

# With valid token
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/protected
```

### Adding a New Authentication Method

**Steps**:

1. **Create validator module** in `src/auth/`:
```rust
// src/auth/new_method.rs
use crate::error::{GatewayError, Result};
use super::{AuthResult, AuthMethodType};
use axum::http::HeaderMap;
use async_trait::async_trait;

pub struct NewMethodValidator {
    // configuration fields
}

impl NewMethodValidator {
    pub fn new(config: &NewMethodConfig) -> Result<Self> {
        // initialize validator
    }
}

#[async_trait]
impl NewMethodValidator {
    pub async fn validate(&self, headers: &HeaderMap) -> Result<AuthResult> {
        // validation logic
        Ok(AuthResult {
            user_id: "extracted_user_id".to_string(),
            auth_method: AuthMethodType::NewMethod,
            metadata: HashMap::new(),
        })
    }
}
```

2. **Add to AuthService** (`src/auth/mod.rs`):
```rust
pub struct AuthService {
    jwt_validator: Option<JwtValidator>,
    api_key_validator: Option<ApiKeyValidator>,
    new_method_validator: Option<NewMethodValidator>,  // Add this
}

impl AuthService {
    pub async fn new(config: Option<&AuthConfig>) -> Result<Option<Self>> {
        // ... existing code ...

        let new_method_validator = config
            .and_then(|c| c.new_method.as_ref())
            .map(|c| NewMethodValidator::new(c))
            .transpose()?;

        Ok(Some(AuthService {
            jwt_validator,
            api_key_validator,
            new_method_validator,
        }))
    }

    pub async fn authenticate(
        &self,
        headers: &HeaderMap,
        allowed_methods: &[AuthMethod],
    ) -> Result<AuthResult> {
        // Add new method to authentication chain
        if self.new_method_validator.is_some()
            && (allowed_methods.is_empty() || allowed_methods.contains(&AuthMethod::NewMethod))
        {
            if let Ok(result) = self.new_method_validator
                .as_ref()
                .unwrap()
                .validate(headers)
                .await
            {
                return Ok(result);
            }
        }
        // ... existing code ...
    }
}
```

3. **Add configuration struct** (`src/config/mod.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub jwt: Option<JwtConfig>,
    pub api_key: Option<ApiKeyConfig>,
    pub new_method: Option<NewMethodConfig>,  // Add this
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMethodConfig {
    // configuration fields
}
```

4. **Add enum variant** (`src/auth/mod.rs`):
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    Jwt,
    ApiKey,
    NewMethod,  // Add this
}
```

5. **Write tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_method_validation() {
        // test code
    }
}
```

### Debugging Request Flow

**Enable debug logging**:
```bash
RUST_LOG=debug cargo run
```

**Key log messages to look for**:
```
INFO gateway: Starting gateway server on 0.0.0.0:8080
DEBUG gateway::proxy: Incoming request method=GET path="/api/users"
DEBUG gateway::router: Route matched backend="http://localhost:3000" params={}
DEBUG gateway::auth: Attempting JWT authentication
DEBUG gateway::auth::jwt: Token validated successfully user_id="user123"
DEBUG gateway::proxy: Proxying request backend_url="http://localhost:3000/api/users"
INFO gateway::proxy: Request completed status=200 latency_ms=45
```

**Trace level for maximum detail**:
```bash
RUST_LOG=trace cargo run
```

**Module-specific logging**:
```bash
# Only debug proxy and auth
RUST_LOG=gateway::proxy=debug,gateway::auth=debug cargo run

# Debug gateway, info for dependencies
RUST_LOG=gateway=debug,tower_http=info cargo run
```

### Performance Testing

**Simple load test with Apache Bench**:
```bash
# 10000 requests, 100 concurrent
ab -n 10000 -c 100 http://localhost:8080/api/users

# With keep-alive
ab -n 10000 -c 100 -k http://localhost:8080/api/users
```

**With authentication**:
```bash
# Create token
TOKEN="your-jwt-token"

# Load test with auth header
ab -n 10000 -c 100 -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/users
```

**Expected performance** (Phase 1 targets):
- Latency: p50 < 5ms, p95 < 15ms
- Throughput: >5,000 req/s (single instance)
- Memory: <50MB base usage

---

## Troubleshooting

### Common Issues

#### 1. "Route not found" (404)

**Symptoms**:
```json
{
  "error": "Route not found: /api/unknown",
  "status": 404
}
```

**Solutions**:
- Check route path in configuration matches request path
- Verify path parameter syntax (`:param` not `{param}`)
- Check if wildcard route is needed (`/*path`)
- Ensure routes are defined before starting gateway

#### 2. "Method not allowed" (405)

**Symptoms**:
```json
{
  "error": "Method POST not allowed for this route",
  "status": 405
}
```

**Solutions**:
- Add method to route's `methods` list
- Use empty `methods: []` to allow all methods
- Check HTTP method in request matches configuration

#### 3. "Unauthorized" (401)

**Symptoms**:
```json
{
  "error": "Authentication failed: Missing authentication credentials",
  "status": 401
}
```

**Solutions**:
- **Missing credentials**: Add `Authorization: Bearer <token>` or `X-API-Key: <key>` header
- **Invalid JWT**: Check token hasn't expired, verify secret/public key matches
- **Invalid API key**: Verify key exists in configuration or Redis
- **Wrong auth method**: Check route's `auth.methods` allows your auth type

**Debug JWT**:
```bash
# Decode token to check expiration (without validation)
echo "eyJhbG..." | cut -d'.' -f2 | base64 -d 2>/dev/null | jq .
```

#### 4. "Bad Gateway" (502)

**Symptoms**:
```json
{
  "error": "Backend error: connection refused",
  "status": 502
}
```

**Solutions**:
- Verify backend service is running
- Check backend URL in configuration is correct
- Ensure network connectivity to backend
- Check firewall rules

**Test backend directly**:
```bash
curl http://localhost:3000/api/users
```

#### 5. "Gateway Timeout" (504)

**Symptoms**:
```json
{
  "error": "Gateway timeout",
  "status": 504
}
```

**Solutions**:
- Backend is too slow, increase `timeout_secs` in config
- Check backend performance
- Verify backend isn't deadlocked or hanging

**Increase timeout**:
```yaml
server:
  timeout_secs: 60  # Increase from default 30
```

#### 6. Build Errors

**"cannot find type `JwtValidator`"**:
- Run `cargo clean && cargo build`
- Check module exports in `src/auth/mod.rs`

**Dependency errors**:
```bash
# Update dependencies
cargo update

# Clean and rebuild
cargo clean
cargo build
```

#### 7. Test Failures

**"connection refused" in integration tests**:
- WireMock server may not have started
- Check async runtime is initialized (`#[tokio::test]`)

**"assertion failed" with expected vs actual**:
- Enable test output: `cargo test -- --nocapture`
- Check test setup creates correct state
- Verify mock server expectations match test

#### 8. Redis Connection Errors

**Symptoms**:
```
ERROR gateway::auth::api_key: Failed to connect to Redis error=...
```

**Solutions**:
- Start Redis: `redis-server` or `docker run -p 6379:6379 redis`
- Verify Redis URL in configuration
- Check Redis is accessible: `redis-cli ping` (should return PONG)
- Use in-memory fallback keys if Redis is unavailable

#### 9. Configuration Errors

**"Configuration error: Invalid backend URL"**:
- Backend URLs must start with `http://` or `https://`
- Example: `backend: "http://localhost:3000"` not `backend: "localhost:3000"`

**"Configuration error: Empty path"**:
- All routes must have non-empty `path` field

**YAML parsing errors**:
- Check YAML syntax (indentation, quotes, etc.)
- Use YAML validator: `yamllint config/gateway.yaml`

### Debug Checklist

When encountering issues:

1. **Enable debug logging**: `RUST_LOG=debug cargo run`
2. **Check configuration**:
   - Run `cargo run -- --check` (if implemented)
   - Validate YAML syntax
   - Verify all required fields present
3. **Test backend directly**: `curl http://localhost:3000/endpoint`
4. **Check authentication**:
   - Decode JWT to verify claims
   - Verify API key exists in config/Redis
5. **Review logs**: Look for ERROR, WARN messages
6. **Run tests**: `cargo test` to verify no regressions
7. **Check GitHub Actions**: CI may reveal platform-specific issues

---

## Project Roadmap

### Completed Phases

#### Phase 1: Foundation & Core Routing ✅
- Path-based routing with parameters and wildcards
- Method-based routing
- Request proxying with configurable timeouts
- YAML configuration system
- Structured logging with tracing
- Comprehensive error handling

#### Phase 2: Authentication & Authorization ✅
- JWT validation (HS256, RS256)
- API key authentication (in-memory + Redis)
- Per-route authentication configuration
- Multiple auth method support
- Health check endpoint bypass
- Detailed auth error responses

### Upcoming Phases

#### Phase 3: Rate Limiting (Week 4)
- Token bucket algorithm with `governor` crate
- In-memory rate limiting per IP
- Redis-backed distributed rate limiting
- Rate limiting by multiple dimensions (IP, user, API key, route)
- Rate limit headers in responses (`X-RateLimit-*`)
- Graceful Redis fallback

**Implementation files**:
- `src/ratelimit/mod.rs` - Rate limit service
- `src/ratelimit/token_bucket.rs` - Local algorithm
- `src/ratelimit/redis.rs` - Distributed storage

#### Phase 4: Circuit Breaking & Resilience (Week 5)
- Circuit breaker states (Closed, Open, Half-Open)
- Per-backend circuit breakers
- Configurable failure thresholds
- Retry logic with exponential backoff
- Fallback responses for open circuits
- Circuit breaker metrics

**Implementation files**:
- `src/resilience/circuit_breaker.rs`
- `src/resilience/retry.rs`

#### Phase 5: Load Balancing & Health Checks (Week 6)
- Load balancing strategies (Round Robin, Least Connections, Weighted, IP Hash)
- Active health checks (HTTP, TCP, custom scripts)
- Passive health checks (failure-based)
- Automatic backend removal/recovery
- Connection pooling per backend

**Implementation files**:
- `src/loadbalancer/mod.rs`
- `src/loadbalancer/strategies.rs`
- `src/healthcheck/mod.rs`

#### Phase 6: Observability & Monitoring (Week 7)
- Prometheus metrics exporter
- Key metrics (latency histograms, error rates, request counts)
- OpenTelemetry distributed tracing
- Trace context propagation
- Structured JSON logging
- Request ID generation
- Grafana dashboard templates

**Implementation files**:
- `src/metrics/mod.rs`
- `src/tracing/mod.rs`

#### Phase 7: Advanced Features (Week 8)
- Request/response transformation (headers, body, URL rewriting)
- CORS middleware with per-route policies
- Hot reload for configuration changes
- Service discovery (etcd, Consul integration)
- Request/response caching
- WebSocket support
- Request size limits
- IP whitelisting/blacklisting

**Implementation files**:
- `src/transform/mod.rs`
- `src/cors/mod.rs`
- `src/discovery/mod.rs`
- `src/cache/mod.rs`

#### Phase 8: Production Hardening (Week 8+)
- Achieve >80% code coverage
- Load testing and benchmarks
- Security audit
- TLS/mTLS support
- Secrets management
- Docker image and Kubernetes manifests
- Helm chart
- Comprehensive documentation
- Deployment guide
- Operations runbook

### Long-term Vision

**Post-v1.0 enhancements**:
- GraphQL federation support
- gRPC proxying
- Advanced caching with Redis
- Request coalescing/deduplication
- A/B testing and canary routing
- Plugin system for custom middleware
- Admin UI for configuration
- ML-based anomaly detection
- Multi-region active-active setup

**See ROADMAP.md for complete details and timelines.**

---

## Quick Reference

### File Locations

| What | Where |
|------|-------|
| Main entry point | `src/main.rs` |
| Library exports | `src/lib.rs` |
| Router implementation | `src/router/mod.rs` |
| Proxy handler | `src/proxy/mod.rs` |
| Auth service | `src/auth/mod.rs` |
| JWT validation | `src/auth/jwt.rs` |
| API key validation | `src/auth/api_key.rs` |
| Configuration parsing | `src/config/mod.rs` |
| Error types | `src/error/mod.rs` |
| Integration tests | `tests/integration_test.rs` |
| Default config | `config/gateway.yaml` |
| Example configs | `examples/*.yaml` |
| CI workflow | `.github/workflows/ci.yml` |

### Key Commands

```bash
# Development
cargo build                          # Build debug
cargo build --release                # Build optimized
cargo run                            # Run with default config
cargo run -- examples/auth-jwt.yaml  # Run with custom config
RUST_LOG=debug cargo run             # Run with debug logging

# Testing
cargo test                           # Run all tests
cargo test -- --nocapture            # Run with output
cargo test test_name                 # Run specific test
cargo tarpaulin --out Html           # Coverage report

# Code Quality
cargo fmt                            # Format code
cargo fmt --all -- --check           # Check formatting
cargo clippy --all-targets -- -D warnings  # Lint code

# Utilities
cargo clean                          # Clean build artifacts
cargo update                         # Update dependencies
cargo tree                           # Show dependency tree
```

### Important URLs

- **Repository**: https://github.com/therealutkarshpriyadarshi/gateway
- **Issues**: https://github.com/therealutkarshpriyadarshi/gateway/issues
- **License**: MIT (see LICENSE file)
- **Rust API Guidelines**: https://rust-lang.github.io/api-guidelines/
- **Axum Docs**: https://docs.rs/axum/latest/axum/
- **Tokio Docs**: https://tokio.rs

---

## For AI Assistants

### Best Practices When Assisting

1. **Always read relevant source files** before making changes
   - Use Read tool to examine existing code
   - Understand current patterns and conventions
   - Check for related tests

2. **Follow existing code style**:
   - Use `rustfmt` automatically
   - Match error handling patterns
   - Follow async patterns (Arc, RwLock)
   - Use tracing for logging

3. **Write tests for new code**:
   - Unit tests in same file as implementation
   - Integration tests in `tests/` for end-to-end flows
   - Use `#[tokio::test]` for async tests
   - Mock external dependencies with WireMock

4. **Update documentation**:
   - Add doc comments for public APIs
   - Update README.md if adding features
   - Update AUTH.md for auth changes
   - Keep CLAUDE.md current (this file)

5. **Validate changes**:
   - Run `cargo fmt`
   - Run `cargo clippy`
   - Run `cargo test`
   - Test manually with example configs

6. **Common modification patterns**:
   - New route → Update config YAML, restart
   - New auth method → Add validator, update AuthService, add config, add tests
   - New error type → Add to GatewayError enum, implement status_code()
   - New module → Create in src/, export in lib.rs, add tests

### Key Things to Know

- **This is a Rust async codebase** using Tokio runtime
- **Configuration is YAML-based** and validated on startup
- **All errors use Result<T> pattern** with custom GatewayError type
- **Shared state uses Arc** for thread-safe reference counting
- **Authentication is modular** with validator pattern
- **Tests use async patterns** (#[tokio::test]) and WireMock for mocking
- **Logging uses tracing** with structured fields
- **CI enforces** formatting (rustfmt), linting (clippy), and tests

### Project Goals

1. **Production-ready**: Battle-tested error handling, comprehensive tests, security focus
2. **High performance**: Rust speed, async I/O, efficient routing (matchit)
3. **Developer-friendly**: Clear configuration, good documentation, helpful errors
4. **Extensible**: Modular design, easy to add new auth methods, transformations, etc.
5. **Cloud-native**: Designed for containers, distributed systems, observability

---

**End of CLAUDE.md**

This document is maintained by the project contributors. When making significant changes to the codebase, please update this file to keep it accurate and helpful for AI assistants and new contributors.
