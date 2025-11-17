import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const apiDuration = new Trend('api_duration');
const requestCount = new Counter('requests');

// Configuration
const BASE_URL = __ENV.GATEWAY_URL || 'http://localhost:8080';

export const options = {
  stages: [
    { duration: '30s', target: 50 },   // Ramp up to 50 users
    { duration: '1m', target: 100 },   // Ramp up to 100 users
    { duration: '2m', target: 100 },   // Stay at 100 users
    { duration: '1m', target: 200 },   // Spike to 200 users
    { duration: '2m', target: 200 },   // Stay at 200 users
    { duration: '30s', target: 0 },    // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<15', 'p(99)<50'],  // 95% < 15ms, 99% < 50ms
    http_req_failed: ['rate<0.01'],                // Error rate < 1%
    errors: ['rate<0.01'],
  },
};

export default function () {
  // Test 1: Health check
  {
    const res = http.get(`${BASE_URL}/health`);
    const success = check(res, {
      'health check status is 200': (r) => r.status === 200,
      'health check response time < 10ms': (r) => r.timings.duration < 10,
    });
    errorRate.add(!success);
    requestCount.add(1);
  }

  sleep(0.1);

  // Test 2: API endpoint
  {
    const res = http.get(`${BASE_URL}/api/users`, {
      headers: {
        'Content-Type': 'application/json',
      },
    });
    const success = check(res, {
      'api status is 200 or 502': (r) => r.status === 200 || r.status === 502,
      'api response time < 30ms': (r) => r.timings.duration < 30,
    });
    apiDuration.add(res.timings.duration);
    errorRate.add(!success);
    requestCount.add(1);
  }

  sleep(0.5);

  // Test 3: Metrics endpoint
  {
    const res = http.get(`${BASE_URL}/metrics`);
    check(res, {
      'metrics status is 200': (r) => r.status === 200,
      'metrics contains data': (r) => r.body.length > 0,
    });
    requestCount.add(1);
  }

  sleep(1);
}

export function handleSummary(data) {
  return {
    'summary.json': JSON.stringify(data),
    stdout: textSummary(data, { indent: ' ', enableColors: true }),
  };
}

function textSummary(data, options) {
  const indent = options.indent || '';
  const enableColors = options.enableColors || false;

  let summary = '\n';
  summary += `${indent}Summary:\n`;
  summary += `${indent}  Requests: ${data.metrics.requests.values.count}\n`;
  summary += `${indent}  Errors: ${(data.metrics.errors.values.rate * 100).toFixed(2)}%\n`;
  summary += `${indent}  Duration:\n`;
  summary += `${indent}    p50: ${data.metrics.http_req_duration.values['p(50)'].toFixed(2)}ms\n`;
  summary += `${indent}    p95: ${data.metrics.http_req_duration.values['p(95)'].toFixed(2)}ms\n`;
  summary += `${indent}    p99: ${data.metrics.http_req_duration.values['p(99)'].toFixed(2)}ms\n`;

  return summary;
}
