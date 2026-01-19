// Basic Load Test for Oxify Server
// Tests the baseline performance at 5,000 req/sec
//
// Usage: k6 run tests/load/basic-load-test.js
// With custom target: k6 run -e TARGET_URL=http://localhost:8080 tests/load/basic-load-test.js

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const requestDuration = new Trend('request_duration');
const requests = new Counter('requests');

// Configuration
const TARGET_URL = __ENV.TARGET_URL || 'http://localhost:8080';

export const options = {
  stages: [
    { duration: '1m', target: 500 },   // Ramp up to 500 VUs
    { duration: '3m', target: 1000 },  // Ramp up to 1000 VUs (≈5,000 req/sec)
    { duration: '5m', target: 1000 },  // Stay at 1000 VUs
    { duration: '1m', target: 0 },     // Ramp down
  ],
  thresholds: {
    'http_req_duration': ['p(95)<100', 'p(99)<200'],  // 95% < 100ms, 99% < 200ms
    'http_req_failed': ['rate<0.01'],                  // Error rate < 1%
    'errors': ['rate<0.01'],
  },
};

export default function () {
  // Test various endpoints
  const requests_to_test = [
    { method: 'GET', url: `${TARGET_URL}/health`, name: 'health_check' },
    { method: 'GET', url: `${TARGET_URL}/ready`, name: 'ready_check' },
    { method: 'GET', url: `${TARGET_URL}/live`, name: 'live_check' },
    { method: 'GET', url: `${TARGET_URL}/metrics`, name: 'metrics' },
  ];

  // Random selection of endpoint to test
  const req = requests_to_test[Math.floor(Math.random() * requests_to_test.length)];

  const response = http.get(req.url, {
    tags: { name: req.name },
  });

  // Check response
  const success = check(response, {
    'status is 200': (r) => r.status === 200,
    'response time < 100ms': (r) => r.timings.duration < 100,
  });

  // Record metrics
  errorRate.add(!success);
  requestDuration.add(response.timings.duration);
  requests.add(1);

  // Small sleep to simulate realistic load (5 requests per VU per second)
  sleep(0.2);
}

export function handleSummary(data) {
  return {
    'stdout': textSummary(data, { indent: ' ', enableColors: true }),
    'summary.json': JSON.stringify(data),
  };
}

function textSummary(data, options = {}) {
  const indent = options.indent || '';
  const enableColors = options.enableColors || false;

  const green = enableColors ? '\x1b[32m' : '';
  const red = enableColors ? '\x1b[31m' : '';
  const yellow = enableColors ? '\x1b[33m' : '';
  const reset = enableColors ? '\x1b[0m' : '';

  let summary = '\n';
  summary += `${indent}========== Load Test Summary ==========\n`;
  summary += `${indent}Total Requests: ${data.metrics.requests.values.count}\n`;
  summary += `${indent}Request Rate: ${(data.metrics.http_reqs.values.rate || 0).toFixed(2)} req/sec\n`;
  summary += `${indent}Error Rate: ${((data.metrics.errors.values.rate || 0) * 100).toFixed(2)}%\n`;
  summary += `${indent}P95 Duration: ${(data.metrics.http_req_duration.values['p(95)'] || 0).toFixed(2)}ms\n`;
  summary += `${indent}P99 Duration: ${(data.metrics.http_req_duration.values['p(99)'] || 0).toFixed(2)}ms\n`;
  summary += `${indent}=======================================\n`;

  return summary;
}
