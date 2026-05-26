.PHONY: all build test hub agent provision clean help

all: build

build:
	@echo "=== Building all Seamless Swarm workspace components... ==="
	cargo build

test:
	@echo "=== Running all Unit & E2E Integration tests... ==="
	cargo test

hub:
	@echo "=== Starting Central Hub / ARM Appliance Simulator... ==="
	cargo run --package compute-module-core

agent:
	@echo "=== Starting Host Background Agent (Scout Model Simulation)... ==="
	cargo run --package host-background-agent

provision:
	@echo "=== Running Key Provisioning CLI Tool... ==="
	cargo run --package provision-keys -- --help

clean:
	@echo "=== Cleaning Cargo target build directories... ==="
	cargo clean

help:
	@echo "========================================================================="
	@echo "                      Seamless Swarm - Makefile                          "
	@echo "========================================================================="
	@echo "  make build      : Compile all Rust workspace crates"
	@echo "  make test       : Execute all unit and E2E integration tests"
	@echo "  make hub        : Start the Central Hub orchestration service"
	@echo "  make agent      : Start a local Host Workstation background agent"
	@echo "  make provision  : Display help/options for the key provisioning CLI"
	@echo "  make clean      : Clear target build caches"
	@echo "========================================================================="
