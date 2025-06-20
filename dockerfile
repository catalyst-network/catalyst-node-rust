# Multi-stage build for Catalyst Node
FROM rust:1.75-bullseye as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    cmake \
    git \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1001 catalyst

# Set working directory
WORKDIR /app

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Build dependencies (this layer will be cached if Cargo.toml doesn't change)
RUN cargo build --release --workspace

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    curl \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Install IPFS
RUN wget https://dist.ipfs.io/kubo/v0.24.0/kubo_v0.24.0_linux-amd64.tar.gz \
    && tar -xzf kubo_v0.24.0_linux-amd64.tar.gz \
    && cd kubo && bash install.sh \
    && rm -rf /kubo* \
    && ipfs --version

# Create app user
RUN useradd -m -u 1001 catalyst

# Create necessary directories
RUN mkdir -p /app/data /app/logs /app/configs /app/dfs_cache \
    && chown -R catalyst:catalyst /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/catalyst-cli /usr/local/bin/catalyst

# Copy default configuration
COPY configs/docker.toml /app/catalyst.toml

# Switch to app user
USER catalyst

# Set working directory
WORKDIR /app

# Initialize IPFS repository
RUN ipfs init --profile server

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD catalyst status --rpc-url http://localhost:8545 || exit 1

# Expose ports
# 30333: P2P networking
# 8545: RPC server
# 8546: WebSocket service bus
# 5001: IPFS API
# 8080: IPFS Gateway
EXPOSE 30333 8545 8546 5001 8080

# Default command
CMD ["catalyst", "start", "--config", "/app/catalyst.toml"]

# Labels for metadata
LABEL org.label-schema.name="Catalyst Node" \
      org.label-schema.description="Catalyst Network blockchain node" \
      org.label-schema.version="0.1.0" \
      org.label-schema.vendor="Catalyst Network" \
      org.label-schema.schema-version="1.0"