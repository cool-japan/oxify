// k6 Load Testing Script for Authorization Engine
//
// Run with:
//   k6 run loadtest/authz_load_test.js
//
// Custom configuration:
//   k6 run --vus 100 --duration 60s loadtest/authz_load_test.js
//
// Smoke test:
//   k6 run --vus 1 --duration 10s loadtest/authz_load_test.js
//
// Stress test:
//   k6 run --vus 500 --duration 300s loadtest/authz_load_test.js

import http from 'k6/http';
import { check, group, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const checkLatency = new Trend('authz_check_latency', true);
const writeLatency = new Trend('authz_write_latency', true);
const batchLatency = new Trend('authz_batch_latency', true);
const checkSuccess = new Rate('authz_check_success');
const writeSuccess = new Rate('authz_write_success');
const totalChecks = new Counter('total_authorization_checks');
const totalWrites = new Counter('total_tuple_writes');

// Configuration
const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const API_PREFIX = '/api/v1/authz';

// Load test stages
export const options = {
  stages: [
    { duration: '30s', target: 50 },   // Ramp up to 50 users
    { duration: '1m', target: 100 },   // Ramp up to 100 users
    { duration: '2m', target: 100 },   // Stay at 100 users
    { duration: '30s', target: 200 },  // Spike to 200 users
    { duration: '1m', target: 200 },   // Stay at 200 users
    { duration: '30s', target: 0 },    // Ramp down to 0 users
  ],
  thresholds: {
    // Authorization checks should complete in <100ms (p95)
    'authz_check_latency': ['p(95)<100', 'p(99)<200'],
    // Batch checks should complete in <500ms (p95)
    'authz_batch_latency': ['p(95)<500', 'p(99)<1000'],
    // Write operations should complete in <50ms (p95)
    'authz_write_latency': ['p(95)<50', 'p(99)<100'],
    // Success rates should be >99%
    'authz_check_success': ['rate>0.99'],
    'authz_write_success': ['rate>0.99'],
    // HTTP error rate should be <1%
    'http_req_failed': ['rate<0.01'],
  },
};

// Test data generators
function randomUser() {
  const userId = Math.floor(Math.random() * 1000);
  return `user:${userId}`;
}

function randomDocument() {
  const docId = Math.floor(Math.random() * 10000);
  return {
    namespace: 'document',
    object_id: `doc${docId}`,
  };
}

function randomRelation() {
  const relations = ['viewer', 'editor', 'owner', 'commenter'];
  return relations[Math.floor(Math.random() * relations.length)];
}

// Setup: Create initial test data
export function setup() {
  console.log('Setting up test data...');

  const tuples = [];
  for (let i = 0; i < 1000; i++) {
    tuples.push({
      namespace: 'document',
      object_id: `doc${i}`,
      relation: 'viewer',
      subject: { User: `user${i % 100}` },
    });
  }

  // Batch write initial data
  const res = http.post(
    `${BASE_URL}${API_PREFIX}/tuples/batch`,
    JSON.stringify({ tuples }),
    { headers: { 'Content-Type': 'application/json' } }
  );

  if (res.status === 200 || res.status === 201) {
    console.log(`Setup complete: Created ${tuples.length} tuples`);
  } else {
    console.error(`Setup failed: ${res.status} ${res.body}`);
  }

  return { setupComplete: true };
}

// Main test scenario
export default function () {
  // Simulate realistic user behavior with mixed operations

  group('authorization_check', () => {
    const doc = randomDocument();
    const user = randomUser();
    const relation = randomRelation();

    const payload = {
      namespace: doc.namespace,
      object_id: doc.object_id,
      relation: relation,
      subject: { User: user },
    };

    const res = http.post(
      `${BASE_URL}${API_PREFIX}/check`,
      JSON.stringify(payload),
      {
        headers: { 'Content-Type': 'application/json' },
        tags: { name: 'AuthzCheck' },
      }
    );

    const success = check(res, {
      'status is 200': (r) => r.status === 200,
      'response has allowed field': (r) => {
        try {
          const body = JSON.parse(r.body);
          return body.hasOwnProperty('allowed');
        } catch {
          return false;
        }
      },
      'latency < 100ms': (r) => r.timings.duration < 100,
    });

    checkLatency.add(res.timings.duration);
    checkSuccess.add(success);
    totalChecks.add(1);
  });

  sleep(0.1);

  // Write new tuple (10% of requests)
  if (Math.random() < 0.1) {
    group('tuple_write', () => {
      const doc = randomDocument();
      const user = randomUser();
      const relation = randomRelation();

      const payload = {
        namespace: doc.namespace,
        object_id: doc.object_id,
        relation: relation,
        subject: { User: user },
      };

      const res = http.post(
        `${BASE_URL}${API_PREFIX}/tuples`,
        JSON.stringify(payload),
        {
          headers: { 'Content-Type': 'application/json' },
          tags: { name: 'TupleWrite' },
        }
      );

      const success = check(res, {
        'status is 200 or 201': (r) => r.status === 200 || r.status === 201,
        'latency < 50ms': (r) => r.timings.duration < 50,
      });

      writeLatency.add(res.timings.duration);
      writeSuccess.add(success);
      totalWrites.add(1);
    });

    sleep(0.1);
  }

  // Batch check (5% of requests)
  if (Math.random() < 0.05) {
    group('batch_check', () => {
      const checks = [];
      const batchSize = 10;

      for (let i = 0; i < batchSize; i++) {
        const doc = randomDocument();
        const user = randomUser();
        const relation = randomRelation();

        checks.push({
          namespace: doc.namespace,
          object_id: doc.object_id,
          relation: relation,
          subject: { User: user },
        });
      }

      const res = http.post(
        `${BASE_URL}${API_PREFIX}/batch-check`,
        JSON.stringify({ checks }),
        {
          headers: { 'Content-Type': 'application/json' },
          tags: { name: 'BatchCheck' },
        }
      );

      check(res, {
        'status is 200': (r) => r.status === 200,
        'response has results array': (r) => {
          try {
            const body = JSON.parse(r.body);
            return Array.isArray(body.results) && body.results.length === batchSize;
          } catch {
            return false;
          }
        },
        'latency < 500ms': (r) => r.timings.duration < 500,
      });

      batchLatency.add(res.timings.duration);
      totalChecks.add(batchSize);
    });

    sleep(0.1);
  }

  // Expand query (2% of requests)
  if (Math.random() < 0.02) {
    group('expand_subjects', () => {
      const doc = randomDocument();
      const relation = randomRelation();

      const res = http.get(
        `${BASE_URL}${API_PREFIX}/expand?namespace=${doc.namespace}&object_id=${doc.object_id}&relation=${relation}`,
        { tags: { name: 'ExpandSubjects' } }
      );

      check(res, {
        'status is 200': (r) => r.status === 200,
        'response has subjects array': (r) => {
          try {
            const body = JSON.parse(r.body);
            return Array.isArray(body.subjects);
          } catch {
            return false;
          }
        },
      });
    });

    sleep(0.1);
  }
}

// Teardown: Clean up test data (optional)
export function teardown(data) {
  if (data.setupComplete) {
    console.log('Load test complete. Check k6 summary for results.');
    console.log(`Total authorization checks: ${totalChecks.value}`);
    console.log(`Total tuple writes: ${totalWrites.value}`);
  }
}

// Custom summary handler
export function handleSummary(data) {
  return {
    'loadtest/summary.json': JSON.stringify(data, null, 2),
    'stdout': textSummary(data, { indent: ' ', enableColors: true }),
  };
}

function textSummary(data, opts) {
  const indent = opts.indent || '';
  const enableColors = opts.enableColors || false;

  let output = '\n';
  output += `${indent}═══════════════════════════════════════════════\n`;
  output += `${indent}  Authorization Engine Load Test Summary\n`;
  output += `${indent}═══════════════════════════════════════════════\n\n`;

  // Add custom metrics summary
  if (data.metrics.authz_check_latency) {
    output += `${indent}Authorization Checks:\n`;
    output += `${indent}  p50: ${data.metrics.authz_check_latency.values['p(50)'].toFixed(2)}ms\n`;
    output += `${indent}  p95: ${data.metrics.authz_check_latency.values['p(95)'].toFixed(2)}ms\n`;
    output += `${indent}  p99: ${data.metrics.authz_check_latency.values['p(99)'].toFixed(2)}ms\n\n`;
  }

  if (data.metrics.authz_write_latency) {
    output += `${indent}Tuple Writes:\n`;
    output += `${indent}  p50: ${data.metrics.authz_write_latency.values['p(50)'].toFixed(2)}ms\n`;
    output += `${indent}  p95: ${data.metrics.authz_write_latency.values['p(95)'].toFixed(2)}ms\n`;
    output += `${indent}  p99: ${data.metrics.authz_write_latency.values['p(99)'].toFixed(2)}ms\n\n`;
  }

  if (data.metrics.total_authorization_checks) {
    output += `${indent}Total Checks: ${data.metrics.total_authorization_checks.values.count}\n`;
  }

  if (data.metrics.total_tuple_writes) {
    output += `${indent}Total Writes: ${data.metrics.total_tuple_writes.values.count}\n`;
  }

  output += `\n${indent}═══════════════════════════════════════════════\n`;

  return output;
}
