# Circuit Breaker & Resilience

The gateway implements a robust circuit breaker pattern to protect backend services from cascading failures and improve system resilience. This document covers circuit breaker functionality, retry logic with exponential backoff, and configuration options.

## Overview

The circuit breaker pattern prevents an application from repeatedly trying to execute an operation that's likely to fail, allowing it to continue without waiting for the fault to be fixed or wasting CPU cycles. It also enables an application to detect whether the fault has been resolved.

## Circuit Breaker States

The circuit breaker has three states:

### 1. Closed (Normal Operation)

- **Behavior**: All requests pass through to the backend
- **Monitoring**: Tracks consecutive failures
- **Transition**: Moves to **Open** after reaching the failure threshold

### 2. Open (Fault Detected)

- **Behavior**: Requests are immediately rejected without hitting the backend
- **Response**: Returns `503 Service Unavailable`
- **Duration**: Remains open for the configured timeout period
- **Transition**: Moves to **Half-Open** after timeout expires

### 3. Half-Open (Testing Recovery)

- **Behavior**: Allows a limited number of probe requests through
- **Monitoring**: Tracks success rate of probe requests
- **Transitions**:
  - Moves to **Closed** if probe requests succeed (reaching success threshold)
  - Returns to **Open** if any probe request fails

## Configuration

### Global Circuit Breaker Configuration

```yaml
circuit_breaker:
  # Number of consecutive failures before opening the circuit
  failure_threshold: 5

  # Number of consecutive successes in half-open state before closing
  success_threshold: 2

  # Duration (in seconds) to wait in open state before transitioning to half-open
  timeout_secs: 60

  # Number of requests to allow in half-open state
  half_open_requests: 3

  # Timeout for individual requests in seconds
  request_timeout_secs: 30
```

### Configuration Options

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | u32 | 5 | Number of consecutive failures before opening circuit |
| `success_threshold` | u32 | 2 | Number of consecutive successes needed to close circuit |
| `timeout_secs` | u64 | 60 | How long circuit stays open before trying half-open |
| `half_open_requests` | u32 | 3 | Max concurrent requests in half-open state |
| `request_timeout_secs` | u64 | 30 | Timeout for individual backend requests |

## Retry Logic with Exponential Backoff

The gateway includes configurable retry logic with exponential backoff for transient failures.

### Retry Configuration

```yaml
retry:
  # Maximum number of retry attempts
  max_retries: 3

  # Initial backoff duration in milliseconds
  initial_backoff_ms: 100

  # Maximum backoff duration in milliseconds
  max_backoff_ms: 10000

  # Backoff multiplier (exponential growth factor)
  backoff_multiplier: 2.0
```

### Configuration Options

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_retries` | u32 | 3 | Maximum number of retry attempts |
| `initial_backoff_ms` | u64 | 100 | Initial wait time before first retry |
| `max_backoff_ms` | u64 | 10000 | Maximum wait time between retries |
| `backoff_multiplier` | f64 | 2.0 | Factor by which backoff increases |

### Retry Behavior

- **Retryable Errors**: Only timeouts and connection errors are retried
- **Non-Retryable**: 4xx client errors, authentication failures
- **Backoff**: Waits increase exponentially (100ms, 200ms, 400ms, etc.)
- **Jitter**: Built-in to prevent thundering herd

## How It Works

### Per-Backend Circuit Breakers

Each backend service has its own independent circuit breaker, ensuring that failures in one service don't affect others.

```
┌─────────────────────────────────────────────┐
│           API Gateway                        │
│                                             │
│  ┌─────────────┐  ┌─────────────┐         │
│  │   Backend A │  │   Backend B │         │
│  │   Circuit   │  │   Circuit   │         │
│  │   [Closed]  │  │   [Open]    │         │
│  └─────────────┘  └─────────────┘         │
│         ✓                ✗                  │
└─────────────────────────────────────────────┘
         │                │
         ▼                ▼
   [Backend A]      [Backend B]
   (healthy)        (failing)
```

### Request Flow with Circuit Breaker

```
┌────────────────────────────────────────────┐
│ 1. Check Circuit State                     │
│    ├─ Closed:     Allow request            │
│    ├─ Open:       Reject (503)             │
│    └─ Half-Open:  Allow limited requests   │
└────────────────────────────────────────────┘
                    │
                    ▼
┌────────────────────────────────────────────┐
│ 2. Retry Logic (if configured)             │
│    ├─ Attempt request                      │
│    ├─ On failure: Wait with backoff        │
│    └─ Retry up to max_retries times        │
└────────────────────────────────────────────┘
                    │
                    ▼
┌────────────────────────────────────────────┐
│ 3. Record Result                           │
│    ├─ Success:     Reset failure count     │
│    ├─ 5xx Error:   Record failure          │
│    └─ Timeout:     Record timeout          │
└────────────────────────────────────────────┘
                    │
                    ▼
┌────────────────────────────────────────────┐
│ 4. Update Circuit State                    │
│    ├─ Failures ≥ threshold: Open circuit   │
│    ├─ In half-open, all success: Close     │
│    └─ In half-open, any failure: Reopen    │
└────────────────────────────────────────────┘
```

## Failure Detection

The circuit breaker considers the following as failures:

1. **Connection Errors**: Unable to connect to backend
2. **Timeouts**: Request exceeds `request_timeout_secs`
3. **5xx Errors**: Backend returns server error status codes

**Not considered failures:**
- 4xx client errors (bad requests, auth failures, not found)
- Successful responses (2xx, 3xx)

## Metrics

The circuit breaker tracks comprehensive metrics for each backend:

```rust
pub struct CircuitBreakerMetrics {
    pub total_requests: u64,           // Total requests attempted
    pub successful_requests: u64,       // Successful completions
    pub failed_requests: u64,           // Failed requests
    pub rejected_requests: u64,         // Rejected by open circuit
    pub timeout_count: u64,             // Timeout occurrences
    pub circuit_opened_count: u64,      // Times circuit opened
    pub circuit_closed_count: u64,      // Times circuit closed
    pub circuit_half_opened_count: u64, // Times circuit half-opened
}
```

## Complete Configuration Example

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  timeout_secs: 30

# Global circuit breaker configuration
circuit_breaker:
  failure_threshold: 5
  success_threshold: 2
  timeout_secs: 60
  half_open_requests: 3
  request_timeout_secs: 30

# Retry configuration
retry:
  max_retries: 3
  initial_backoff_ms: 100
  max_backoff_ms: 10000
  backoff_multiplier: 2.0

routes:
  - path: "/api/users"
    backend: "http://user-service:3000"
    methods: ["GET", "POST"]

  - path: "/api/orders"
    backend: "http://order-service:3001"
    methods: ["GET", "POST"]
```

## Use Cases

### 1. Protecting Against Cascading Failures

When a backend service becomes slow or unresponsive:
- Circuit breaker detects failures quickly
- Stops sending requests to failing service
- Prevents request queue buildup
- Protects gateway from resource exhaustion

### 2. Graceful Degradation

During partial system failures:
- Failing services are isolated
- Healthy services continue operating normally
- System degrades gracefully instead of total failure

### 3. Automatic Recovery

When backend services recover:
- Circuit transitions to half-open automatically
- Tests recovery with probe requests
- Resumes normal operation when stable

### 4. Preventing Resource Exhaustion

During high load or slow backends:
- Timeouts prevent indefinite waiting
- Retries with backoff prevent overwhelming systems
- Fast-fail for unavailable services

## Best Practices

### Choosing Failure Threshold

```yaml
# For critical services with low tolerance for errors
circuit_breaker:
  failure_threshold: 3

# For services with expected occasional errors
circuit_breaker:
  failure_threshold: 10
```

### Timeout Configuration

```yaml
# For fast services (microservices, caches)
circuit_breaker:
  timeout_secs: 30
  request_timeout_secs: 5

# For slower services (batch operations, reports)
circuit_breaker:
  timeout_secs: 120
  request_timeout_secs: 60
```

### Retry Configuration

```yaml
# For highly available services
retry:
  max_retries: 3
  initial_backoff_ms: 50

# For rate-limited APIs
retry:
  max_retries: 5
  initial_backoff_ms: 1000
  max_backoff_ms: 30000
```

## Monitoring & Observability

### Log Messages

The circuit breaker logs important state transitions:

```
INFO  Circuit breaker opening (consecutive_failures=5)
INFO  Circuit breaker transitioning to half-open (timeout=60s)
INFO  Circuit breaker closing (consecutive_successes=2)
WARN  Circuit breaker open, rejecting request
```

### Metrics to Monitor

1. **Circuit Open Rate**: How often circuits are opening
2. **Rejection Rate**: Percentage of requests rejected
3. **Timeout Rate**: Frequency of backend timeouts
4. **Recovery Time**: How long circuits stay open
5. **Success Rate in Half-Open**: Quality of recovery

## Testing

### Unit Tests

```bash
# Run circuit breaker unit tests
cargo test circuit_breaker

# Run with output
cargo test circuit_breaker -- --nocapture
```

### Integration Tests

```bash
# Run all integration tests
cargo test --test circuit_breaker_tests

# Run specific test
cargo test --test circuit_breaker_tests test_circuit_breaker_integration
```

### Chaos Testing

To test circuit breaker behavior:

1. **Simulate backend failures**: Stop backend service
2. **Verify circuit opens**: Check for 503 responses
3. **Test recovery**: Restart backend and verify circuit closes
4. **Load testing**: Use high load to trigger timeouts

## Troubleshooting

### Circuit Opens Too Frequently

**Symptoms**: Circuit breaker opens frequently even during normal operation

**Solutions**:
- Increase `failure_threshold`
- Increase `request_timeout_secs`
- Review backend service health
- Check if retry configuration is too aggressive

### Circuit Never Opens

**Symptoms**: Backend is clearly failing but circuit stays closed

**Solutions**:
- Verify circuit breaker is enabled in configuration
- Check if errors are being properly detected (5xx vs 4xx)
- Review `failure_threshold` setting
- Ensure timeout configuration is appropriate

### Slow Recovery After Failures

**Symptoms**: Circuit takes too long to close after backend recovers

**Solutions**:
- Reduce `timeout_secs` for faster half-open transition
- Reduce `success_threshold` for faster recovery
- Increase `half_open_requests` to test recovery faster

### Thundering Herd on Recovery

**Symptoms**: When circuit closes, backend is overwhelmed

**Solutions**:
- Reduce `half_open_requests` for gradual recovery
- Increase `initial_backoff_ms` in retry configuration
- Consider implementing rate limiting alongside circuit breaker

## Performance Impact

The circuit breaker has minimal performance overhead:

- **Closed State**: ~1-2μs per request (state check)
- **Open State**: <1μs per request (fast rejection)
- **Half-Open State**: ~2-3μs per request (probe counting)

**Memory Usage**: ~1KB per backend for circuit breaker state

## Related Features

- **Rate Limiting**: Protects against abuse and overload
- **Authentication**: Ensures only authorized requests proceed
- **Health Checks**: Active monitoring of backend availability (Phase 5)
- **Load Balancing**: Distributes load across healthy backends (Phase 5)

## References

- [Martin Fowler: Circuit Breaker](https://martinfowler.com/bliki/CircuitBreaker.html)
- [Microsoft: Circuit Breaker Pattern](https://docs.microsoft.com/en-us/azure/architecture/patterns/circuit-breaker)
- [AWS: Implementing the Circuit Breaker Pattern](https://aws.amazon.com/builders-library/timeouts-retries-and-backoff-with-jitter/)
