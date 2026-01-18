# Catalyst Node Makefile
# Provides convenient commands for building, testing, and running the Catalyst node

.PHONY: help build test run clean setup dev docker fmt clippy docs bench

# Default target
help:
	@echo "Catalyst Node Development Commands"
	@echo "================================="
	@echo ""
	@echo "Setup and Build:"
	@echo "  setup     - Install dependencies and set up development environment"
	@echo "  build     - Build the project in release mode"
	@echo "  dev       - Build the project in development mode"
	@echo "  clean     - Clean build artifacts"
	@echo ""
	@echo "Code Quality:"
	@echo "  fmt       - Format code with rustfmt"
	@echo "  clippy    - Run clippy lints"
	@echo "  test      - Run all tests"
	@echo "  bench     - Run benchmarks"
	@echo ""
	@echo "Documentation:"
	@echo "  docs      - Generate and open documentation"
	@echo ""
	@echo "Running:"
	@echo "  run       - Run the node with default configuration"
	@echo "  run-validator - Run as a validator node"
	@echo "  run-storage   - Run with storage provision enabled"
	@echo ""
	@echo "Docker:"
	@echo "  docker-build  - Build Docker image"
	@echo "  docker-run    - Run Docker container"
	@echo ""
	@echo "Network:"
	@echo "  genesis   - Create genesis configuration"
	@echo "  testnet   - Start local testnet with 3 nodes"

# Variables
CARGO = cargo
DOCKER = docker
PROJECT_NAME = catalyst-node
RUST_LOG ?= info

# Cargo target directory
# When working from GVFS/SMB mounts, file locking can be unsupported which breaks
# Cargo's `.cargo-lock` in `target/` (os error 95). Use a local target dir by default.
PROJECT_ROOT := $(CURDIR)
DEFAULT_CARGO_TARGET_DIR := $(PROJECT_ROOT)/target
GVFS_CARGO_TARGET_DIR := $(HOME)/.cache/$(PROJECT_NAME)/cargo-target
GVFS_WORKDIR := $(HOME)/.cache/$(PROJECT_NAME)/gvfs-workdir
GVFS_MIRROR_DIR := $(GVFS_WORKDIR)/src

ifeq (,$(findstring gvfs,$(PROJECT_ROOT)))
  CARGO_TARGET_DIR ?= $(DEFAULT_CARGO_TARGET_DIR)
  CARGO_MANIFEST_PATH ?= $(PROJECT_ROOT)/Cargo.toml
else
  CARGO_TARGET_DIR ?= $(GVFS_CARGO_TARGET_DIR)
  # On GVFS/SMB mounts, Cargo may fail to lock/write Cargo.lock. Build from a local mirror.
  CARGO_MANIFEST_PATH ?= $(GVFS_MIRROR_DIR)/Cargo.toml
endif

export CARGO_TARGET_DIR

# Keep a local mirror of the source tree when building from GVFS/SMB.
.PHONY: sync-gvfs-mirror
sync-gvfs-mirror:
ifeq (,$(findstring gvfs,$(PROJECT_ROOT)))
	@true
else
	@mkdir -p "$(GVFS_MIRROR_DIR)"
	@rsync -a --delete \
		--exclude 'target' \
		--exclude '.git' \
		"$(PROJECT_ROOT)/" "$(GVFS_MIRROR_DIR)/"
endif

# System dependency checks
.PHONY: check-system-deps
check-system-deps:
	@command -v cc >/dev/null 2>&1 || { \
		echo "Missing C toolchain (cc). On Ubuntu/Debian: sudo apt install build-essential"; \
		exit 1; \
	}
	@printf '#include <stddef.h>\n' | cc -E - >/dev/null 2>&1 || { \
		echo "C standard headers not found (missing stddef.h). On Ubuntu/Debian: sudo apt install build-essential"; \
		exit 1; \
	}
	@command -v pkg-config >/dev/null 2>&1 || { \
		echo "Missing pkg-config. On Ubuntu/Debian: sudo apt install pkg-config"; \
		exit 1; \
	}
	@pkg-config --exists openssl >/dev/null 2>&1 || { \
		echo "Missing OpenSSL development headers (.pc not found). On Ubuntu/Debian: sudo apt install libssl-dev"; \
		exit 1; \
	}
	@command -v clang >/dev/null 2>&1 || { \
		echo "Missing clang (needed by bindgen). On Ubuntu/Debian: sudo apt install clang libclang-dev"; \
		exit 1; \
	}
	@command -v cmake >/dev/null 2>&1 || { \
		echo "Missing cmake (often required by native deps like RocksDB). On Ubuntu/Debian: sudo apt install cmake"; \
		exit 1; \
	}

# Setup development environment
setup:
	@echo "Setting up Catalyst development environment..."
	@$(MAKE) check-system-deps
	rustup update
	rustup component add rustfmt clippy
	$(CARGO) install cargo-audit cargo-outdated
	@echo "Installing IPFS for DFS support..."
	@if ! command -v ipfs &> /dev/null; then \
		echo "Please install IPFS manually: https://ipfs.io/docs/install/"; \
	fi
	@echo "Setup complete!"

# Build commands
build:
	@echo "Building Catalyst node (release)..."
	@$(MAKE) check-system-deps
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) build --release --workspace --manifest-path "$(CARGO_MANIFEST_PATH)"

dev:
	@echo "Building Catalyst node (debug)..."
	@$(MAKE) check-system-deps
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) build --workspace --manifest-path "$(CARGO_MANIFEST_PATH)"

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	$(CARGO) clean
	rm -rf target/
	rm -rf data/
	rm -rf logs/
	rm -rf dfs_cache/

# Code formatting and linting
fmt:
	@echo "Formatting code..."
	$(CARGO) fmt --all

clippy:
	@echo "Running clippy..."
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

# Testing
test:
	@echo "Running tests..."
	@$(MAKE) sync-gvfs-mirror
	RUST_LOG=debug $(CARGO) test --workspace --manifest-path "$(CARGO_MANIFEST_PATH)"

test-all-features:
	@echo "Running tests (all features)..."
	@$(MAKE) sync-gvfs-mirror
	RUST_LOG=debug $(CARGO) test --workspace --all-features --manifest-path "$(CARGO_MANIFEST_PATH)"

test-integration:
	@echo "Running integration tests..."
	@$(MAKE) sync-gvfs-mirror
	RUST_LOG=debug $(CARGO) test --workspace --test integration_tests --manifest-path "$(CARGO_MANIFEST_PATH)"

bench:
	@echo "Running benchmarks..."
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) bench --workspace --manifest-path "$(CARGO_MANIFEST_PATH)"

# Documentation
docs:
	@echo "Generating documentation..."
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) doc --workspace --all-features --no-deps --open --manifest-path "$(CARGO_MANIFEST_PATH)"

# Run commands
run: build
	@echo "Starting Catalyst node..."
	RUST_LOG=$(RUST_LOG) $(CARGO_TARGET_DIR)/release/catalyst-cli start

run-dev: dev
	@echo "Starting Catalyst node (debug)..."
	RUST_LOG=$(RUST_LOG) $(CARGO) run --bin catalyst-cli -- start

run-validator: build
	@echo "Starting Catalyst validator node..."
	RUST_LOG=$(RUST_LOG) $(CARGO_TARGET_DIR)/release/catalyst-cli start --validator --rpc

run-storage: build
	@echo "Starting Catalyst storage node..."
	RUST_LOG=$(RUST_LOG) $(CARGO_TARGET_DIR)/release/catalyst-cli start --storage --storage-capacity 50

run-full: build
	@echo "Starting full Catalyst node (validator + storage + RPC)..."
	RUST_LOG=$(RUST_LOG) $(CARGO_TARGET_DIR)/release/catalyst-cli start --validator --storage --rpc

# Docker commands
docker-build:
	@echo "Building Docker image..."
	$(DOCKER) build -t $(PROJECT_NAME):latest .

docker-run:
	@echo "Running Docker container..."
	$(DOCKER) run -p 8545:8545 -p 8546:8546 -p 30333:30333 \
		-v $(PWD)/data:/app/data \
		$(PROJECT_NAME):latest

# Network setup
genesis:
	@echo "Creating genesis configuration..."
	mkdir -p configs
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) run --bin catalyst-cli --manifest-path "$(CARGO_MANIFEST_PATH)" -- create-genesis --output configs/genesis.json

identity:
	@echo "Generating node identity..."
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) run --bin catalyst-cli --manifest-path "$(CARGO_MANIFEST_PATH)" -- generate-identity --output identity.json

# Development utilities
check:
	@echo "Running cargo check..."
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) check --workspace --all-targets --all-features --manifest-path "$(CARGO_MANIFEST_PATH)"

audit:
	@echo "Running security audit..."
	$(CARGO) audit

outdated:
	@echo "Checking for outdated dependencies..."
	$(CARGO) outdated

update:
	@echo "Updating dependencies..."
	$(CARGO) update

# Local testnet
testnet: build
	@echo "Starting local testnet with 3 nodes..."
	@mkdir -p testnet/{node1,node2,node3}
	@# Node 1 (Bootstrap + Validator)
	RUST_LOG=info $(CARGO_TARGET_DIR)/release/catalyst-cli start \
		--config testnet/node1/config.toml \
		--validator --rpc --rpc-port 8545 &
	@sleep 2
	@# Node 2 (Validator + Storage)
	RUST_LOG=info $(CARGO_TARGET_DIR)/release/catalyst-cli start \
		--config testnet/node2/config.toml \
		--validator --storage --rpc-port 8546 \
		--bootstrap-peers "/ip4/127.0.0.1/tcp/30333" &
	@sleep 2
	@# Node 3 (Storage only)
	RUST_LOG=info $(CARGO_TARGET_DIR)/release/catalyst-cli start \
		--config testnet/node3/config.toml \
		--storage --rpc-port 8547 \
		--bootstrap-peers "/ip4/127.0.0.1/tcp/30333" &
	@echo "Testnet started! Nodes running on ports 8545, 8546, 8547"
	@echo "Press Ctrl+C to stop all nodes"
	@wait

stop-testnet:
	@echo "Stopping testnet..."
	@pkill -f catalyst-cli || true

# Development helpers
watch:
	@echo "Watching for changes and rebuilding..."
	$(CARGO) watch -x "build --workspace"

watch-test:
	@echo "Watching for changes and running tests..."
	$(CARGO) watch -x "test --workspace"

# Performance profiling
profile:
	@echo "Building with profiling..."
	@$(MAKE) sync-gvfs-mirror
	$(CARGO) build --release --workspace --manifest-path "$(CARGO_MANIFEST_PATH)"
	@echo "Run with: perf record $(CARGO_TARGET_DIR)/release/catalyst-cli start"

# Release preparation
pre-release: fmt clippy test audit
	@echo "Pre-release checks complete!"

# Installation
install: build
	@echo "Installing Catalyst CLI..."
	$(CARGO) install --path crates/catalyst-cli --force

uninstall:
	@echo "Uninstalling Catalyst CLI..."
	$(CARGO) uninstall catalyst-cli

# Status check
status:
	@echo "Checking node status..."
	$(CARGO) run --bin catalyst-cli -- status

peers:
	@echo "Checking connected peers..."
	$(CARGO) run --bin catalyst-cli -- peers

# Backup and restore
backup:
	@echo "Creating backup..."
	@mkdir -p backups
	tar -czf backups/catalyst-backup-$(shell date +%Y%m%d-%H%M%S).tar.gz data/ configs/ *.toml *.json

restore:
	@echo "List available backups:"
	@ls -la backups/
	@echo "To restore: tar -xzf backups/[backup-file]"