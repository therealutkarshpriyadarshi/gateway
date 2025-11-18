import http from 'k6/http';
import { check, sleep } from 'k6';

const BASE_URL = __ENV.GATEWAY_URL || 'http://localhost:8080';

export const options = {
  stages: [
    { duration: '1m', target: 100 },    // Normal load
    { duration: '2m', target: 200 },    // Increased load
    { duration: '2m', target: 400 },    // High load
    { duration: '2m', target: 800 },    // Very high load
    { duration: '2m', target: 1200 },   // Extreme load
    { duration: '2m', target: 1600 },   // Breaking point?
    { duration: '1m', target: 0 },      // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(50)<50', 'p(95)<200'],
    http_req_failed: ['rate<0.1'],
  },
};

export default function () {
  const res = http.get(`${BASE_URL}/health`);
  check(res, {
    'status is 200': (r) => r.status === 200,
  });
  sleep(Math.random() * 2);
}

export function handleSummary(data) {
  console.log('Finding breaking point...');
  console.log(`Max VUs: ${Math.max(...Object.values(data.metrics.vus.values))}`);
  console.log(`Max RPS: ${data.metrics.http_reqs.values.rate.toFixed(0)}`);
  console.log(`Error rate: ${(data.metrics.http_req_failed.values.rate * 100).toFixed(2)}%`);

  return {
    'stress-test-results.json': JSON.stringify(data),
  };
}
