# oxify-api

**The Face** - REST API Server for OxiFY Workflow Orchestration

## Overview

`oxify-api` provides a production-ready HTTP API for managing and executing LLM workflows. Built on top of `oxify-server` (Axum-based HTTP runtime), it offers complete CRUD operations, real-time execution monitoring via Server-Sent Events, and OpenAPI documentation.

**Status**: ✅ Phase 1 Complete - Production Ready with SSE
**Part of**: OxiFY Enterprise Architecture (Codename: Absolute Zero)

## Features

- ✅ **Workflow CRUD**: Create, Read, Update, Delete workflows
- ✅ **Workflow Execution**: Async execution with real-time monitoring
- ✅ **Server-Sent Events (SSE)**: Stream execution updates in real-time
- ✅ **OpenAPI Documentation**: Auto-generated API docs with utoipa
- ✅ **Authentication Ready**: Integrated with oxify-authn (JWT, OAuth2)
- ✅ **Authorization Ready**: Integrated with oxify-authz (ReBAC)
- ⚠️ **Rate Limiting**: Dependency included, implementation pending
- ⚠️ **Swagger UI**: Commented out due to Axum 0.8 compatibility

## Architecture

```
┌─────────────────────────────────────────┐
│          oxify-api Server              │
│                                         │
│  ┌───────────────────────────────────┐ │
│  │   Axum HTTP Server                │ │
│  │   - CORS middleware               │ │
│  │   - Compression (gzip/brotli)     │ │
│  │   - Request tracing               │ │
│  └───────────────────────────────────┘ │
│                ↓                        │
│  ┌───────────────────────────────────┐ │
│  │   API Handlers                    │ │
│  │   - Workflow CRUD                 │ │
│  │   - Execution endpoints           │ │
│  │   - SSE streaming                 │ │
│  └───────────────────────────────────┘ │
│                ↓                        │
│  ┌───────────────────────────────────┐ │
│  │   Storage Layer                   │ │
│  │   - WorkflowStore (in-memory)     │ │
│  │   - ExecutionStore (in-memory)    │ │
│  └───────────────────────────────────┘ │
│                ↓                        │
│  ┌───────────────────────────────────┐ │
│  │   oxify-engine                    │ │
│  │   - DAG execution                 │ │
│  │   - Parallel node execution       │ │
│  └───────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

## API Endpoints

### Health Check

```http
GET /health

Response:
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### Workflow Management

#### Create Workflow

```http
POST /api/v1/workflows
Content-Type: application/json

{
  "workflow": {
    "metadata": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "My RAG Workflow",
      "description": "Retrieval-Augmented Generation workflow",
      "version": "1.0.0",
      "tags": ["rag", "production"]
    },
    "nodes": [...],
    "edges": [...]
  }
}

Response (201 Created):
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "message": "Workflow created successfully"
}
```

#### List Workflows

```http
GET /api/v1/workflows

Response:
{
  "workflows": [...],
  "total": 10
}
```

#### Get Workflow

```http
GET /api/v1/workflows/:id

Response:
{
  "workflow": {...}
}
```

#### Update Workflow

```http
PUT /api/v1/workflows/:id
Content-Type: application/json

{
  "workflow": {...}
}

Response:
{
  "message": "Workflow updated successfully"
}
```

#### Delete Workflow

```http
DELETE /api/v1/workflows/:id

Response:
{
  "message": "Workflow deleted successfully"
}
```

### Workflow Execution

#### Execute Workflow

```http
POST /api/v1/workflows/:id/execute
Content-Type: application/json

{
  "variables": {
    "user_query": "What is the weather in Tokyo?",
    "max_results": 5
  }
}

Response (202 Accepted):
{
  "execution_id": "660e8400-e29b-41d4-a716-446655440000",
  "message": "Workflow execution started"
}
```

#### Get Execution Status

```http
GET /api/v1/executions/:id

Response:
{
  "execution_id": "660e8400-e29b-41d4-a716-446655440000",
  "workflow_id": "550e8400-e29b-41d4-a716-446655440000",
  "state": "Completed",
  "variables": {...},
  "node_results": [...]
}
```

#### List All Executions

```http
GET /api/v1/executions

Response:
{
  "executions": [
    {
      "execution_id": "...",
      "workflow_id": "...",
      "state": "Completed",
      "started_at": null
    }
  ],
  "total": 5
}
```

#### List Workflow Executions

```http
GET /api/v1/workflows/:id/executions

Response:
{
  "executions": [...],
  "total": 3
}
```

#### Stream Execution Updates (SSE)

```http
GET /api/v1/executions/:id/stream
Accept: text/event-stream

Response (SSE stream):
data: {"execution_id":"...","workflow_id":"...","state":"Running","node_results_count":2}

data: {"execution_id":"...","workflow_id":"...","state":"Completed","node_results_count":5}
```

## Running the Server

### Development

```bash
cargo run --bin oxify-api
```

Server starts on `http://0.0.0.0:3000`

### Production

```bash
cargo build --release
./target/release/oxify-api
```

## OpenAPI Documentation

The API is fully documented with OpenAPI 3.0 schemas using `utoipa`.

Access documentation:
- **OpenAPI JSON**: `http://localhost:3000/api-docs/openapi.json` (when Swagger UI is enabled)
- **Swagger UI**: `http://localhost:3000/swagger-ui` (currently disabled due to Axum 0.8 compatibility)

## Storage

Current implementation uses in-memory storage (`HashMap` with `RwLock`):
- `WorkflowStore`: Stores workflow definitions
- `ExecutionStore`: Stores execution contexts

**Note**: In-memory storage is volatile. For production, implement persistent storage:
- PostgreSQL for workflows and executions
- Redis for caching and sessions

## Authentication & Authorization

The API is integrated with OxiFY's security layer:

### Authentication (`oxify-authn`)
- ✅ JWT authentication middleware available
- ✅ OAuth2/OIDC support
- ✅ API key authentication

### Authorization (`oxify-authz`)
- ✅ ReBAC (Relation-Based Access Control)
- ✅ Zanzibar-style authorization

**Note**: Middleware is available but not enforced in current endpoints. Add authentication to routes as needed.

## Rate Limiting

Rate limiting infrastructure is included (`tower_governor`) but not yet implemented.

Planned:
- Per-IP rate limiting (100 req/min)
- Per-user rate limiting (1000 req/min)
- Per-workflow execution limits

## Configuration

Environment variables (planned):
```bash
OXIFY_HOST=0.0.0.0
OXIFY_PORT=3000
OXIFY_DATABASE_URL=postgresql://localhost/oxify
OXIFY_REDIS_URL=redis://localhost:6379
OXIFY_LOG_LEVEL=info
```

## Error Responses

All errors follow a consistent format:

```json
{
  "error": "ValidationError",
  "message": "Workflow must have at least one Start node"
}
```

HTTP Status Codes:
- `200 OK`: Success
- `201 Created`: Resource created
- `202 Accepted`: Async operation started
- `400 Bad Request`: Validation error
- `404 Not Found`: Resource not found
- `500 Internal Server Error`: Unexpected error

## Examples

### Create and Execute a Simple Workflow

```bash
# Create workflow
curl -X POST http://localhost:3000/api/v1/workflows \
  -H "Content-Type: application/json" \
  -d @workflow.json

# Execute workflow
curl -X POST http://localhost:3000/api/v1/workflows/550e8400-e29b-41d4-a716-446655440000/execute \
  -H "Content-Type: application/json" \
  -d '{"variables": {"input": "Hello"}}'

# Stream execution (SSE)
curl -N http://localhost:3000/api/v1/executions/660e8400-e29b-41d4-a716-446655440000/stream
```

## Tech Stack

- **Axum 0.8**: Web framework
- **Tower**: Middleware layer
- **Tower-HTTP**: CORS, compression, tracing
- **Tokio**: Async runtime
- **Utoipa**: OpenAPI documentation
- **Serde**: JSON serialization
- **Futures/Tokio-Stream**: SSE implementation

## Future Enhancements

See `TODO.md` for detailed roadmap.

Highlights:
- [ ] Rate limiting implementation
- [ ] Swagger UI (waiting for Axum 0.8 support)
- [ ] Persistent storage (PostgreSQL)
- [ ] WebSocket support
- [ ] GraphQL API
- [ ] Batch operations
- [ ] Workflow import/export (JSON/YAML)
- [ ] Execution cancellation
- [ ] Audit logging

## See Also

- `oxify-model`: Workflow data models
- `oxify-engine`: DAG execution engine
- `oxify-server`: HTTP server runtime
- `oxify-authn`: Authentication layer
- `oxify-authz`: Authorization layer

## License

Apache-2.0
