# oxify-ui TODO

## Completed

- [x] Basic crate structure with Axum + Askama + HTMX
- [x] Base layout with Tailwind CSS and dark mode
- [x] Dashboard page with statistics
- [x] Workflow list page with HTMX pagination
- [x] Workflow detail page with DAG preview
- [x] Workflow edit/new pages
- [x] Execution list and detail pages
- [x] SSE-based real-time execution monitoring
- [x] Toast notifications
- [x] Responsive sidebar navigation
- [x] API client module for backend integration (with mock fallback)
- [x] Interactive DAG editor with SVG-based visual builder
- [x] Loading states for async operations (skeleton loaders, HTMX indicators)
- [x] Error handling with styled error pages
- [x] Drag-and-drop node creation
- [x] Edge creation by connecting ports
- [x] Undo/redo support
- [x] Keyboard shortcuts for common actions (copy/paste/duplicate, arrow keys, zoom, etc.)
- [x] Node configuration panels with two-way data binding
- [x] Workflow validation before save (cycle detection, connectivity check)
- [x] JSON API routes for workflow CRUD operations
- [x] Live log streaming via SSE
- [x] Execution history with filtering
- [x] Import/export workflow definitions (JSON)
- [x] Keyboard shortcuts help modal
- [x] Node-level execution status visualization on DAG
- [x] User preferences persistence (localStorage)
- [x] Workflow templates/presets (6 templates: Blank, Simple Chat, RAG, Agent, Parallel, Approval)
- [x] Search across workflows and executions
- [x] Export execution results (CSV/JSON)
- [x] Add authentication integration with oxify-authn (JWT middleware, login/logout handlers)
- [x] Enable live API mode (toggle mock data off via config - use OXIFY_UI_USE_MOCK env var)
- [x] YAML import/export support for workflows
- [x] Unit tests for handlers (10 tests implemented)
- [x] Response caching with Cache-Control headers for API endpoints and static assets
- [x] SSE reconnection logic with exponential backoff and automatic retry
- [x] Execution comparison view (compare up to 4 executions side-by-side)
- [x] Infinite scroll for execution and workflow lists (HTMX revealed trigger)

## In Progress

_None_

## Planned

### API Integration

_All authentication features completed_

### Workflow Editor

_YAML import/export completed_

### Execution Monitoring

_All features completed_

### User Experience

_Infinite scroll completed_

### Performance

- [x] Add response caching where appropriate (Cache-Control middleware implemented)
- [x] Implement infinite scroll for large lists (execution list with automatic pagination)
- [x] Optimize SSE connections (reconnection with exponential backoff, automatic retry)
- [x] Add service worker for offline support (caches static assets, HTML pages, and images with cache-first strategy)
- [x] SSE batching for multiple concurrent streams (multiplexed SSE endpoint with client-side demultiplexer)

### Testing

- [x] Unit tests for handlers (10 tests implemented - auth, cache, htmx, json_api)
- [x] Integration tests for routes (18 tests implemented - UI routes, HTMX partials, JSON API endpoints)
- [ ] E2E tests with headless browser
- [ ] Visual regression tests

### Documentation

- [x] API documentation for HTMX endpoints (comprehensive HTMX_API.md with all endpoints documented)
- [x] User guide for workflow creation (detailed WORKFLOW_GUIDE.md with examples and best practices)
