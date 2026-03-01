# Dockerfile for poly-server
# Minimal multi-stage build for self-hosted chat server reference implementation
#
# Build:   docker build -t poly-server servers/server/
# Run:     docker run -e POLY_SERVER_BIND=0.0.0.0:7080 -v /data:/data poly-server

FROM rust:1-bookworm AS builder

WORKDIR /build

# Copy workspace manifests (for dependency caching)
COPY Cargo.toml Cargo.lock ./

# Copy only the server crate manifest (no apps, no unnecessary clients)
COPY servers/server ./servers/server

# Create dummy main.rs for dependency layer caching
RUN mkdir -p servers/server/src && \
    echo "fn main() {}" > servers/server/src/main.rs

# Pre-build dependencies only (cached if Cargo.toml unchanged)
RUN cargo build -p poly-server --release 2>&1 | grep -v "could not compile" || true

# Copy actual source
COPY servers/server ./servers/server

# Invalidate build cache and rebuild
RUN touch servers/server/src/main.rs && \
    cargo build -p poly-server --release

# --- Runtime stage ---
FROM debian:bookworm-slim

# Install only CA certs (minimal runtime dependencies)
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/poly-server /usr/local/bin/

# Create data directory for SurrealKV
RUN mkdir -p /data && chmod 700 /data

# Configuration via environment variables
ENV POLY_SERVER_BIND=0.0.0.0:7080
ENV POLY_SERVER_DB_PATH=/data/server.db
ENV POLY_SERVER_UPLOADS_DIR=/data/uploads
ENV POLY_SERVER_NAME="Poly Chat Server"
ENV POLY_SERVER_JWT_SECRET=change-me-in-production
ENV RUST_LOG=info,poly_server=debug

# Health check: test if server is listening
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD /usr/bin/wget --no-verbose --tries=1 --spider http://127.0.0.1:7080/health || exit 1

EXPOSE 7080

# Run the server
CMD ["poly-server"]
