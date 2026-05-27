.PHONY: all build test node medium provision clean help

all: build

build:
	@echo "=== Building all Seamless System workspace components... ==="
	cargo build

test:
	@echo "=== Running all Unit & E2E Integration tests... ==="
	cargo test

node:
	@echo "=== Starting Seamless Node (Ctrl+C to stop) ==="
	cargo run --package seamless-node

medium:
	@echo "=== Starting Swarm Medium Interface (q to quit) ==="
	cargo run --package swarm-medium

provision:
	@echo "=== Key Provisioning CLI ==="
	cargo run --package provision-keys -- --help

clean:
	@echo "=== Cleaning Cargo target build directories... ==="
	cargo clean

help:
	@echo "========================================================================="
	@echo "                   Seamless System — Makefile                            "
	@echo "========================================================================="
	@echo "  make build      : Compile all workspace crates"
	@echo "  make test       : Run unit and integration tests"
	@echo "  make node       : Start a node (auto-elects leader or joins as follower)"
	@echo "  make medium     : Start the interactive Medium CLI (user task interface)"
	@echo "  make provision  : Key provisioning CLI tool"
	@echo "  make clean      : Clean build artifacts"
	@echo "========================================================================="
	@echo ""
	@echo "  Demo flow (2+ terminals):"
	@echo "    Terminal 1: make node   # first node up becomes leader"
	@echo "    Terminal 2: make node   # subsequent nodes join as followers"
	@echo "    Terminal 3: make medium # connect the task UI to the current leader"
	@echo "========================================================================="
