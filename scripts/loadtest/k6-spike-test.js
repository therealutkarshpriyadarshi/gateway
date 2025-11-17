import http from 'k6/http';
import { check, sleep } from 'k6';

const BASE_URL = __ENV.GATEWAY_URL || 'http://localhost:8080';

export const options = {
  stages: [
    { duration: '10s', target: 100 },   // Normal load
    { duration: '1s', target: 1000 },   // Sudden spike!
    { duration: '30s', target: 1000 },  // Stay at spike
    { duration: '10s', target: 100 },   // Return to normal
    { duration: '10s', target: 0 },     // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(99)<100'],   // More lenient during spike
    http_req_failed: ['rate<0.05'],     // Allow 5% errors during spike
  },
};

export default function () {
  const res = http.get(`${BASE_URL}/health`);
  check(res, {
    'status is 200': (r) => r.status === 200,
  });
  sleep(0.1);
}
