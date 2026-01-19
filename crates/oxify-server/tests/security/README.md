# Security Testing for Oxify Server

This directory contains security testing scripts based on OWASP Top 10 vulnerabilities.

## Prerequisites

Install k6:
```bash
# macOS
brew install k6

# Linux (Debian/Ubuntu)
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update
sudo apt-get install k6

# Windows (using Chocolatey)
choco install k6
```

## Test Categories

### 1. SQL Injection Test (`sql-injection-test.js`)
Tests for SQL injection vulnerabilities (OWASP A03:2021 - Injection).

**What it tests:**
- SQL injection in query parameters
- SQL injection in POST body
- Error message disclosure
- Proper input validation

**Run:**
```bash
k6 run tests/security/sql-injection-test.js
```

**Expected Result:** All tests should pass with no SQL errors leaked.

### 2. XSS Test (`xss-test.js`)
Tests for Cross-Site Scripting vulnerabilities (OWASP A03:2021 - Injection).

**What it tests:**
- Reflected XSS in query parameters
- Stored XSS in POST data
- HTML entity encoding
- Content-Type headers
- Script injection prevention

**Run:**
```bash
k6 run tests/security/xss-test.js
```

**Expected Result:** All XSS payloads should be sanitized or rejected.

### 3. Authentication & Authorization Test (`auth-test.js`)
Tests for broken authentication (OWASP A07:2021 - Identification and Authentication Failures).

**What it tests:**
- Missing authentication tokens
- Invalid JWT tokens
- Expired tokens
- Brute force protection
- Session security
- Horizontal privilege escalation
- Vertical privilege escalation

**Run:**
```bash
k6 run tests/security/auth-test.js
```

**Expected Result:** Unauthorized requests should be blocked with 401/403 status codes.

### 4. Security Headers Test (`headers-test.js`)
Tests for security misconfiguration (OWASP A05:2021 - Security Misconfiguration).

**What it tests:**
- Content-Security-Policy (CSP)
- X-Frame-Options
- X-Content-Type-Options
- Strict-Transport-Security (HSTS)
- Referrer-Policy
- Permissions-Policy
- X-XSS-Protection
- Server header information disclosure
- CORS configuration
- Cache control for sensitive endpoints

**Run:**
```bash
k6 run tests/security/headers-test.js
```

**Expected Result:** All security headers should be present and properly configured.

## Running All Security Tests

To run all security tests in sequence:

```bash
#!/bin/bash
echo "Running Security Test Suite..."
echo "=============================="

k6 run tests/security/sql-injection-test.js
k6 run tests/security/xss-test.js
k6 run tests/security/auth-test.js
k6 run tests/security/headers-test.js

echo "=============================="
echo "Security Test Suite Complete"
```

Save this as `run-all-security-tests.sh` and make it executable:
```bash
chmod +x run-all-security-tests.sh
./run-all-security-tests.sh
```

## Custom Configuration

All tests support custom target URL:

```bash
k6 run -e TARGET_URL=https://api.oxify.example.com tests/security/sql-injection-test.js
```

## OWASP Top 10 2021 Coverage

| OWASP Category | Test Coverage | Status |
|---------------|--------------|--------|
| A01:2021 - Broken Access Control | auth-test.js | ✅ |
| A02:2021 - Cryptographic Failures | headers-test.js (HSTS) | ✅ |
| A03:2021 - Injection | sql-injection-test.js, xss-test.js | ✅ |
| A04:2021 - Insecure Design | Manual review required | 📝 |
| A05:2021 - Security Misconfiguration | headers-test.js | ✅ |
| A06:2021 - Vulnerable Components | Dependency audit (cargo audit) | 📝 |
| A07:2021 - Identification/Authentication Failures | auth-test.js | ✅ |
| A08:2021 - Software/Data Integrity Failures | Manual review required | 📝 |
| A09:2021 - Security Logging/Monitoring Failures | logs/ directory review | 📝 |
| A10:2021 - Server-Side Request Forgery (SSRF) | Future enhancement | ⏳ |

## Interpreting Results

### Passed Test
```
✅ PASS - No vulnerabilities found
```

### Failed Test
```
❌ FAIL - X vulnerabilities found
```

When a test fails, review the detailed output for:
1. Which specific payload triggered the vulnerability
2. The response status code
3. The response body content
4. Any error messages disclosed

## Best Practices

### Before Production

1. **Run all security tests** in staging environment
2. **Fix all vulnerabilities** before deploying
3. **Implement rate limiting** for auth endpoints
4. **Use parameterized queries** to prevent SQL injection
5. **Sanitize all user input** to prevent XSS
6. **Implement proper authentication** with JWT
7. **Add all security headers** recommended by tests
8. **Enable HTTPS only** in production
9. **Regular security audits** with automated tools
10. **Keep dependencies updated** (cargo audit)

### Continuous Security

Integrate security tests into CI/CD:

```yaml
# Example: GitHub Actions
- name: Security Tests
  run: |
    k6 run tests/security/sql-injection-test.js
    k6 run tests/security/xss-test.js
    k6 run tests/security/auth-test.js
    k6 run tests/security/headers-test.js
```

### Additional Tools

Complement these tests with:

- **cargo audit** - Check for vulnerable dependencies
- **ZAP (OWASP Zed Attack Proxy)** - Dynamic application security testing
- **Burp Suite** - Manual penetration testing
- **Trivy** - Container image scanning
- **Dependabot** - Automated dependency updates

## Vulnerability Disclosure

If you discover a security vulnerability in Oxify Server:

1. **DO NOT** open a public GitHub issue
2. Email security@oxify.example.com with details
3. Include steps to reproduce
4. Allow 90 days for patching before public disclosure

## References

- [OWASP Top 10 2021](https://owasp.org/www-project-top-ten/)
- [OWASP Testing Guide](https://owasp.org/www-project-web-security-testing-guide/)
- [k6 Documentation](https://k6.io/docs/)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)

## License

Apache-2.0
