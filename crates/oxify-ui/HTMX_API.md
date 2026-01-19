# OxiFY UI - HTMX API Documentation

This document describes the HTMX endpoints provided by the OxiFY UI server. These endpoints return HTML fragments designed to be swapped into the page using HTMX.

## Table of Contents

- [Workflow Endpoints](#workflow-endpoints)
- [Execution Endpoints](#execution-endpoints)
- [Node Editor Endpoints](#node-editor-endpoints)
- [Notification Endpoints](#notification-endpoints)
- [Server-Sent Events (SSE)](#server-sent-events-sse)

---

## Workflow Endpoints

### List Workflows (Partial)

Returns a paginated list of workflow cards.

**Endpoint:** `GET /htmx/workflows`

**Query Parameters:**
- `q` (optional) - Search query string to filter workflows
- `status` (optional) - Filter by workflow status (e.g., "active", "draft")
- `sort` (optional) - Sort order (e.g., "name", "updated_at")
- `page` (optional) - Page number (default: 1)

**Response:** HTML fragment containing workflow cards

**Example Usage:**
```html
<div hx-get="/htmx/workflows?page=1" hx-trigger="load">
  Loading workflows...
</div>
```

**Example Response:**
```html
<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
  <div class="workflow-card">
    <h3>Customer Support Chat</h3>
    <p>5 nodes • Last run: 2 hours ago</p>
  </div>
  <!-- More workflow cards... -->
</div>
```

---

### Search Workflows

Returns filtered workflow list based on search query (debounced endpoint).

**Endpoint:** `GET /htmx/workflows/search`

**Query Parameters:**
- `q` (optional) - Search query string
- `status` (optional) - Filter by status
- `sort` (optional) - Sort order
- `page` (optional) - Page number

**Response:** HTML fragment containing filtered workflow cards

**Example Usage:**
```html
<input
  type="search"
  name="q"
  hx-get="/htmx/workflows/search"
  hx-trigger="keyup changed delay:500ms"
  hx-target="#workflow-list"
  placeholder="Search workflows..."
/>
```

---

### Get Workflow Card

Returns a single workflow card by ID.

**Endpoint:** `GET /htmx/workflows/{id}`

**Path Parameters:**
- `id` - UUID of the workflow

**Response:** HTML fragment containing a single workflow card

**Example Usage:**
```html
<div hx-get="/htmx/workflows/123e4567-e89b-12d3-a456-426614174000" hx-trigger="revealed">
  Loading...
</div>
```

---

### Delete Workflow

Deletes a workflow and returns updated list or empty response.

**Endpoint:** `DELETE /htmx/workflows/{id}`

**Path Parameters:**
- `id` - UUID of the workflow to delete

**Response:** HTTP 200 with empty body on success

**Example Usage:**
```html
<button
  hx-delete="/htmx/workflows/123e4567-e89b-12d3-a456-426614174000"
  hx-confirm="Are you sure you want to delete this workflow?"
  hx-target="closest .workflow-card"
  hx-swap="outerHTML swap:1s"
>
  Delete
</button>
```

---

### Get Workflow Preview

Returns a visual preview of the workflow DAG.

**Endpoint:** `GET /htmx/workflows/{id}/preview`

**Path Parameters:**
- `id` - UUID of the workflow

**Response:** HTML fragment containing SVG workflow diagram

**Example Usage:**
```html
<div hx-get="/htmx/workflows/123e4567-e89b-12d3-a456-426614174000/preview" hx-trigger="load">
  Loading preview...
</div>
```

**Example Response:**
```html
<svg width="600" height="400" viewBox="0 0 600 400">
  <!-- DAG visualization -->
  <circle cx="100" cy="100" r="30" fill="#0ea5e9"/>
  <text x="100" y="105" text-anchor="middle">Start</text>
  <!-- More nodes and edges... -->
</svg>
```

---

## Execution Endpoints

### List Executions (Partial)

Returns a paginated list of execution records.

**Endpoint:** `GET /htmx/executions`

**Query Parameters:**
- `workflow_id` (optional) - Filter by workflow UUID
- `status` (optional) - Filter by execution status (e.g., "running", "completed", "failed")
- `page` (optional) - Page number (default: 1)
- `per_page` (optional) - Items per page (default: 20)

**Response:** HTML fragment containing execution list

**Example Usage:**
```html
<div hx-get="/htmx/executions?status=running" hx-trigger="load">
  Loading executions...
</div>
```

---

### Get Execution Rows (Infinite Scroll)

Returns additional execution rows for infinite scrolling.

**Endpoint:** `GET /htmx/executions/rows`

**Query Parameters:**
- `page` (required) - Page number
- `workflow_id` (optional) - Filter by workflow UUID
- `status` (optional) - Filter by execution status

**Response:** HTML fragment containing execution table rows

**Example Usage:**
```html
<tr hx-get="/htmx/executions/rows?page=2"
    hx-trigger="revealed"
    hx-swap="afterend">
  <td colspan="5">Loading more...</td>
</tr>
```

**Example Response:**
```html
<tr>
  <td>exec-abc123</td>
  <td>Customer Support Chat</td>
  <td><span class="badge badge-success">Completed</span></td>
  <td>2.3s</td>
  <td>5 minutes ago</td>
</tr>
<!-- More rows... -->
```

---

### Get Execution Status

Returns the current status of an execution with progress indicator.

**Endpoint:** `GET /htmx/executions/{id}/status`

**Path Parameters:**
- `id` - UUID of the execution

**Response:** HTML fragment containing status information

**Example Usage:**
```html
<div
  id="execution-status"
  hx-get="/htmx/executions/123e4567-e89b-12d3-a456-426614174000/status"
  hx-trigger="every 2s"
>
  Loading status...
</div>
```

**Example Response:**
```html
<div id="execution-status" class="status-widget">
  <div class="progress-bar">
    <div class="progress-fill" style="width: 65%"></div>
  </div>
  <p>Running - Node 3 of 5 (65%)</p>
  <span class="badge badge-blue">Running</span>
</div>
```

---

### Get Execution Logs

Returns execution logs (optionally filtered).

**Endpoint:** `GET /htmx/executions/{id}/logs`

**Path Parameters:**
- `id` - UUID of the execution

**Query Parameters:**
- `level` (optional) - Filter by log level (e.g., "error", "warn", "info")
- `limit` (optional) - Maximum number of log entries

**Response:** HTML fragment containing log entries

**Example Usage:**
```html
<div
  id="execution-logs"
  hx-get="/htmx/executions/123e4567-e89b-12d3-a456-426614174000/logs"
  hx-trigger="every 3s"
>
  Loading logs...
</div>
```

**Example Response:**
```html
<div class="log-container">
  <div class="log-entry log-info">
    <span class="timestamp">2026-01-09 10:30:45</span>
    <span class="level">INFO</span>
    <span class="message">Starting workflow execution</span>
  </div>
  <div class="log-entry log-error">
    <span class="timestamp">2026-01-09 10:30:47</span>
    <span class="level">ERROR</span>
    <span class="message">API call failed: Connection timeout</span>
  </div>
</div>
```

---

### Get Execution Graph

Returns the execution DAG with node-level status visualization.

**Endpoint:** `GET /htmx/executions/{id}/graph`

**Path Parameters:**
- `id` - UUID of the execution

**Response:** HTML fragment containing SVG graph with execution status

**Example Usage:**
```html
<div
  hx-get="/htmx/executions/123e4567-e89b-12d3-a456-426614174000/graph"
  hx-trigger="load"
>
  Loading execution graph...
</div>
```

**Example Response:**
```html
<svg width="800" height="600" viewBox="0 0 800 600">
  <!-- Nodes with execution status -->
  <circle cx="100" cy="100" r="30" fill="#10b981" class="node-completed"/>
  <circle cx="250" cy="100" r="30" fill="#3b82f6" class="node-running"/>
  <circle cx="400" cy="100" r="30" fill="#9ca3af" class="node-pending"/>
  <!-- Edges... -->
</svg>
```

---

## Node Editor Endpoints

### Get Node Configuration Form

Returns a configuration form for a specific node type.

**Endpoint:** `GET /htmx/nodes/form/{node_type}`

**Path Parameters:**
- `node_type` - Type of node (e.g., "llm", "retriever", "condition", "loop")

**Query Parameters:**
- `node_id` (optional) - UUID of existing node for editing
- `workflow_id` (optional) - UUID of parent workflow

**Response:** HTML fragment containing node configuration form

**Example Usage:**
```html
<div hx-get="/htmx/nodes/form/llm" hx-trigger="click">
  Add LLM Node
</div>
```

**Example Response:**
```html
<form hx-post="/api/v1/workflows/nodes" hx-target="#node-list" hx-swap="beforeend">
  <div class="form-group">
    <label for="model">Model</label>
    <select name="model" id="model">
      <option value="gpt-4">GPT-4</option>
      <option value="gpt-3.5-turbo">GPT-3.5 Turbo</option>
    </select>
  </div>
  <div class="form-group">
    <label for="temperature">Temperature</label>
    <input type="number" name="temperature" id="temperature"
           min="0" max="2" step="0.1" value="0.7"/>
  </div>
  <button type="submit">Add Node</button>
</form>
```

---

### Validate Node Configuration

Validates node configuration and returns validation results.

**Endpoint:** `POST /htmx/nodes/validate`

**Request Body:** Form data with node configuration

**Response:** HTML fragment with validation results (errors or success message)

**Example Usage:**
```html
<form hx-post="/htmx/nodes/validate" hx-target="#validation-result">
  <input type="text" name="node_name" required/>
  <input type="number" name="max_tokens" min="1" max="4096"/>
  <button type="submit">Validate</button>
</form>
<div id="validation-result"></div>
```

**Example Response (Success):**
```html
<div class="alert alert-success">
  ✓ Configuration is valid
</div>
```

**Example Response (Error):**
```html
<div class="alert alert-error">
  <ul>
    <li>max_tokens must be between 1 and 4096</li>
    <li>node_name is required</li>
  </ul>
</div>
```

---

## Notification Endpoints

### Show Toast Notification

Returns a toast notification to display to the user.

**Endpoint:** `GET /htmx/toast`

**Query Parameters:**
- `message` (required) - Notification message text
- `type` (optional) - Notification type: "success", "error", "warning", "info" (default: "info")
- `duration` (optional) - Display duration in milliseconds (default: 3000)

**Response:** HTML fragment containing toast notification

**Example Usage:**
```html
<div hx-get="/htmx/toast?message=Workflow saved!&type=success" hx-trigger="click">
  Save Workflow
</div>
```

**Example Response:**
```html
<div class="toast toast-success" role="alert" x-data="{ show: true }" x-show="show"
     x-init="setTimeout(() => show = false, 3000)">
  <div class="toast-icon">✓</div>
  <div class="toast-message">Workflow saved!</div>
  <button class="toast-close" @click="show = false">×</button>
</div>
```

---

## Server-Sent Events (SSE)

### Single Execution Stream

Opens an SSE connection for real-time updates of a single execution.

**Endpoint:** `GET /sse/executions/{id}`

**Path Parameters:**
- `id` - UUID of the execution to monitor

**Response:** Server-Sent Events stream

**Event Types:**
- `message` - Execution progress updates (HTML fragments with HTMX OOB swap)

**Example Usage:**
```html
<div
  hx-ext="sse"
  sse-connect="/sse/executions/123e4567-e89b-12d3-a456-426614174000"
  sse-swap="message"
  hx-target="#execution-status"
>
  <div id="execution-status">Connecting to execution stream...</div>
</div>
```

**Example Event:**
```
event: message
data: <div id="execution-status" hx-swap-oob="true">
data:   <div class="progress-bar">
data:     <div class="progress-fill" style="width: 75%"></div>
data:   </div>
data:   <p>Running - 75% complete</p>
data: </div>

```

---

### Multiplexed Execution Stream

Opens an SSE connection for monitoring multiple executions simultaneously.

**Endpoint:** `GET /sse/executions`

**Query Parameters:**
- `ids` (required) - Comma-separated list of execution UUIDs

**Response:** Server-Sent Events stream

**Event Types:**
- `execution_update` - Updates for any monitored execution (JSON payload)
- `message` - Keep-alive pings

**Example Usage (JavaScript):**
```javascript
// Using the SSEMultiplexer class
const multiplexer = window.sseMultiplexer;

// Subscribe to execution updates
multiplexer.subscribe('exec-uuid-1', (data) => {
  console.log('Execution 1 progress:', data.progress);
  // data.html contains the HTML fragment to swap
});

multiplexer.subscribe('exec-uuid-2', (data) => {
  console.log('Execution 2 progress:', data.progress);
});

// Unsubscribe when done
// multiplexer.unsubscribe('exec-uuid-1', callback);
```

**Example Event:**
```
event: execution_update
data: {
data:   "execution_id": "123e4567-e89b-12d3-a456-426614174000",
data:   "progress": 65,
data:   "status": "running",
data:   "current_node": "llm_node_2",
data:   "html": "<div id=\"execution-status-123e4567-e89b-12d3-a456-426614174000\" hx-swap-oob=\"true\">...</div>"
data: }

```

**Benefits:**
- Single connection for multiple executions
- Reduced server load
- Automatic reconnection with exponential backoff
- Client-side demultiplexing

---

## Response Formats

### Success Response
All successful HTMX endpoint responses return HTML fragments ready to be swapped into the DOM.

### Error Response
Error responses (4xx, 5xx) return HTML error fragments:

```html
<div class="alert alert-error" role="alert">
  <strong>Error:</strong> Failed to load workflow. Please try again.
</div>
```

---

## HTMX Headers

The API supports standard HTMX request headers:

- `HX-Request: true` - Indicates HTMX request
- `HX-Trigger` - ID of element that triggered the request
- `HX-Target` - ID of target element for swap
- `HX-Current-URL` - Current URL of the browser

And response headers:

- `HX-Trigger` - Trigger events on client
- `HX-Redirect` - Client-side redirect
- `HX-Refresh` - Refresh the page
- `HX-Push-Url` - Push new URL to browser history

---

## Rate Limiting

All endpoints implement reasonable rate limiting:
- Standard endpoints: 100 requests/minute per IP
- SSE endpoints: 10 concurrent connections per IP

---

## Caching

Responses include appropriate `Cache-Control` headers:
- Static partials: `max-age=300` (5 minutes)
- Dynamic content: `no-cache`
- Real-time updates: `no-store`

---

## Authentication

All HTMX endpoints require authentication via:
- Cookie-based session (preferred)
- Bearer token in `Authorization` header

Unauthenticated requests receive a 401 redirect to `/login`.

---

## Error Handling

The API returns appropriate HTTP status codes:
- `200 OK` - Success
- `400 Bad Request` - Invalid parameters
- `401 Unauthorized` - Authentication required
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server error

Error responses include user-friendly HTML error messages suitable for display.

---

## Best Practices

1. **Use debouncing for search** - Apply `delay:500ms` trigger for search inputs
2. **Implement infinite scroll** - Use `revealed` trigger for pagination
3. **Handle loading states** - Show loading indicators during requests
4. **Use appropriate swap strategies** - Choose `innerHTML`, `outerHTML`, or `beforeend` based on use case
5. **Monitor with SSE multiplexer** - Use multiplexed endpoint for monitoring multiple executions
6. **Implement error handling** - Show error toasts for failed requests
7. **Cache appropriately** - Respect cache headers for optimal performance

---

## Support

For issues or questions about the HTMX API, please open an issue on GitHub or refer to the [main documentation](./README.md).
