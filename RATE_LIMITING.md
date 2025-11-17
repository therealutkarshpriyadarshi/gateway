# Rate Limiting

Phase 3 implementation of distributed rate limiting for the API Gateway.

## Overview

The gateway provides comprehensive rate limiting capabilities to protect backend services from abuse and ensure fair resource allocation across clients. It supports both local (in-memory) and distributed (Redis-backed) rate limiting with automatic fallback.

## Features

### Multiple Dimensions
- **IP Address**: Rate limit by client IP
- **User ID**: Rate limit by authenticated user (from JWT claims)
- **API Key**: Rate limit by API key
- **Route**: Rate limit specific endpoints

### Algorithms
- **Token Bucket**: Smooth rate limiting with burst support (local only)
- **Sliding Window**: More accurate distributed rate limiting
- **Fixed Window**: Simpler distributed implementation
- **Token Bucket (Redis)**: Distributed smooth rate limiting

### Resilience
- **Graceful Fallback**: Automatically falls back to local rate limiting if Redis is unavailable
- **Error Handling**: Rate limits deny requests on errors for safety
- **Atomic Operations**: Redis Lua scripts ensure consistency

### Response Headers
All responses include rate limit information:
- `X-RateLimit-Limit`: Maximum requests allowed
- `X-RateLimit-Remaining`: Remaining requests in current window
- `X-RateLimit-Reset`: Seconds until the limit resets
- `Retry-After`: (429 responses only) Seconds to wait before retrying

## Configuration

### Global Rate Limiting

Apply rate limits to all routes:

```yaml
rate_limiting:
  enabled: true
  algorithm: sliding_window  # Options: sliding_window, fixed_window, token_bucket

  # Global rate limits (applied to all routes)
  global:
    - dimension: ip
      requests: 1000
      window_secs: 3600  # 1000 requests per hour per IP

    - dimension: user
      requests: 10000
      window_secs: 3600  # 10000 requests per hour per authenticated user

  # Optional: Redis configuration for distributed rate limiting
  redis:
    url: "redis://localhost:6379"
```

### Per-Route Rate Limiting

Override global limits for specific routes:

```yaml
routes:
  - path: "/api/expensive-operation"
    backend: "http://localhost:3000"
    rate_limit:
      - dimension: ip
        requests: 10
        window_secs: 60  # 10 requests per minute
        burst: 15         # Allow bursts up to 15

  - path: "/api/public"
    backend: "http://localhost:3001"
    rate_limit:
      - dimension: route
        requests: 100
        window_secs: 60
```

### Configuration Options

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `enabled` | boolean | No | Enable/disable rate limiting globally (default: true) |
| `algorithm` | string | No | Algorithm for Redis: `sliding_window`, `fixed_window`, `token_bucket` (default: sliding_window) |
| `global` | array | No | Global rate limit rules |
| `redis.url` | string | No | Redis connection URL. If not provided, uses local-only rate limiting |

### Rate Limit Rule Options

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `dimension` | string | Yes | What to rate limit by: `ip`, `user`, `apikey`, `route` |
| `requests` | number | Yes | Maximum requests allowed |
| `window_secs` | number | Yes | Time window in seconds |
| `burst` | number | No | Burst size (for token bucket, defaults to `requests`) |

## Examples

### Example 1: Basic IP-based Rate Limiting (Local Only)

```yaml
server:
  host: "0.0.0.0"
  port: 8080

rate_limiting:
  enabled: true
  global:
    - dimension: ip
      requests: 100
      window_secs: 60  # 100 requests per minute per IP

routes:
  - path: "/api/*path"
    backend: "http://localhost:3000"
```

This configuration:
- Limits each IP address to 100 requests per minute
- Uses local (in-memory) rate limiting (no Redis)
- Applies to all routes

### Example 2: Multi-Dimensional Rate Limiting with Redis

```yaml
server:
  host: "0.0.0.0"
  port: 8080

rate_limiting:
  enabled: true
  algorithm: sliding_window

  global:
    - dimension: ip
      requests: 1000
      window_secs: 3600  # 1000/hour per IP

    - dimension: user
      requests: 5000
      window_secs: 3600  # 5000/hour per authenticated user

  redis:
    url: "redis://localhost:6379"

auth:
  jwt:
    secret: "your-secret-key"
    algorithm: "HS256"

routes:
  - path: "/api/users"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: [jwt]
```

This configuration:
- Uses Redis for distributed rate limiting across multiple gateway instances
- Limits IPs to 1000 requests/hour
- Limits authenticated users to 5000 requests/hour
- Requires JWT authentication to track user-based limits

### Example 3: Per-Route Rate Limiting

```yaml
server:
  host: "0.0.0.0"
  port: 8080

rate_limiting:
  enabled: true
  redis:
    url: "redis://localhost:6379"

routes:
  # Expensive operation - very restrictive
  - path: "/api/reports/generate"
    backend: "http://localhost:3000"
    rate_limit:
      - dimension: ip
        requests: 5
        window_secs: 3600  # 5 reports per hour

  # Normal API - moderate limits
  - path: "/api/data"
    backend: "http://localhost:3001"
    rate_limit:
      - dimension: ip
        requests: 100
        window_secs: 60  # 100/minute

  # Public endpoint - higher limits
  - path: "/api/public"
    backend: "http://localhost:3002"
    rate_limit:
      - dimension: ip
        requests: 1000
        window_secs: 60  # 1000/minute
```

### Example 4: API Key Rate Limiting

```yaml
server:
  host: "0.0.0.0"
  port: 8080

auth:
  api_key:
    header: "X-API-Key"
    keys:
      "premium-key-123": "Premium tier customer"
      "free-key-456": "Free tier customer"

rate_limiting:
  enabled: true
  redis:
    url: "redis://localhost:6379"

routes:
  - path: "/api/data"
    backend: "http://localhost:3000"
    auth:
      required: true
      methods: [apikey]
    rate_limit:
      - dimension: apikey
        requests: 1000
        window_secs: 3600  # Rate limit per API key
```

## Rate Limit Algorithms

### Local (In-Memory) - Token Bucket

Used when Redis is not configured. Each key (IP, user, etc.) gets its own token bucket.

**Characteristics:**
- Smooth rate limiting with burst support
- Excellent for allowing short bursts while enforcing average rate
- Tokens replenish continuously
- Not shared across gateway instances

**Best for:** Single gateway instance, allowing bursts

### Redis - Sliding Window (Default)

Tracks individual requests in a sorted set, removing old entries.

**Characteristics:**
- Most accurate rate limiting
- Higher memory usage (stores each request)
- Handles distributed scenarios well
- No boundary issues between windows

**Best for:** High-accuracy requirements, moderate traffic

### Redis - Fixed Window

Simple counter with expiration.

**Characteristics:**
- Lowest memory usage
- Very fast
- Can have burst issues at window boundaries
- Slight inaccuracy

**Best for:** High traffic, memory-constrained environments

### Redis - Token Bucket

Distributed implementation of token bucket algorithm.

**Characteristics:**
- Smooth rate limiting with burst support
- Tracks tokens and last refill time
- Moderate memory usage
- Consistent across instances

**Best for:** Distributed deployments needing burst support

## Response Codes

### 200 OK (or other success codes)

Request allowed. Headers included:
```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 42
```

### 429 Too Many Requests

Rate limit exceeded. Response includes:

**Headers:**
```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 42
Retry-After: 42
```

**Body:**
```json
{
  "error": "Rate limit exceeded",
  "status": 429,
  "limit": 100,
  "remaining": 0,
  "reset_after": 42,
  "retry_after": 42
}
```

## Operational Considerations

### Redis Failover

The gateway automatically falls back to local rate limiting if Redis is unavailable:

1. On startup, gateway pings Redis
2. If Redis is down, uses local rate limiting
3. If Redis fails during operation, requests fall back to local limits
4. Gateway logs warnings when fallback occurs

### Scaling

**Single Instance:**
- Local rate limiting is sufficient
- Lower latency (no Redis roundtrip)
- Simpler deployment

**Multiple Instances:**
- Use Redis for consistent limits across instances
- Each instance maintains local fallback
- Coordinate limits via Redis

### Memory Usage

**Local Rate Limiting:**
- One rate limiter per unique key (IP, user, etc.)
- Minimal memory per limiter (~100 bytes)
- Old limiters are not automatically cleaned up

**Redis Rate Limiting:**
- Sliding window: ~50 bytes per request in window
- Fixed window: ~20 bytes per window
- Token bucket: ~40 bytes per key
- Keys auto-expire after 2x window duration

### Performance

**Local:**
- Latency: <1ms
- Throughput: >100k checks/sec

**Redis:**
- Latency: 1-5ms (network + Redis)
- Throughput: Limited by Redis capacity
- Use pipelining for batch checks (future enhancement)

## Monitoring

### Metrics

Monitor these metrics in your production environment:

- **Rate limit hits**: Count of 429 responses
- **Rate limit dimension**: Which dimension (IP, user, etc.) is hitting limits
- **Redis availability**: Track fallback to local rate limiting
- **Response times**: Monitor p95, p99 latency
- **Active limiters**: Number of unique keys being rate limited

### Logging

The gateway logs rate limiting events:

```
DEBUG gateway::rate_limit: Rate limit check passed for key: gateway:ratelimit:ip:192.168.1.1
WARN  gateway::rate_limit: Rate limit exceeded for key: gateway:ratelimit:ip:192.168.1.1
WARN  gateway::rate_limit: Redis rate limit check failed, using local fallback
```

Enable debug logging for rate limiting:
```bash
RUST_LOG=gateway::rate_limit=debug cargo run
```

## Testing

### Unit Tests

Run unit tests for rate limiting:
```bash
cargo test rate_limit
```

### Integration Tests with Redis

Start Redis:
```bash
docker run -d -p 6379:6379 redis:latest
```

Run Redis integration tests:
```bash
cargo test --lib -- --ignored
```

### Load Testing

Example using Apache Bench:
```bash
# Test rate limiting (should see 429s)
ab -n 1000 -c 10 http://localhost:8080/api/test

# Check headers
curl -v http://localhost:8080/api/test
```

## Troubleshooting

### Rate limits not working

1. **Check configuration:**
   ```bash
   # Verify config is valid
   cargo run -- --config config/gateway.yaml --validate
   ```

2. **Check logs:**
   ```bash
   RUST_LOG=gateway::rate_limit=debug cargo run
   ```

3. **Verify Redis connection:**
   ```bash
   redis-cli ping
   ```

### 429 errors for legitimate traffic

1. **Check current limits:**
   - Review `X-RateLimit-*` headers in responses
   - Verify `requests` and `window_secs` in config

2. **Adjust limits:**
   - Increase `requests` or `window_secs`
   - Add `burst` parameter for spiky traffic

3. **Change dimensions:**
   - Use `user` or `apikey` instead of `ip` for more granular control

### Redis connection issues

1. **Verify Redis is running:**
   ```bash
   redis-cli ping
   ```

2. **Check Redis URL:**
   - Ensure `redis.url` in config is correct
   - Test connection: `redis-cli -u redis://localhost:6379 ping`

3. **Check logs for fallback:**
   - Gateway should log warnings when falling back to local

### Memory usage growing

**Local rate limiting:**
- Limiters are never cleaned up automatically
- Restart gateway periodically if needed
- Consider using Redis for auto-expiration

**Redis rate limiting:**
- Keys auto-expire
- Monitor Redis memory usage
- Adjust `maxmemory` and eviction policy if needed

## Best Practices

1. **Start conservative:** Begin with restrictive limits and loosen based on actual usage

2. **Use multiple dimensions:** Combine IP and user-based limits for better protection

3. **Per-route limits:** Set stricter limits on expensive operations

4. **Monitor 429 rates:** High 429 rates may indicate:
   - Limits are too strict
   - Actual abuse/attack
   - Client bugs (retry loops)

5. **Test with production traffic:** Load test with realistic patterns before going live

6. **Redis for production:** Use Redis in production for:
   - Multiple gateway instances
   - Consistent limits
   - Better visibility

7. **Graceful degradation:** Local fallback ensures service continues if Redis fails

8. **Documentation:** Document your rate limits in API documentation so clients know what to expect

## Future Enhancements

Planned improvements for future versions:

- Dynamic rate limiting based on load
- Per-user tier support (free, premium, enterprise)
- Rate limit quota management (monthly allowances)
- Adaptive limits based on behavior
- Geographic rate limiting
- Batch rate limit checks for better Redis performance
- Rate limit metrics export (Prometheus)
- Admin API for runtime limit adjustment

## References

- [Token Bucket Algorithm](https://en.wikipedia.org/wiki/Token_bucket)
- [Leaky Bucket Algorithm](https://en.wikipedia.org/wiki/Leaky_bucket)
- [Redis Rate Limiting Patterns](https://redis.io/docs/manual/patterns/rate-limiter/)
- [RFC 6585 - Additional HTTP Status Codes](https://tools.ietf.org/html/rfc6585#section-4)
