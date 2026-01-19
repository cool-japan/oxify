// Spike Test for Oxify Server
// Tests the system's response to sudden traffic spikes
//
// Usage: k6 run tests/load/spike-test.js

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

const errorRate = new Rate('errors');
const requestDuration = new Trend('request_duration');
const requests = new Counter('requests');

const TARGET_URL = __ENV.TARGET_URL || 'http://localhost:8080';

export const options = {
  stages: [
    { duration: '1m', target: 500 },    // Normal load
    { duration: '30s', target: 10000 }, // Sudden spike!
    { duration: '3m', target: 10000 },  // Maintain spike
    { duration: '1m', target: 500 },    // Recovery
    { duration: '30s', target: 15000 }, // Second spike!
    { duration: '3m', target: 15000 },  // Maintain second spike
    { duration: '2m', target: 0 },      // Ramp down
  ],
  thresholds: {
    'http_req_duration': ['p(95)<1000', 'p(99)<2000'],  // Allow degradation during spike
    'http_req_failed': ['rate<0.1'],                     // Allow up to 10% errors during spike
    'errors': ['rate<0.1'],
  },
};

export default function () {
  const endpoints = [
    `${TARGET_URL}/health`,
    `${TARGET_URL}/ready`,
    `${TARGET_URL}/live`,
  ];

  const url = endpoints[Math.floor(Math.random() * endpoints.length)];
  const response = http.get(url);

  const success = check(response, {
    'status is 200': (r) => r.status === 200,
  });

  errorRate.add(!success);
  requestDuration.add(response.timings.duration);
  requests.add(1);

  sleep(0.1);
}
