# oxify-ui

Web UI for OxiFY - A Rust-native server-side rendered web interface using Axum + Askama + HTMX.

## Architecture

This crate implements a modern web UI with the following stack:

- **Axum** - High-performance async web framework
- **Askama** - Compile-time checked Jinja2-style templates
- **HTMX** - Hypermedia-driven interactions without heavy JavaScript
- **Alpine.js** - Minimal client-side interactivity (dropdowns, modals)
- **Tailwind CSS** - Utility-first CSS framework (via CDN)
- **Server-Sent Events (SSE)** - Real-time execution monitoring

## Features

- Dashboard with workflow statistics
- Workflow management (list, create, edit, delete)
- Visual DAG preview for workflows
- Real-time execution monitoring via SSE
- Dark mode support with system preference detection
- Responsive design for mobile and desktop
- HTMX-powered partial updates without full page reloads

## Project Structure

```
oxify-ui/
├── src/
│   ├── lib.rs           # Library entry point
│   ├── main.rs          # Server binary
│   ├── error.rs         # Error types
│   ├── state.rs         # Application state
│   ├── routes.rs        # Route definitions
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── pages.rs     # Full page handlers
│   │   ├── htmx.rs      # HTMX partial handlers
│   │   └── sse.rs       # SSE stream handlers
│   └── templates/
│       ├── mod.rs       # Template structs
│       └── partials.rs  # Partial template structs
├── templates/
│   ├── layouts/
│   │   ├── base.html         # Base layout
│   │   └── _sidebar_content.html
│   ├── pages/
│   │   ├── dashboard.html
│   │   ├── workflow_list.html
│   │   ├── workflow_detail.html
│   │   ├── workflow_edit.html
│   │   ├── workflow_new.html
│   │   ├── execution_list.html
│   │   ├── execution_detail.html
│   │   └── settings.html
│   └── partials/
│       ├── workflow_list.html
│       ├── workflow_card.html
│       ├── workflow_preview.html
│       ├── execution_status.html
│       ├── execution_logs.html
│       ├── node_form.html
│       └── toast.html
└── static/              # Static assets (if any)
```

## Running

```bash
# Development mode
cargo run -p oxify-ui

# With hot reload (requires feature flag)
cargo run -p oxify-ui --features hot-reload
```

The server starts at `http://127.0.0.1:3001` by default.

## Configuration

Environment variables:

- `OXIFY_UI_HOST` - Host to bind to (default: `127.0.0.1`)
- `OXIFY_UI_PORT` - Port to listen on (default: `3001`)
- `OXIFY_API_URL` - Backend API URL (default: `http://127.0.0.1:3000`)

## Dependencies

Key dependencies from workspace:

- `axum` - Web framework
- `askama` / `askama_axum` - Template engine
- `tower-http` - HTTP middleware (compression, static files, CORS)
- `tokio` - Async runtime
- `futures` - Async utilities for SSE streams
