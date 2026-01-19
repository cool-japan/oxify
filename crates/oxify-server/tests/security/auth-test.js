// Authentication & Authorization Security Test for Oxify Server
// Tests for broken authentication (OWASP A07:2021)
//
// Usage: k6 run tests/security/auth-test.js

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

export default function () {
    group('Authentication Tests', function () {
        // Test 1: Access protected endpoint without authentication
        group('Missing Auth Token', function () {
            const res = http.get(`${TARGET_URL}/api/workflows/protected`);
            const vulnerable = !check(res, {
                'returns 401 Unauthorized': (r) => r.status === 401,
                'does not leak data': (r) => {
                    return !(
                        r.body.includes('password') ||
                        r.body.includes('secret') ||
                        r.body.includes('api_key')
                    );
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 2: Invalid JWT token
        group('Invalid JWT Token', function () {
            const res = http.get(`${TARGET_URL}/api/workflows/protected`, {
                headers: {
                    'Authorization': 'Bearer invalid.token.here',
                },
            });
            const vulnerable = !check(res, {
                'rejects invalid token': (r) => r.status === 401 || r.status === 403,
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 3: Expired token
        group('Expired Token', function () {
            // Expired JWT (exp: 2020-01-01)
            const expiredToken = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyLCJleHAiOjE1Nzc4MzY4MDB9.invalid';
            const res = http.get(`${TARGET_URL}/api/workflows/protected`, {
                headers: {
                    'Authorization': `Bearer ${expiredToken}`,
                },
            });
            const vulnerable = !check(res, {
                'rejects expired token': (r) => r.status === 401 || r.status === 403,
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 4: Brute force protection
        group('Brute Force Protection', function () {
            const failedAttempts = [];
            for (let i = 0; i < 20; i++) {
                const res = http.post(`${TARGET_URL}/api/auth/login`, JSON.stringify({
                    username: 'admin',
                    password: `wrong_password_${i}`,
                }), {
                    headers: { 'Content-Type': 'application/json' },
                });
                failedAttempts.push(res.status);
            }

            const vulnerable = !check(failedAttempts, {
                'rate limiting after multiple failures': (attempts) => {
                    // Should start returning 429 (Too Many Requests) after several failures
                    const lastFew = attempts.slice(-5);
                    return lastFew.some(status => status === 429);
                },
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 5: Session fixation
        group('Session Security', function () {
            const res = http.get(`${TARGET_URL}/api/auth/session`, {
                headers: {
                    'Cookie': 'session_id=attacker_controlled_session',
                },
            });
            const vulnerable = !check(res, {
                'rejects attacker session': (r) => r.status === 401 || r.status === 403,
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 6: Password in URL
        group('Sensitive Data in URL', function () {
            const res = http.post(`${TARGET_URL}/api/auth/login?password=secret123`, JSON.stringify({
                username: 'admin',
            }), {
                headers: { 'Content-Type': 'application/json' },
            });
            const vulnerable = !check(res, {
                'rejects password in URL': (r) => r.status === 400 || r.status === 422,
            });
            vulnerableRate.add(vulnerable);
        });
    });

    group('Authorization Tests', function () {
        // Test 7: Horizontal privilege escalation
        group('Horizontal Privilege Escalation', function () {
            const res = http.get(`${TARGET_URL}/api/users/other_user_id/profile`, {
                headers: {
                    'Authorization': 'Bearer user1_token',
                },
            });
            const vulnerable = !check(res, {
                'prevents access to other users data': (r) => r.status === 403 || r.status === 404,
            });
            vulnerableRate.add(vulnerable);
        });

        // Test 8: Vertical privilege escalation
        group('Vertical Privilege Escalation', function () {
            const res = http.post(`${TARGET_URL}/api/admin/users`, JSON.stringify({
                username: 'new_admin',
                role: 'admin',
            }), {
                headers: {
                    'Authorization': 'Bearer regular_user_token',
                    'Content-Type': 'application/json',
                },
            });
            const vulnerable = !check(res, {
                'prevents regular user from admin actions': (r) => r.status === 403,
            });
            vulnerableRate.add(vulnerable);
        });
    });
}

export function handleSummary(data) {
    const vulnerableCount = data.metrics.vulnerable.values.count || 0;

    console.log(`\n=== Authentication & Authorization Security Test Summary ===`);
    console.log(`Vulnerabilities Found: ${vulnerableCount}`);
    console.log(`Status: ${vulnerableCount === 0 ? '✅ PASS' : '❌ FAIL'}`);
    console.log(`===========================================================\n`);

    return {
        'stdout': JSON.stringify(data, null, 2),
        'auth-report.json': JSON.stringify(data),
    };
}
