// SQL Injection Security Test for Oxify Server
// Tests for SQL injection vulnerabilities (OWASP A03:2021)
//
// Usage: k6 run tests/security/sql-injection-test.js

import http from 'k6/http';
import { check, group } from 'k6';
import { Rate } from 'k6/metrics';

const vulnerableRate = new Rate('vulnerable');
const TARGET_URL = __ENV.TARGET_URL || 'http://localhost:8080';

export const options = {
    vus: 1,
    duration: '30s',
    thresholds: {
        'vulnerable': ['rate==0'], // No vulnerabilities should be found
    },
};

// Common SQL injection payloads
const SQL_INJECTION_PAYLOADS = [
    "' OR '1'='1",
    "' OR '1'='1' --",
    "' OR '1'='1' /*",
    "admin'--",
    "admin' #",
    "admin'/*",
    "' or 1=1--",
    "' or 1=1#",
    "' or 1=1/*",
    "') or '1'='1--",
    "') or ('1'='1--",
    "1' UNION SELECT NULL--",
    "1' UNION SELECT NULL, NULL--",
    "1' UNION SELECT NULL, NULL, NULL--",
    "' AND 1=0 UNION ALL SELECT 'admin', '81dc9bdb52d04dc20036dbd8313ed055'",
    "1'; DROP TABLE users--",
    "1'; DELETE FROM users--",
    "1' AND SLEEP(5)--",
    "1' WAITFOR DELAY '0:0:5'--",
];

export default function () {
    group('SQL Injection Tests', function () {
        SQL_INJECTION_PAYLOADS.forEach((payload, index) => {
            // Test query parameters
            const url1 = `${TARGET_URL}/api/workflows?id=${encodeURIComponent(payload)}`;
            const res1 = http.get(url1);

            const vulnerable1 = check(res1, {
                'no SQL error messages': (r) => {
                    const body = r.body.toLowerCase();
                    return !(
                        body.includes('sql') ||
                        body.includes('mysql') ||
                        body.includes('postgresql') ||
                        body.includes('ora-') ||
                        body.includes('sqlite') ||
                        body.includes('syntax error') ||
                        body.includes('database error') ||
                        body.includes('query failed')
                    );
                },
                'proper error code': (r) => r.status === 400 || r.status === 404 || r.status === 422,
            });

            vulnerableRate.add(!vulnerable1);

            // Test POST body
            const res2 = http.post(`${TARGET_URL}/api/workflows`, JSON.stringify({
                name: payload,
                description: payload,
            }), {
                headers: { 'Content-Type': 'application/json' },
            });

            const vulnerable2 = check(res2, {
                'no SQL error in POST': (r) => {
                    const body = r.body.toLowerCase();
                    return !(
                        body.includes('sql') ||
                        body.includes('mysql') ||
                        body.includes('postgresql') ||
                        body.includes('syntax error')
                    );
                },
            });

            vulnerableRate.add(!vulnerable2);
        });
    });
}

export function handleSummary(data) {
    const vulnerableCount = data.metrics.vulnerable.values.count || 0;
    const totalTests = SQL_INJECTION_PAYLOADS.length * 2;

    console.log(`\n=== SQL Injection Security Test Summary ===`);
    console.log(`Total Tests: ${totalTests}`);
    console.log(`Vulnerabilities Found: ${vulnerableCount}`);
    console.log(`Status: ${vulnerableCount === 0 ? '✅ PASS' : '❌ FAIL'}`);
    console.log(`==========================================\n`);

    return {
        'stdout': JSON.stringify(data, null, 2),
        'sql-injection-report.json': JSON.stringify(data),
    };
}
