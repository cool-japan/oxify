// Soak Test for Oxify Server
// Tests the system's stability over an extended period (reliability test)
//
// Usage: k6 run tests/load/soak-test.js

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

const errorRate = new Rate('errors');
const requestDuration = new Trend('request_duration');
const requests = new Counter('requests');

const TARGET_URL = __ENV.TARGET_URL || 'http://localhost:8080';

export const options = {
  stages: [
    { duration: '5m', target: 1000 },   // Ramp up to baseline
    { duration: '2h', target: 1000 },   // Stay at baseline for 2 hours
    { duration: '5m', target: 0 },      // Ramp down
  ],
  thresholds: {
    'http_req_duration': ['p(95)<100', 'p(99)<200'],  // Performance should remain stable
    'http_req_failed': ['rate<0.01'],                  // Low error rate over time
    'errors': ['rate<0.01'],
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
    'response time < 200ms': (r) => r.timings.duration < 200,
    'no memory leaks': (r) => r.status !== 500, // 500 errors might indicate memory issues
  });

  errorRate.add(!success);
  requestDuration.add(response.timings.duration);
  requests.add(1);

  sleep(0.2);
}

export function handleSummary(data) {
  console.log('Soak Test Summary:');
  console.log(`Total Duration: ${data.state.testRunDurationMs / 1000 / 60} minutes`);
  console.log(`Total Requests: ${data.metrics.requests.values.count}`);
  console.log(`Error Rate: ${((data.metrics.errors.values.rate || 0) * 100).toFixed(2)}%`);
  console.log(`Avg Response Time: ${(data.metrics.http_req_duration.values.avg || 0).toFixed(2)}ms`);
  console.log(`P95 Response Time: ${(data.metrics.http_req_duration.values['p(95)'] || 0).toFixed(2)}ms`);

  return {
    'stdout': JSON.stringify(data, null, 2),
    'soak-test-summary.json': JSON.stringify(data),
  };
}
