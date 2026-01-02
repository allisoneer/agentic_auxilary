.PHONY: all check check-normal check-verbose test test-normal test-verbose build build-normal build-verbose fmt fmt-check clean doc audit outdated install run help

# Default targets (silent if successful)
all: check test build

check:
	@echo "Checking code formatting..."
	@./hack/cargo-smart.sh fmt-check --manifest-path linear-schema/Cargo.toml
	@./hack/cargo-smart.sh fmt-check --manifest-path linear-queries/Cargo.toml
	@./hack/cargo-smart.sh fmt-check --manifest-path linear-tools/Cargo.toml
	@echo "Running clippy..."
	@./hack/cargo-smart.sh clippy -p linear-schema -p linear-queries -p linear-tools --all-targets -- -D warnings

test:
	@./hack/cargo-smart.sh test -p linear-schema -p linear-queries -p linear-tools

build:
	@./hack/cargo-smart.sh build -p linear-schema -p linear-queries -p linear-tools

# Normal output versions
check-normal:
	cargo fmt --manifest-path linear-schema/Cargo.toml -- --check
	cargo fmt --manifest-path linear-queries/Cargo.toml -- --check
	cargo fmt --manifest-path linear-tools/Cargo.toml -- --check
	cargo clippy -p linear-schema -p linear-queries -p linear-tools --all-targets -- -D warnings

test-normal:
	cargo test -p linear-schema -p linear-queries -p linear-tools

build-normal:
	cargo build -p linear-schema -p linear-queries -p linear-tools

# Verbose versions
check-verbose:
	cargo fmt --manifest-path linear-schema/Cargo.toml -- --check
	cargo fmt --manifest-path linear-queries/Cargo.toml -- --check
	cargo fmt --manifest-path linear-tools/Cargo.toml -- --check
	cargo clippy -p linear-schema -p linear-queries -p linear-tools --all-targets --verbose -- -D warnings

test-verbose:
	cargo test -p linear-schema -p linear-queries -p linear-tools --verbose -- --nocapture

build-verbose:
	cargo build -p linear-schema -p linear-queries -p linear-tools --verbose

# Other targets
fmt:
	cargo fmt --manifest-path linear-schema/Cargo.toml
	cargo fmt --manifest-path linear-queries/Cargo.toml
	cargo fmt --manifest-path linear-tools/Cargo.toml

fmt-check:
	cargo fmt --manifest-path linear-schema/Cargo.toml -- --check
	cargo fmt --manifest-path linear-queries/Cargo.toml -- --check
	cargo fmt --manifest-path linear-tools/Cargo.toml -- --check

clean:
	cargo clean

doc:
	cargo doc -p linear-schema -p linear-queries -p linear-tools --no-deps --open

audit:
	cargo audit -p linear-schema -p linear-queries -p linear-tools

outdated:
	cargo outdated -p linear-schema -p linear-queries -p linear-tools

# Installation
install:
	cargo install --path linear-tools

# Development
run:
	cargo run -p linear-tools --

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
