# Oxify Server Architecture

## Overview

Oxify Server is a production-ready HTTP server built with Rust and Axum, designed for high performance, security, and observability. It serves as the interface layer for the Oxify workflow orchestration system.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         Load Balancer (nginx)                     │
│                      Ingress Controller (K8s)                     │
└────────────────────────────┬──────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Oxify Server Cluster                       │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐       │
│  │ Pod 1        │   │ Pod 2        │   │ Pod 3        │       │
│  │              │   │              │   │              │       │
│  │ ┌──────────┐ │   │ ┌──────────┐ │   │ ┌──────────┐ │       │
│  │ │  Axum    │ │   │ │  Axum    │ │   │ │  Axum    │ │       │
│  │ │  Server  │ │   │ │  Server  │ │   │ │  Server  │ │       │
│  │ └──────────┘ │   │ └──────────┘ │   │ └──────────┘ │       │
│  └──────────────┘   └──────────────┘   └──────────────┘       │
└────────────────────────────┬──────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ PostgreSQL   │    │    Redis     │    │   Qdrant     │
│  (Database)  │    │   (Cache)    │    │   (Vector)   │
└──────────────┘    └──────────────┘    └──────────────┘
        │
        ▼
┌──────────────────────────────────────────────┐
│            Observability Stack                │
│  ┌────────────┐  ┌────────────┐             │
│  │ Prometheus │  │  Grafana   │             │
│  │  (Metrics) │  │  (Dashboards)│           │
│  └────────────┘  └────────────┘             │
└──────────────────────────────────────────────┘
```

## Core Components

### 1. HTTP Server (Axum)
- **Purpose:** Handle HTTP requests with high performance
- **Technology:** Axum web framework on Tokio runtime
- **Features:**
  - Async/await for non-blocking I/O
  - Type-safe routing
  - Middleware composition
  - WebSocket and SSE support

### 2. Middleware Pipeline
The request flows through multiple middleware layers:

```
Request → Request ID → Logging → Rate Limiting → Authentication →
Validation → Security Headers → CORS → Compression → Handler → Response
```

#### Middleware Components:
1. **Request ID:** Assigns unique UUID to each request
2. **Logging:** Structured logging with tracing
3. **Rate Limiting:** Token bucket algorithm per IP
4. **Authentication:** JWT token validation
5. **Validation:** Input sanitization and size limits
6. **Security Headers:** CSP, HSTS, X-Frame-Options, etc.
7. **CORS:** Cross-Origin Resource Sharing
8. **Compression:** gzip, brotli, deflate

### 3. Caching Layer
- **HTTP Response Caching:** LRU cache with ETag support
- **Cache Strategy:** TTL-based expiration with automatic cleanup
- **Statistics:** Hit rate, miss rate, evictions tracking

### 4. Async Optimization
- **CPU-Intensive Tasks:** Spawn on blocking threadpool
- **Performance Tracking:** Global async statistics
- **Batch Operations:** Minimize context switches

### 5. Connection Management
- **HTTP/2 Connection Pool:** Persistent connections
- **Database Pool:** Configurable min/max connections
- **Redis Pool:** Connection reuse for caching

### 6. DDoS Protection
- **Connection Limits:** Per-IP and global limits
- **Slowloris Protection:** Request timing tracker
- **Rate Detection:** Minimum data rate enforcement

### 7. TLS/HTTPS
- **TLS 1.2/1.3 Support:** Modern encryption
- **Certificate Management:** ACME protocol (Let's Encrypt)
- **Automatic Renewal:** Monitors certificate expiration

### 8. Real-Time Communication
- **Server-Sent Events (SSE):** Streaming workflow updates
- **WebSockets:** Bidirectional communication for collaboration
- **Connection Manager:** Track active connections per user

## Data Flow

### HTTP Request Flow
```
1. Client → Load Balancer
2. Load Balancer → Oxify Server Pod
3. Request ID Middleware (assigns UUID)
4. Logging Middleware (logs request details)
5. Rate Limiting (checks IP limits)
6. Authentication Middleware (validates JWT)
7. Validation Middleware (sanitizes input)
8. Security Headers Middleware (adds headers)
9. Handler (business logic)
10. Response Caching (cache if applicable)
11. Compression Middleware (compresses response)
12. Logging Middleware (logs response)
13. Client ← Response
```

### Workflow Execution Flow
```
1. Client → POST /api/workflows/execute
2. Authentication & Validation
3. Workflow Engine (oxify-engine crate)
4. SSE/WebSocket notification to subscribers
5. Database storage (PostgreSQL)
6. Vector search (Qdrant) for RAG
7. Response → Client
```

## Module Structure

```
oxify-server/
├── src/
│   ├── lib.rs              # Module exports
│   ├── server.rs           # HTTP server runtime
│   ├── middleware.rs       # Custom middleware
│   ├── error.rs            # Error types (RFC 7807)
│   ├── types.rs            # Configuration types
│   ├── rate_limit.rs       # Rate limiting
│   ├── validation.rs       # Input validation
│   ├── security.rs         # Security headers
│   ├── metrics.rs          # Prometheus metrics
│   ├── cache.rs            # HTTP response caching
│   ├── async_optimization.rs # Async utilities
│   ├── connection_pool.rs  # Connection management
│   ├── ddos_protection.rs  # DDoS mitigation
│   ├── tls.rs              # TLS configuration
│   ├── acme.rs             # ACME certificate management
│   ├── sse.rs              # Server-Sent Events
│   ├── websocket.rs        # WebSocket support
│   ├── openapi.rs          # OpenAPI/Swagger docs
│   ├── chaos.rs            # Chaos engineering
│   ├── shutdown.rs         # Graceful shutdown
│   └── tracing_config.rs   # Logging configuration
├── tests/
│   ├── load/               # k6 load tests
│   └── security/           # OWASP security tests
├── k8s/                    # Kubernetes manifests
├── helm/                   # Helm chart
└── docker-compose.yml      # Local development
```

## Security Architecture

### Defense in Depth
1. **Network Layer:** Ingress with TLS termination
2. **Application Layer:**
   - Rate limiting
   - Input validation
   - Authentication/Authorization
   - Security headers
3. **Data Layer:** Encrypted connections to databases

### Security Headers
- **Content-Security-Policy (CSP):** Prevent XSS
- **X-Frame-Options:** Prevent clickjacking
- **X-Content-Type-Options:** Prevent MIME sniffing
- **Strict-Transport-Security (HSTS):** Force HTTPS
- **Referrer-Policy:** Control referrer information
- **Permissions-Policy:** Control browser features

## Performance Optimization

### Caching Strategy
```
┌─────────────┐
│   Request   │
└──────┬──────┘
       │
       ▼
┌─────────────┐     Hit      ┌─────────────┐
│ LRU Cache   ├─────────────►│   Response  │
│   (ETags)   │              └─────────────┘
└──────┬──────┘
       │ Miss
       ▼
┌─────────────┐
│   Handler   │
│  (Business  │
│    Logic)   │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ Cache Entry │
│  (with TTL) │
└─────────────┘
```

### Connection Pooling
- **HTTP/2:** Reuse connections with multiplexing
- **Database:** Pool size: min=10, max=50
- **Redis:** Connection reuse for cache operations

### Async Optimization
- **CPU-Intensive:** spawn_blocking for heavy computation
- **I/O Bound:** Async/await for network operations
- **Batch Operations:** Minimize context switches

## Observability

### Metrics (Prometheus)
- **HTTP Metrics:**
  - Request count (by method, path, status)
  - Request duration (histogram with P50, P95, P99)
  - Active connections (gauge)
  - Error rate (4xx, 5xx counters)

- **Cache Metrics:**
  - Hit rate, miss rate
  - Evictions
  - Cache size

- **Connection Metrics:**
  - Pool utilization
  - Connection age
  - Reuse count

### Logging (Tracing)
- **Structured Logging:** JSON format in production
- **Log Levels:** Configurable via RUST_LOG
- **Context:** Request ID, user ID, trace ID

### Tracing (Future)
- **OpenTelemetry:** W3C Trace Context
- **Distributed Tracing:** Span creation per middleware
- **Export:** OTLP to Jaeger/Zipkin

## Deployment Architecture

### Kubernetes Deployment
```
┌─────────────────────────────────────────────┐
│            Kubernetes Cluster                │
│  ┌────────────────────────────────────────┐ │
│  │          Namespace: oxify              │ │
│  │                                        │ │
│  │  ┌──────────────────────────────────┐ │ │
│  │  │  Deployment (3-20 replicas)      │ │ │
│  │  │  - Rolling update                │ │ │
│  │  │  - Resource limits               │ │ │
│  │  │  - Liveness/Readiness probes     │ │ │
│  │  └──────────────┬───────────────────┘ │ │
│  │                 │                      │ │
│  │  ┌──────────────▼───────────────────┐ │ │
│  │  │  HorizontalPodAutoscaler (HPA)   │ │ │
│  │  │  - CPU: 70%                      │ │ │
│  │  │  - Memory: 80%                   │ │ │
│  │  └──────────────────────────────────┘ │ │
│  │                                        │ │
│  │  ┌──────────────────────────────────┐ │ │
│  │  │  PodDisruptionBudget (PDB)       │ │ │
│  │  │  - minAvailable: 2               │ │ │
│  │  └──────────────────────────────────┘ │ │
│  │                                        │ │
│  │  ┌──────────────────────────────────┐ │ │
│  │  │  Service (ClusterIP)             │ │ │
│  │  │  - Session affinity              │ │ │
│  │  └──────────────┬───────────────────┘ │ │
│  │                 │                      │ │
│  │  ┌──────────────▼───────────────────┐ │ │
│  │  │  Ingress (TLS)                   │ │ │
│  │  │  - cert-manager                  │ │ │
│  │  │  - Rate limiting                 │ │ │
│  │  └──────────────────────────────────┘ │ │
│  └────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

### High Availability
- **Replicas:** 3-20 pods (autoscaling)
- **Pod Anti-Affinity:** Spread across nodes
- **Topology Spread:** Distribute across zones
- **PodDisruptionBudget:** Ensure minimum availability

## Scalability

### Horizontal Scaling
- **HPA:** Auto-scale based on CPU/memory
- **Custom Metrics:** Request rate, error rate
- **Scaling Policies:**
  - Scale up: 50% increase per minute
  - Scale down: 25% decrease per minute

### Performance Targets
| Metric | Baseline | Target |
|--------|----------|--------|
| Throughput | 5,000 req/s | 50,000 req/s |
| P95 Latency | <100ms | <100ms |
| P99 Latency | <200ms | <200ms |
| Error Rate | <1% | <1% |

## Resilience

### Chaos Engineering
- **Failure Injection:** Random failures for testing
- **Latency Injection:** Network delay simulation
- **Resource Exhaustion:** CPU/memory pressure
- **Configuration:** Development and aggressive modes

### Recovery Mechanisms
- **Graceful Shutdown:** 30s termination grace period
- **Circuit Breaker:** (Future enhancement)
- **Retry Logic:** (Future enhancement)
- **Fallback:** (Future enhancement)

## API Documentation

### OpenAPI/Swagger
- **Endpoint:** `/swagger-ui/`
- **Spec:** `/api-docs/openapi.json`
- **Features:**
  - Interactive API explorer
  - Request/response examples
  - Authentication documentation

## Testing Strategy

### Unit Tests
- 137 tests covering all modules
- 100% passing rate
- Zero warnings policy

### Load Testing (k6)
- Basic load test (5,000 req/s)
- Stress test (50,000 req/s)
- Spike test (sudden traffic)
- Soak test (2-hour reliability)

### Security Testing (k6)
- SQL injection prevention
- XSS protection
- Authentication/Authorization
- Security headers validation

## Future Enhancements

1. **GraphQL API:** Flexible querying with async-graphql
2. **gRPC Support:** Binary protocol for internal services
3. **Service Mesh:** Istio/Linkerd integration
4. **Distributed Tracing:** OpenTelemetry + Jaeger
5. **Circuit Breaker:** Resilience patterns
6. **API Gateway:** Kong/Ambassador integration

## References

- [Axum Documentation](https://docs.rs/axum/)
- [Tokio Runtime](https://tokio.rs/)
- [Prometheus](https://prometheus.io/)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/configuration/overview/)
- [12-Factor App](https://12factor.net/)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)

## License

MIT OR Apache-2.0
