# ─── Stage 1: Build ───────────────────────────────────────────────────────────
FROM rust:1.77-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    mkdir -p src/hunter src/treasury && \
    echo "pub mod scanner;" > src/hunter/mod.rs && \
    echo "" > src/hunter/scanner.rs && \
    echo "" > src/treasury/guard.rs && \
    echo "" > src/red_mev.rs && \
    echo "" > src/rpc_rotator.rs && \
    cargo build --release 2>/dev/null || true && \
    rm -rf src

# Build actual binary
COPY src/ src/
RUN cargo build --release --bin uesh

# ─── Stage 2: Distroless Runtime ──────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot

LABEL org.opencontainers.image.title="UESH Kilo Immortal v5"
LABEL org.opencontainers.image.description="Autonomous Web3 organism - immortal phase engine"
LABEL org.opencontainers.image.source="https://github.com/mina-alpha/uesh-kilo-immortal-v5"
LABEL org.opencontainers.image.version="5.0.0"

# Copy binary from builder
COPY --from=builder /build/target/release/uesh /uesh

# Copy config files
COPY .env.example /.env.example
COPY deploy_akash.py /deploy_akash.py
COPY contracts/ /contracts/

# Expose ports
#   8080 - HTTP API (Axum proxy + health + status)
#   4001 - libp2p swarm
#   9090 - Prometheus metrics
EXPOSE 8080 4001 9090

# Health check
# (distroless has no shell, so we use the binary itself)
# Kilo agent monitors /health endpoint externally

# Run as nonroot user
USER nonroot:nonroot

# Entrypoint
ENTRYPOINT ["/uesh"]
