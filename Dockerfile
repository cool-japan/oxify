# =============================================================================
# OxiFY - Multi-stage Docker build
# Produces a minimal production image with oxify-ui and oxify-cli binaries.
# =============================================================================

# ---- Build arguments ---------------------------------------------------------
ARG OXIFY_LOG_LEVEL=info

# =============================================================================
# Stage 1: builder
# =============================================================================
FROM rust:1.87-slim AS builder

ARG OXIFY_LOG_LEVEL

# Install system dependencies required for linking
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy dependency manifests first for better layer caching.
# Cargo needs Cargo.toml + Cargo.lock at the workspace root, plus every crate
# Cargo.toml so the dependency graph can be resolved without the source files.
COPY Cargo.toml Cargo.lock ./

# Copy all crate manifests (but not source) to cache dependency compilation.
COPY crates/oxify/Cargo.toml                    crates/oxify/Cargo.toml
COPY crates/oxify-api/Cargo.toml               crates/oxify-api/Cargo.toml
COPY crates/oxify-authn/Cargo.toml             crates/oxify-authn/Cargo.toml
COPY crates/oxify-authz/Cargo.toml             crates/oxify-authz/Cargo.toml
COPY crates/oxify-cli/Cargo.toml               crates/oxify-cli/Cargo.toml
COPY crates/oxify-connect-comm/Cargo.toml      crates/oxify-connect-comm/Cargo.toml
COPY crates/oxify-connect-data/Cargo.toml      crates/oxify-connect-data/Cargo.toml
COPY crates/oxify-connect-db/Cargo.toml        crates/oxify-connect-db/Cargo.toml
COPY crates/oxify-connect-graphql/Cargo.toml   crates/oxify-connect-graphql/Cargo.toml
COPY crates/oxify-connect-llm/Cargo.toml       crates/oxify-connect-llm/Cargo.toml
COPY crates/oxify-connect-storage/Cargo.toml   crates/oxify-connect-storage/Cargo.toml
COPY crates/oxify-connect-vector/Cargo.toml    crates/oxify-connect-vector/Cargo.toml
COPY crates/oxify-connect-vision/Cargo.toml    crates/oxify-connect-vision/Cargo.toml
COPY crates/oxify-engine/Cargo.toml            crates/oxify-engine/Cargo.toml
COPY crates/oxify-mcp/Cargo.toml               crates/oxify-mcp/Cargo.toml
COPY crates/oxify-model/Cargo.toml             crates/oxify-model/Cargo.toml
COPY crates/oxify-server/Cargo.toml            crates/oxify-server/Cargo.toml
COPY crates/oxify-storage/Cargo.toml           crates/oxify-storage/Cargo.toml
COPY crates/oxify-ui/Cargo.toml                crates/oxify-ui/Cargo.toml
COPY crates/oxify-vector/Cargo.toml            crates/oxify-vector/Cargo.toml

# Create stub lib.rs / main.rs files for every crate so that `cargo fetch`
# and dependency compilation succeeds before copying real sources.
RUN for crate in \
        oxify oxify-api oxify-authn oxify-authz \
        oxify-connect-comm oxify-connect-data oxify-connect-db \
        oxify-connect-graphql oxify-connect-llm oxify-connect-storage \
        oxify-connect-vector oxify-connect-vision oxify-engine \
        oxify-mcp oxify-model oxify-server oxify-storage oxify-vector; do \
    mkdir -p "crates/${crate}/src" && \
    echo "" > "crates/${crate}/src/lib.rs"; \
    done && \
    mkdir -p crates/oxify-cli/src && echo "fn main() {}" > crates/oxify-cli/src/main.rs && \
    mkdir -p crates/oxify-ui/src && echo "fn main() {}" > crates/oxify-ui/src/main.rs

# Pre-compile dependencies only (cached layer)
RUN cargo build --release --bin oxify-ui --bin oxify-cli 2>&1 || true
RUN find target -name "*.d" -delete

# Now copy the real sources and build for real
COPY crates/ crates/

# Touch main entry points so Cargo knows they changed
RUN touch crates/oxify-ui/src/main.rs crates/oxify-cli/src/main.rs

RUN cargo build --release --bin oxify-ui --bin oxify-cli 2>&1

# =============================================================================
# Stage 2: runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

ARG OXIFY_LOG_LEVEL

# Install runtime dependencies only (no build tools)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root system user
RUN groupadd --system --gid 1000 oxify && \
    useradd --system --uid 1000 --gid oxify --shell /bin/false --no-create-home oxify

WORKDIR /app

# Copy compiled binaries from builder stage
COPY --from=builder /app/target/release/oxify-ui  /usr/local/bin/oxify-ui
COPY --from=builder /app/target/release/oxify-cli /usr/local/bin/oxify-cli

# Copy UI static assets and Askama templates (needed at runtime)
COPY --from=builder /app/crates/oxify-ui/static    /app/crates/oxify-ui/static
COPY --from=builder /app/crates/oxify-ui/templates /app/crates/oxify-ui/templates

# Create writable tmp directory used by the app at runtime
RUN mkdir -p /app/tmp && chown -R oxify:oxify /app

# Drop privileges
USER oxify

# oxify-ui listens on port 3000 by default (OXIFY_UI_PORT env var controls this)
EXPOSE 3000

# Default environment
ENV RUST_LOG=${OXIFY_LOG_LEVEL} \
    OXIFY_UI_HOST=0.0.0.0 \
    OXIFY_UI_PORT=3000 \
    OXIFY_UI_USE_MOCK=false

# Health check — the UI server exposes a lightweight /health endpoint.
# Allow 30s startup, then check every 15s with a 5s timeout.
HEALTHCHECK --interval=15s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

ENTRYPOINT ["oxify-ui"]
