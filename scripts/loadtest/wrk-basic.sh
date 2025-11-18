#!/bin/bash
# Basic load test using wrk

set -e

GATEWAY_URL="${GATEWAY_URL:-http://localhost:8080}"
DURATION="${DURATION:-30s}"
THREADS="${THREADS:-12}"
CONNECTIONS="${CONNECTIONS:-400}"

echo "================================================"
echo "API Gateway Load Test - Basic"
echo "================================================"
echo "URL: $GATEWAY_URL"
echo "Duration: $DURATION"
echo "Threads: $THREADS"
echo "Connections: $CONNECTIONS"
echo "================================================"

# Test 1: Health endpoint
echo ""
echo "Test 1: Health Endpoint"
wrk -t$THREADS -c$CONNECTIONS -d$DURATION \
    --latency \
    $GATEWAY_URL/health

# Test 2: API endpoint (if backend available)
echo ""
echo "Test 2: API Endpoint"
wrk -t$THREADS -c$CONNECTIONS -d$DURATION \
    --latency \
    $GATEWAY_URL/api/users || echo "Skipped (no backend)"

# Test 3: Higher load
echo ""
echo "Test 3: High Load (800 connections)"
wrk -t$THREADS -c800 -d$DURATION \
    --latency \
    $GATEWAY_URL/health

echo ""
echo "================================================"
echo "Load test completed!"
echo "================================================"
