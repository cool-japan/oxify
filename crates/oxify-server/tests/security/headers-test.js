// Security Headers Test for Oxify Server
// Tests for security misconfiguration (OWASP A05:2021)
//
// Usage: k6 run tests/security/headers-test.js

import http from 'k6/http';
import { check, group } from 'k6';
import { Rate } from 'k6/metrics';

const vulnerableRate = new Rate('vulnerable');
const TARGET_URL = __ENV.TARGET_URL || 'http://localhost:8080';

export const options = {
    vus: 1,
    iterations: 1,
    thresholds: {
        'vulnerable': ['rate==0'], // No vulnerabilities should be found
    },
};

export default function () {
    group('Security Headers Tests', function () {
        const res = http.get(`${TARGET_URL}/health`);

        // Test 1: Content-Security-Policy
        group('Content-Security-Policy', function () {
            const vulnerable = !check(res, {
                'CSP header present': (r) => r.headers['Content-Security-Policy'] !== undefined,
                'CSP is restrictive': (r) => {
                    const csp = r.headers['Content-Security-Policy'] || '';
                    return !(csp.includes("'unsafe-inline'") && csp.includes("'unsafe-eval'"));
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 2: X-Frame-Options
        group('X-Frame-Options', function () {
            const vulnerable = !check(res, {
                'X-Frame-Options present': (r) => r.headers['X-Frame-Options'] !== undefined,
                'X-Frame-Options is DENY or SAMEORIGIN': (r) => {
                    const xfo = r.headers['X-Frame-Options'] || '';
                    return xfo === 'DENY' || xfo === 'SAMEORIGIN';
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 3: X-Content-Type-Options
        group('X-Content-Type-Options', function () {
            const vulnerable = !check(res, {
                'X-Content-Type-Options present': (r) => r.headers['X-Content-Type-Options'] !== undefined,
                'X-Content-Type-Options is nosniff': (r) => {
                    return r.headers['X-Content-Type-Options'] === 'nosniff';
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 4: Strict-Transport-Security (HSTS)
        group('Strict-Transport-Security', function () {
            const httpsRes = http.get(`${TARGET_URL}/health`.replace('http://', 'https://'));
            const vulnerable = !check(httpsRes, {
                'HSTS header present': (r) => r.headers['Strict-Transport-Security'] !== undefined,
                'HSTS max-age is sufficient': (r) => {
                    const hsts = r.headers['Strict-Transport-Security'] || '';
                    const match = hsts.match(/max-age=(\d+)/);
                    if (!match) return false;
                    const maxAge = parseInt(match[1]);
                    return maxAge >= 31536000; // 1 year
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 5: Referrer-Policy
        group('Referrer-Policy', function () {
            const vulnerable = !check(res, {
                'Referrer-Policy present': (r) => r.headers['Referrer-Policy'] !== undefined,
                'Referrer-Policy is restrictive': (r) => {
                    const policy = r.headers['Referrer-Policy'] || '';
                    return ['no-referrer', 'same-origin', 'strict-origin', 'strict-origin-when-cross-origin'].includes(policy);
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 6: Permissions-Policy
        group('Permissions-Policy', function () {
            const vulnerable = !check(res, {
                'Permissions-Policy present': (r) => r.headers['Permissions-Policy'] !== undefined,
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 7: X-XSS-Protection
        group('X-XSS-Protection', function () {
            const vulnerable = !check(res, {
                'X-XSS-Protection present': (r) => r.headers['X-Xss-Protection'] !== undefined || r.headers['X-XSS-Protection'] !== undefined,
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 8: Server header
        group('Server Header', function () {
            const vulnerable = !check(res, {
                'Server header not exposing version': (r) => {
                    const server = r.headers['Server'] || '';
                    return !(server.includes('/') || server.match(/\d+\.\d+/));
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 9: CORS headers
        group('CORS Configuration', function () {
            const corsRes = http.options(`${TARGET_URL}/api/workflows`, null, {
                headers: {
                    'Origin': 'https://evil.com',
                    'Access-Control-Request-Method': 'GET',
                },
            });
            const vulnerable = !check(corsRes, {
                'CORS not allowing all origins': (r) => {
                    const allowOrigin = r.headers['Access-Control-Allow-Origin'] || '';
                    return allowOrigin !== '*' || allowOrigin === '';
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 10: Cache headers for sensitive endpoints
        group('Cache Control', function () {
            const authRes = http.get(`${TARGET_URL}/api/auth/login`);
            const vulnerable = !check(authRes, {
                'Sensitive endpoints have no-cache': (r) => {
                    const cacheControl = r.headers['Cache-Control'] || '';
                    return cacheControl.includes('no-cache') || cacheControl.includes('no-store');
                },
            });
            vulnerableRate.add(vulnerable);
        });
    });
}

export function handleSummary(data) {
    const vulnerableCount = data.metrics.vulnerable.values.count || 0;

    console.log(`\n=== Security Headers Test Summary ===`);
    console.log(`Vulnerabilities Found: ${vulnerableCount}`);
    console.log(`Status: ${vulnerableCount === 0 ? '✅ PASS' : '❌ FAIL'}`);
    console.log(`====================================\n`);

    if (vulnerableCount > 0) {
        console.log(`⚠️  Missing or misconfigured security headers detected!`);
    }

    return {
        'stdout': JSON.stringify(data, null, 2),
        'headers-report.json': JSON.stringify(data),
    };
}
