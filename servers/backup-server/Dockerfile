# Dockerfile for poly-backup-server
# Minimal multi-stage build for encrypted backup sync server
#
# Build:   docker build -t poly-backup-server servers/backup-server/
# Run:     docker run -e POLY_BACKUP_PASSPHRASE=... -v /data:/data poly-backup-server

FROM rust:1-bookworm AS builder

WORKDIR /build

# Copy workspace manifests (for dependency caching)
COPY Cargo.toml Cargo.lock ./

# Copy only the server crate manifest (no apps, no unnecessary clients)
COPY servers/backup-server ./servers/backup-server

# Create dummy lib.rs for dependency layer caching
RUN mkdir -p servers/backup-server/src && \
    echo "fn main() {}" > servers/backup-server/src/main.rs

# Pre-build dependencies only (cached if Cargo.toml unchanged)
RUN cargo build -p poly-backup-server --release 2>&1 | grep -v "could not compile" || true

# Copy actual source
COPY servers/backup-server ./servers/backup-server

# Invalidate build cache and rebuild
RUN touch servers/backup-server/src/main.rs && \
    cargo build -p poly-backup-server --release

# --- Runtime stage ---
FROM debian:bookworm-slim

# Install only CA certs (minimal runtime dependencies)
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/poly-backup-server /usr/local/bin/

# Create data directory for SurrealKV
RUN mkdir -p /data && chmod 700 /data

# Configuration via environment variables
ENV POLY_BACKUP_BIND=0.0.0.0:3080
ENV POLY_BACKUP_DATA_DIR=/data
ENV POLY_BACKUP_NAME="Poly Backup Server"
ENV RUST_LOG=info,poly_backup_server=debug

# Health check: test if server is listening
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD /usr/bin/wget --no-verbose --tries=1 --spider http://127.0.0.1:3080/api/challenge || exit 1

EXPOSE 3080

# Run the backup server
CMD ["poly-backup-server"]
