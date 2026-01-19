# Load Testing for Authorization Engine

This directory contains k6 load testing scripts for the authorization engine.

## Prerequisites

Install k6:
```bash
# macOS
brew install k6

# Linux (Debian/Ubuntu)
sudo gpg -k
sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update
sudo apt-get install k6

# Or download binary
# https://k6.io/docs/get-started/installation/
```

## Running Load Tests

### 1. Start the Authorization Server

First, ensure your authorization server is running on `http://localhost:8080` (or configure `BASE_URL` environment variable).

### 2. Run Smoke Test (Validation)

Quick test with minimal load to verify the setup:
```bash
k6 run --vus 1 --duration 10s loadtest/authz_load_test.js
```

### 3. Run Standard Load Test

Default configuration with realistic traffic patterns:
```bash
k6 run loadtest/authz_load_test.js
```

This runs a 5-minute test with:
- Ramp up to 100 users
- Spike to 200 users
- Mixed operations (checks, writes, batch checks, expand queries)

### 4. Run Stress Test

High-load scenario to find breaking points:
```bash
k6 run --vus 500 --duration 300s loadtest/authz_load_test.js
```

### 5. Custom Configuration

Override default settings:
```bash
# Custom VUs and duration
k6 run --vus 100 --duration 60s loadtest/authz_load_test.js

# Custom base URL
BASE_URL=http://production.example.com:8080 k6 run loadtest/authz_load_test.js

# Save detailed results
k6 run --out json=results.json loadtest/authz_load_test.js
```

## Performance Targets

The load test validates these performance thresholds:

| Metric | p95 Target | p99 Target |
|--------|-----------|-----------|
| Authorization checks | < 100ms | < 200ms |
| Batch checks | < 500ms | < 1000ms |
| Tuple writes | < 50ms | < 100ms |
| Success rate | > 99% | > 99% |

## Test Scenarios

The load test simulates realistic authorization workloads:

1. **Authorization Checks (80%)**: Verify if a user has permission
2. **Tuple Writes (10%)**: Add new permission relationships
3. **Batch Checks (5%)**: Check multiple permissions in one request
4. **Expand Queries (2%)**: Get all users with a specific permission

## Interpreting Results

After running the test, k6 provides a summary:

```
Authorization Engine Load Test Summary
═══════════════════════════════════════════════

Authorization Checks:
  p50: 12.45ms
  p95: 89.32ms
  p99: 156.78ms

Tuple Writes:
  p50: 8.23ms
  p95: 42.11ms
  p99: 87.54ms

Total Checks: 45,234
Total Writes: 4,523
```

**Key Metrics:**
- **p50 (median)**: 50% of requests faster than this
- **p95**: 95% of requests faster than this (important SLA metric)
- **p99**: 99% of requests faster than this (tail latency)

**Red Flags:**
- ❌ p95 > 100ms for checks → Cache not effective
- ❌ p99 > 200ms for checks → Database queries slow
- ❌ Success rate < 99% → Server errors or timeouts

## Advanced Usage

### Cloud Execution

Run load tests from k6 Cloud:
```bash
k6 cloud loadtest/authz_load_test.js
```

### Distributed Testing

Run from multiple geographic locations:
```bash
k6 run --out cloud loadtest/authz_load_test.js
```

### Integration with CI/CD

GitHub Actions example:
```yaml
- name: Run Load Test
  run: |
    k6 run --quiet loadtest/authz_load_test.js
    if [ $? -ne 0 ]; then
      echo "Load test failed thresholds"
      exit 1
    fi
```

## Customizing Tests

Edit `authz_load_test.js` to:
- Adjust stages (ramp-up/down patterns)
- Modify thresholds (performance targets)
- Change operation mix (read/write ratio)
- Add custom scenarios (multi-tenant, edge cases)

## Troubleshooting

**Connection refused:**
- Ensure server is running on correct port
- Check `BASE_URL` environment variable

**Timeouts:**
- Increase k6 timeout: `--http-debug=full`
- Check server logs for errors

**Threshold failures:**
- Review server performance (CPU, memory, DB connections)
- Check Leopard index is loaded
- Verify Redis cache is enabled

## Further Reading

- [k6 Documentation](https://k6.io/docs/)
- [Performance Testing Best Practices](https://k6.io/docs/testing-guides/test-types/)
- [Authorization Engine Architecture](../README.md)
