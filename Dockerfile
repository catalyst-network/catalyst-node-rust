# syntax=docker/dockerfile:1

############################
# Build stage
############################
FROM rust:1.80 AS build

# Needed for crates with native deps (rocksdb/zstd, openssl) and bindgen
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang llvm-dev libclang-dev \
    build-essential pkg-config cmake \
    libssl-dev zlib1g-dev libzstd-dev liblz4-dev libbz2-dev \
    protobuf-compiler ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# If you want faster rebuilds, you can copy manifests first.
# With a good .dockerignore you can also just copy the whole repo.
COPY . .

# Build the workspace release binaries (we need catalyst-node)
RUN cargo build --release --workspace

############################
# Runtime stage
############################
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1001 catalyst
USER catalyst
WORKDIR /app

# Copy only the node binary
COPY --from=build /app/target/release/catalyst-node /usr/local/bin/catalyst-node

# Optional: declare the typical ports (Compose will map as needed)
EXPOSE 30333 8545 8546

# Proper signal handling
ENTRYPOINT ["/usr/bin/tini","--","/usr/local/bin/catalyst-node"]
CMD []
