// Stress Test for Oxify Server
// Tests the system at and beyond normal capacity (up to 50,000 req/sec target)
//
// Usage: k6 run tests/load/stress-test.js

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

const errorRate = new Rate('errors');
const requestDuration = new Trend('request_duration');
const requests = new Counter('requests');

const TARGET_URL = __ENV.TARGET_URL || 'http://localhost:8080';

export const options = {
  stages: [
    { duration: '2m', target: 1000 },   // Ramp up to baseline
    { duration: '3m', target: 2000 },   // Increase load
    { duration: '3m', target: 5000 },   // Stress level (≈25,000 req/sec)
    { duration: '3m', target: 10000 },  // Maximum stress (≈50,000 req/sec)
    { duration: '2m', target: 5000 },   // Scale back
    { duration: '2m', target: 0 },      // Ramp down
  ],
  thresholds: {
    'http_req_duration': ['p(95)<500', 'p(99)<1000'],  // More lenient under stress
    'http_req_failed': ['rate<0.05'],                   // Allow up to 5% errors under stress
    'errors': ['rate<0.05'],
  },
};

export default function () {
  const endpoints = [
    `${TARGET_URL}/health`,
    `${TARGET_URL}/ready`,
    `${TARGET_URL}/live`,
    `${TARGET_URL}/metrics`,
  ];

  const url = endpoints[Math.floor(Math.random() * endpoints.length)];
  const response = http.get(url);

  const success = check(response, {
    'status is 200': (r) => r.status === 200,
    'response time < 500ms': (r) => r.timings.duration < 500,
  });

  errorRate.add(!success);
  requestDuration.add(response.timings.duration);
  requests.add(1);

  sleep(0.2);
}
