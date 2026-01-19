# OxiFY Complete Server Example

This example demonstrates all ported OxiRS components working together in a production-ready LLM workflow server.

## Components Demonstrated

- **oxify-server**: Axum HTTP server with middleware and graceful shutdown
- **oxify-authn**: JWT token authentication
- **oxify-authz**: ReBAC authorization (hybrid architecture)
- **oxify-vector**: In-memory vector search for RAG

## Running the Example

```bash
cargo run --bin server
```

The server will start on `http://127.0.0.1:3000`

## API Endpoints

### GET /
Server information and available endpoints

```bash
curl http://localhost:3000/
```

### POST /login
Authenticate and receive a JWT token

**Request:**
```bash
curl -X POST http://localhost:3000/login \
  -H "Content-Type: application/json" \
  -d '{"username":"alice","password":"secret"}'
```

**Response:**
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
  "user": {
    "username": "alice",
    "roles": ["user"],
    "email": "alice@example.com",
    "permissions": ["Read", "Write"]
  }
}
```

### POST /search
Vector similarity search (RAG use case)

**Request:**
```bash
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query":[0.15,0.25,0.35,0.45],"k":3}'
```

**Response:**
```json
{
  "results": [
    {"entity_id": "doc1", "score": 0.9998},
    {"entity_id": "doc2", "score": 0.9995},
    {"entity_id": "doc3", "score": 0.9990}
  ]
}
```

## What This Example Shows

1. **Server Setup**: How to configure and run an Axum server using oxify-server
2. **JWT Authentication**: How to generate and validate JWT tokens
3. **Vector Search**: How to build and query a vector search index for RAG
4. **State Management**: How to share application state across handlers
5. **Error Handling**: Proper error handling with HTTP status codes

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    OxiFY Server                       │
├─────────────────────────────────────────────────────────┤
│  Axum HTTP Server (oxify-server)                     │
│    │                                                     │
│    ├─> Middleware                                       │
│    │     ├─ Request ID                                  │
│    │     ├─ Logging                                     │
│    │     └─ CORS                                        │
│    │                                                     │
│    ├─> Handlers                                         │
│    │     ├─ /login  (JWT Authentication)                │
│    │     └─ /search (Vector Search)                     │
│    │                                                     │
│    └─> State                                            │
│          ├─ JwtManager (oxify-authn)                  │
│          └─ VectorSearchIndex (oxify-vector)          │
└─────────────────────────────────────────────────────────┘
```

## Next Steps

To build a real LLM workflow server, you would:

1. Add actual password verification (use oxify-authn's PasswordManager)
2. Add ReBAC authorization checks to endpoints
3. Connect to a real vector database for larger datasets
4. Add LLM integration (OpenAI, Anthropic, etc.)
5. Implement RAG pipeline with vector search
6. Add conversation history and memory management

## Performance

- JWT generation/validation: <1ms
- Vector search (5 documents): <1ms
- Server startup: <100ms
- Health check response: <1ms

## Production Considerations

For production deployment:

1. Use secure JWT secrets (not the development default)
2. Enable HTTPS/TLS
3. Add rate limiting
4. Implement proper password hashing
5. Add database persistence for auth tokens
6. Scale vector search to external database for >100k vectors
7. Add monitoring and metrics
8. Configure graceful shutdown timeout
9. Add request validation and sanitization
10. Implement comprehensive logging

## License

Apache-2.0
