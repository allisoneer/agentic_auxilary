.PHONY: all check check-normal check-verbose test test-normal test-verbose build build-normal build-verbose fmt fmt-check clean doc audit outdated install run help

# Default targets (silent if successful)
all: check test build

check:
	@echo "Checking code formatting..."
	@./hack/cargo-smart.sh fmt-check
	@echo "Running clippy..."
	@./hack/cargo-smart.sh clippy --all-targets -- -D warnings

test:
	@./hack/cargo-smart.sh test

build:
	@./hack/cargo-smart.sh build

# Normal output versions
check-normal:
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings

test-normal:
	cargo test

build-normal:
	cargo build

# Verbose versions
check-verbose:
	cargo fmt --all -- --check
	cargo clippy --all-targets --verbose -- -D warnings

test-verbose:
	cargo test --verbose -- --nocapture

build-verbose:
	cargo build --verbose

# Other targets
fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clean:
	cargo clean

doc:
	cargo doc --no-deps --open

audit:
	cargo audit

outdated:
	cargo outdated

# Installation
install:
	cargo install --path .

# Development
run:
	cargo run --

help:
	@echo "Available targets:"
	@echo "  all          - Run check, test, and build (default, silent)"
	@echo "  check        - Run formatting and clippy checks (silent)"
	@echo "  test         - Run tests (silent)"
	@echo "  build        - Build the project (silent)"
	@echo "  *-normal     - Normal output versions of check/test/build"
	@echo "  *-verbose    - Verbose output versions of check/test/build"
	@echo "  fmt          - Format code"
	@echo "  fmt-check    - Check code formatting"
	@echo "  clean        - Clean build artifacts"
	@echo "  doc          - Build and open documentation"
	@echo "  audit        - Check for security vulnerabilities"
	@echo "  outdated     - Check for outdated dependencies"
	@echo "  install      - Install the tool"
	@echo "  run          - Run the tool"
