# Stage 1: Build
FROM rust:1.77-slim-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    mkdir -p src/scanner src/treasury && \
    echo "pub mod analysis;" > src/scanner/mod.rs && \
    echo "" > src/scanner/analysis.rs && \
    echo "" > src/treasury/guard.rs && \
    echo "" > src/arbitrage.rs && \
    echo "" > src/rpc_rotator.rs && \
    cargo build --release 2>/dev/null || true && \
    rm -rf src

COPY src/ src/
RUN cargo build --release --bin l2-arb-engine

# Stage 2: Runtime
FROM gcr.io/distroless/cc-debian12:nonroot

LABEL org.opencontainers.image.title="L2 Arbitrage Engine v5"
LABEL org.opencontainers.image.description="High-performance L2 arbitrage trading engine"
LABEL org.opencontainers.image.source="https://github.com/mina-alpha/l2-arbitrage-engine-v5"
LABEL org.opencontainers.image.version="5.0.0"

COPY --from=builder /build/target/release/l2-arb-engine /l2-arb-engine

COPY .env.example /.env.example
COPY contracts/ /contracts/

EXPOSE 8080 9090

USER nonroot:nonroot

ENTRYPOINT ["/l2-arb-engine"]
