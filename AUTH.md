# Authentication & Authorization

The API Gateway supports comprehensive authentication mechanisms to protect your backend services. This document provides detailed information about configuring and using authentication.

## Table of Contents

- [Overview](#overview)
- [Authentication Methods](#authentication-methods)
  - [JWT (JSON Web Tokens)](#jwt-json-web-tokens)
  - [API Keys](#api-keys)
- [Configuration](#configuration)
- [Per-Route Authentication](#per-route-authentication)
- [Health Check Bypass](#health-check-bypass)
- [Testing Authentication](#testing-authentication)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)

## Overview

The gateway supports two primary authentication methods:

1. **JWT (JSON Web Tokens)**: Industry-standard token-based authentication with support for HS256 (symmetric) and RS256 (asymmetric) algorithms
2. **API Keys**: Simple, efficient authentication using static or Redis-backed API keys

You can configure authentication globally and override it per-route. Routes can require specific authentication methods or accept any configured method.

## Authentication Methods

### JWT (JSON Web Tokens)

JWT authentication validates bearer tokens in the `Authorization` header.

#### Supported Algorithms

- **HS256** (HMAC with SHA-256): Symmetric encryption using a shared secret
- **RS256** (RSA Signature with SHA-256): Asymmetric encryption using public/private key pairs

#### Configuration

##### HS256 (Symmetric)

```yaml
auth:
  jwt:
    secret: "your-256-bit-secret-key"
    algorithm: "HS256"
    issuer: "https://your-auth-server.com"  # Optional
    audience: "https://your-api.com"        # Optional
```

**Important**: Keep your secret key secure and never commit it to version control. Use environment variables or a secrets manager in production.

##### RS256 (Asymmetric)

```yaml
auth:
  jwt:
    public_key: |
      -----BEGIN PUBLIC KEY-----
      MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA...
      -----END PUBLIC KEY-----
    algorithm: "RS256"
    issuer: "https://your-auth-server.com"
    audience: "https://your-api.com"
```

#### Token Format

Tokens must be provided in the `Authorization` header using the Bearer scheme:

```http
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

#### Token Validation

The gateway validates:

1. **Signature**: Ensures the token hasn't been tampered with
2. **Expiration** (`exp` claim): Rejects expired tokens
3. **Issuer** (`iss` claim): Validates if configured
4. **Audience** (`aud` claim): Validates if configured

#### Claims

Standard JWT claims are extracted and logged:

- `sub` (subject): User identifier
- `exp` (expiration): Unix timestamp
- `iss` (issuer): Token issuer
- `aud` (audience): Intended audience
- `iat` (issued at): Token creation time

Custom claims are also extracted and available for logging and auditing.

### API Keys

API key authentication validates keys provided in a custom header (default: `X-API-Key`).

#### In-Memory Keys

Store API keys directly in the configuration file:

```yaml
auth:
  api_key:
    header: "X-API-Key"
    keys:
      "sk_test_123456789": "Development key"
      "sk_prod_abcdefghi": "Production key"
      "partner_xyz": "Partner XYZ key"
```

#### Redis-Backed Keys

For distributed environments, use Redis to store API keys:

```yaml
auth:
  api_key:
    header: "X-API-Key"
    keys:
      "emergency_key": "Fallback key"  # Optional in-memory fallback
    redis:
      url: "redis://localhost:6379"
      prefix: "gateway:apikey:"
```

#### Key Format

Provide the API key in the configured header:

```http
X-API-Key: sk_test_123456789
```

#### Custom Header Name

You can customize the header name:

```yaml
auth:
  api_key:
    header: "X-Custom-API-Key"  # Use any header name
```

## Configuration

### Global Configuration

Configure authentication methods globally in your `gateway.yaml`:

```yaml
auth:
  jwt:
    secret: "your-secret-key"
    algorithm: "HS256"
  api_key:
    header: "X-API-Key"
    keys:
      "key1": "Description 1"
      "key2": "Description 2"
```

### Multiple Authentication Methods

You can configure both JWT and API key authentication simultaneously:

```yaml
auth:
  jwt:
    secret: "jwt-secret"
    algorithm: "HS256"
  api_key:
    header: "X-API-Key"
    keys:
      "apikey123": "API Key for Service A"
```

## Per-Route Authentication

Each route can specify its authentication requirements:

```yaml
routes:
  # No authentication required
  - path: "/api/public"
    backend: "http://localhost:3000"
    methods: ["GET"]

  # Authentication required - accepts any configured method
  - path: "/api/users"
    backend: "http://localhost:3000"
    methods: ["GET", "POST"]
    auth:
      required: true
      methods: []  # Empty = accept any method

  # JWT only
  - path: "/api/admin"
    backend: "http://localhost:3001"
    methods: ["GET", "POST"]
    auth:
      required: true
      methods: ["jwt"]

  # API key only
  - path: "/api/webhook"
    backend: "http://localhost:3002"
    methods: ["POST"]
    auth:
      required: true
      methods: ["apikey"]

  # Both JWT and API key explicitly listed
  - path: "/api/flexible"
    backend: "http://localhost:3003"
    methods: ["GET"]
    auth:
      required: true
      methods: ["jwt", "apikey"]
```

### Authentication Fallthrough

When multiple methods are allowed, the gateway tries them in this order:

1. JWT (if configured and allowed)
2. API Key (if configured and allowed)

The first successful authentication is used. If all methods fail, a 401 Unauthorized response is returned.

## Health Check Bypass

The following paths automatically bypass authentication:

- `/health`
- `/healthz`
- `/ready`
- `/readiness`
- `/ping`

This ensures health checks and readiness probes work without authentication.

## Testing Authentication

### Testing with curl

#### JWT Authentication

```bash
# Get a JWT token (from your auth service)
export TOKEN="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."

# Make authenticated request
curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/users

# Without token (should fail)
curl http://localhost:8080/api/users
# Response: 401 Unauthorized
```

#### API Key Authentication

```bash
# With valid API key
curl -H "X-API-Key: sk_test_123456789" \
     http://localhost:8080/api/data

# Without API key (should fail)
curl http://localhost:8080/api/data
# Response: 401 Unauthorized
```

### Generating Test JWT Tokens

For testing, you can generate JWT tokens using online tools like [jwt.io](https://jwt.io) or command-line tools:

```bash
# Using Python
python3 -c "
import jwt
import time

payload = {
    'sub': 'user123',
    'exp': int(time.time()) + 3600,  # 1 hour from now
    'iat': int(time.time())
}

token = jwt.encode(payload, 'your-secret-key', algorithm='HS256')
print(token)
"
```

## Best Practices

### Security

1. **Use Strong Secrets**: For HS256, use at least 256-bit (32-byte) secrets
2. **Rotate Keys Regularly**: Implement key rotation policies
3. **Use RS256 in Production**: Asymmetric encryption is more secure for distributed systems
4. **Never Commit Secrets**: Use environment variables or secret managers
5. **Validate Issuer and Audience**: Always configure these in production
6. **Short Token Lifetimes**: Set JWT expiration to reasonable durations (e.g., 15-60 minutes)

### API Key Management

1. **Prefix Conventions**: Use prefixes like `sk_test_` or `sk_prod_` to identify key types
2. **Description Required**: Always add descriptions to track key usage
3. **Redis for Scale**: Use Redis-backed keys in multi-instance deployments
4. **Revocation Strategy**: Have a process to revoke compromised keys

### Configuration

1. **Environment Variables**: Load secrets from environment:
   ```bash
   export JWT_SECRET="your-secret-key"
   # Reference in config or use templating
   ```

2. **Separate Configs**: Use different config files for dev/staging/prod:
   ```
   config/
     ├── development.yaml
     ├── staging.yaml
     └── production.yaml
   ```

3. **Principle of Least Privilege**: Only enable authentication where needed

### Monitoring

1. **Log Authentication Events**: Monitor successful and failed auth attempts
2. **Alert on Anomalies**: Set up alerts for unusual authentication patterns
3. **Track Key Usage**: Monitor which API keys are being used

## Troubleshooting

### 401 Unauthorized

**Problem**: Requests return 401 Unauthorized

**Solutions**:

1. **Check Token/Key Format**:
   - JWT: Must be `Authorization: Bearer <token>`
   - API Key: Must be in configured header (e.g., `X-API-Key: <key>`)

2. **Verify Token Expiration**:
   ```bash
   # Decode JWT (without validation) to check expiration
   echo "eyJhbG..." | cut -d'.' -f2 | base64 -d 2>/dev/null | jq .
   ```

3. **Check Algorithm Match**: Ensure JWT algorithm in token matches config

4. **Verify Secret/Public Key**: Ensure the key in config matches the one used to sign tokens

### Authentication Required But No Auth Configured

**Problem**: Error message "Authentication required but no auth service configured"

**Solution**: Add `auth` section to your global configuration:

```yaml
auth:
  jwt:
    secret: "your-secret-key"
    algorithm: "HS256"
```

### Token Signature Verification Failed

**Problem**: Valid tokens are rejected with signature verification errors

**Solutions**:

1. **HS256**: Ensure the `secret` in config matches the signing secret
2. **RS256**: Ensure the `public_key` corresponds to the private key used for signing
3. **Check Algorithm**: Verify the algorithm in the token header matches config

### Redis Connection Errors

**Problem**: Gateway fails to connect to Redis for API key storage

**Solutions**:

1. **Verify Redis is Running**:
   ```bash
   redis-cli ping
   # Should return: PONG
   ```

2. **Check Connection URL**: Ensure the Redis URL is correct
   ```yaml
   redis:
     url: "redis://localhost:6379"  # Check host and port
   ```

3. **Fallback Keys**: Configure in-memory keys as fallback:
   ```yaml
   api_key:
     keys:
       "emergency_key": "Fallback access"
     redis:
       url: "redis://localhost:6379"
   ```

### Mixed Authentication Not Working

**Problem**: Route accepts both JWT and API key, but one method fails

**Solution**: When using empty `methods: []`, the gateway tries all configured methods. Check logs to see which authentication method is being attempted and which is failing.

## Examples

See the `examples/` directory for complete configuration examples:

- `auth-jwt.yaml` - JWT authentication with HS256
- `auth-rs256.yaml` - JWT authentication with RS256
- `auth-apikey.yaml` - API key authentication with in-memory keys
- `auth-redis.yaml` - API key authentication with Redis
- `auth-mixed.yaml` - Mixed JWT and API key authentication

## API Reference

### Error Responses

#### 401 Unauthorized

```json
{
  "error": "Authentication failed: <reason>",
  "status": 401
}
```

Common reasons:
- `Missing authentication credentials`
- `Invalid JWT token: <details>`
- `Invalid API key`
- `JWT: Token validation failed: <details>`

#### 500 Internal Server Error

```json
{
  "error": "Authentication required but no auth providers configured",
  "status": 500
}
```

This indicates a configuration problem where a route requires authentication but no authentication methods are configured globally.

## Further Reading

- [JWT Specification (RFC 7519)](https://tools.ietf.org/html/rfc7519)
- [JSON Web Algorithms (RFC 7518)](https://tools.ietf.org/html/rfc7518)
- [API Key Best Practices](https://cloud.google.com/docs/authentication/api-keys)
- [OAuth 2.0 Framework](https://tools.ietf.org/html/rfc6749)

## Support

For issues or questions:

- GitHub Issues: [github.com/therealutkarshpriyadarshi/gateway/issues](https://github.com/therealutkarshpriyadarshi/gateway/issues)
- Documentation: See README.md for general gateway documentation
