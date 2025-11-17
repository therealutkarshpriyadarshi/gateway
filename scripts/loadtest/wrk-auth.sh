#!/bin/bash
# Load test with JWT authentication

set -e

GATEWAY_URL="${GATEWAY_URL:-http://localhost:8080}"
DURATION="${DURATION:-30s}"
THREADS="${THREADS:-12}"
CONNECTIONS="${CONNECTIONS:-400}"
JWT_TOKEN="${JWT_TOKEN:-eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyMTIzIiwiZXhwIjoxOTk5OTk5OTk5fQ.placeholder}"

echo "================================================"
echo "API Gateway Load Test - With Authentication"
echo "================================================"
echo "URL: $GATEWAY_URL"
echo "Duration: $DURATION"
echo "Threads: $THREADS"
echo "Connections: $CONNECTIONS"
echo "================================================"

# Create wrk script with JWT header
cat > /tmp/wrk-auth.lua <<EOF
wrk.method = "GET"
wrk.headers["Authorization"] = "Bearer $JWT_TOKEN"
EOF

echo ""
echo "Test: Authenticated Endpoint"
wrk -t$THREADS -c$CONNECTIONS -d$DURATION \
    --latency \
    -s /tmp/wrk-auth.lua \
    $GATEWAY_URL/api/protected || echo "Skipped (no protected endpoint)"

# Cleanup
rm /tmp/wrk-auth.lua

echo ""
echo "================================================"
echo "Authenticated load test completed!"
echo "================================================"
