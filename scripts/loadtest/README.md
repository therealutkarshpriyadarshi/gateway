# Load Testing Scripts

This directory contains various load testing scripts for the API Gateway.

## Prerequisites

Install one or more of the following tools:

- **wrk**: `brew install wrk` or `apt-get install wrk`
- **hey**: `go install github.com/rakyll/hey@latest`
- **ab** (ApacheBench): Usually pre-installed or `apt-get install apache2-utils`
- **k6**: See https://k6.io/docs/getting-started/installation/

## Quick Start

```bash
# Basic load test with wrk
./wrk-basic.sh

# Load test with authentication
./wrk-auth.sh

# Comprehensive k6 test
k6 run k6-load-test.js

# Spike test
k6 run k6-spike-test.js
```

## Test Scenarios

### 1. Basic Load Test
Tests basic routing and proxying with increasing load.

```bash
./wrk-basic.sh
```

### 2. Authentication Load Test
Tests authenticated endpoints with JWT tokens.

```bash
./wrk-auth.sh
```

### 3. Comprehensive K6 Test
Full featured load test with multiple scenarios, virtual users, and checks.

```bash
k6 run k6-load-test.js
```

### 4. Spike Test
Tests gateway behavior under sudden traffic spikes.

```bash
k6 run k6-spike-test.js
```

### 5. Stress Test
Tests gateway limits and breaking points.

```bash
k6 run k6-stress-test.js
```

## Expected Performance Metrics

- **Latency**: p50 < 5ms, p95 < 15ms, p99 < 50ms
- **Throughput**: >5,000 req/s (single instance)
- **Error Rate**: <0.1% under normal load
- **Memory**: <512MB under load

## Analyzing Results

### wrk Output
```
Running 30s test @ http://localhost:8080/health
  12 threads and 400 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency     4.32ms    2.15ms  45.21ms   87.34%
    Req/Sec     7.82k     1.21k   11.23k    69.23%
  2812347 requests in 30.01s, 421.45MB read
Requests/sec:  93733.45
Transfer/sec:     14.05MB
```

### k6 Output
Look for:
- `http_req_duration`: Request latency percentiles
- `http_reqs`: Total requests and rate
- `http_req_failed`: Error rate
- Custom checks and thresholds

## Troubleshooting

### Connection Refused
- Ensure gateway is running: `curl http://localhost:8080/health`
- Check port mapping in Docker/Kubernetes

### High Error Rates
- Check backend services are running
- Review rate limiting configuration
- Check circuit breaker settings

### Poor Performance
- Enable debug logging: `RUST_LOG=debug`
- Check resource limits (CPU, memory)
- Review metrics: `curl http://localhost:8080/metrics`
