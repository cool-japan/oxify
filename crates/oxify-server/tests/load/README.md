# Load Testing for Oxify Server

This directory contains k6 load testing scripts for the Oxify Server.

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

## Test Types

### 1. Basic Load Test (`basic-load-test.js`)
Tests baseline performance at approximately 5,000 req/sec.

**Duration:** ~10 minutes
**Target:** 1,000 VUs (≈5,000 req/sec)
**Purpose:** Establish baseline performance metrics

```bash
k6 run tests/load/basic-load-test.js
```

**Expected Results:**
- P95 latency: < 100ms
- P99 latency: < 200ms
- Error rate: < 1%

### 2. Stress Test (`stress-test.js`)
Tests the system at and beyond normal capacity (up to 50,000 req/sec).

**Duration:** ~15 minutes
**Target:** Up to 10,000 VUs (≈50,000 req/sec)
**Purpose:** Find the breaking point and recovery behavior

```bash
k6 run tests/load/stress-test.js
```

**Expected Results:**
- P95 latency: < 500ms
- P99 latency: < 1000ms
- Error rate: < 5%

### 3. Spike Test (`spike-test.js`)
Tests the system's response to sudden traffic spikes.

**Duration:** ~11 minutes
**Target:** Sudden spikes to 10,000-15,000 VUs
**Purpose:** Validate autoscaling and recovery

```bash
k6 run tests/load/spike-test.js
```

**Expected Results:**
- P95 latency: < 1000ms during spike
- P99 latency: < 2000ms during spike
- Error rate: < 10%
- System should recover after spike

### 4. Soak Test (`soak-test.js`)
Tests system stability over extended period (2 hours).

**Duration:** ~2 hours 10 minutes
**Target:** 1,000 VUs (≈5,000 req/sec)
**Purpose:** Detect memory leaks, resource exhaustion

```bash
k6 run tests/load/soak-test.js
```

**Expected Results:**
- P95 latency: < 100ms (stable over time)
- P99 latency: < 200ms (stable over time)
- Error rate: < 1%
- No degradation over time

## Custom Configuration

All tests support custom target URL:

```bash
k6 run -e TARGET_URL=https://api.oxify.example.com tests/load/basic-load-test.js
```

## Performance Targets

| Metric | Baseline | Target |
|--------|----------|--------|
| Throughput | 5,000 req/sec | 50,000 req/sec |
| P95 Latency | < 100ms | < 100ms |
| P99 Latency | < 200ms | < 200ms |
| Error Rate | < 1% | < 1% |

## Running Tests Against Production

**WARNING:** Only run load tests against production with proper authorization and monitoring.

1. Use a separate testing endpoint or environment
2. Monitor system metrics (CPU, memory, network)
3. Have rollback plan ready
4. Start with smaller load and gradually increase

```bash
# Example: Production test with gradual ramp
k6 run -e TARGET_URL=https://api.oxify.example.com \
  --vus 100 \
  --duration 5m \
  tests/load/basic-load-test.js
```

## Monitoring During Tests

Monitor these metrics:
- **Prometheus:** http://localhost:9090
- **Grafana:** http://localhost:3000
- **System Metrics:**
  - CPU utilization
  - Memory usage
  - Network I/O
  - Request latency (P50, P95, P99)
  - Error rate (4xx, 5xx)
  - Active connections

## Analyzing Results

k6 outputs results in JSON format:
```bash
k6 run --out json=results.json tests/load/basic-load-test.js
```

View summary:
```bash
cat summary.json | jq '.metrics'
```

## Continuous Load Testing

Integrate with CI/CD:

```yaml
# Example: GitHub Actions
- name: Load Test
  run: |
    k6 run --quiet tests/load/basic-load-test.js
    if [ $? -ne 0 ]; then
      echo "Load test failed"
      exit 1
    fi
```

## Troubleshooting

### High Error Rate
- Check server logs
- Verify resource limits (CPU, memory)
- Check database connection pool
- Review rate limiting configuration

### High Latency
- Check for slow database queries
- Review caching effectiveness
- Monitor network latency
- Check for CPU throttling

### Memory Leaks
- Run soak test
- Monitor memory usage over time
- Check for connection leaks
- Review cache eviction policies

## References

- [k6 Documentation](https://k6.io/docs/)
- [Load Testing Best Practices](https://k6.io/docs/test-types/load-testing/)
- [Oxify Server Metrics](http://localhost:8080/metrics)
