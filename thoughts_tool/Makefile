.PHONY: all check test build check-normal check-verbose test-normal test-verbose build-normal build-verbose \
        clean fmt fmt-check doc test-unit test-integration test-all audit outdated help

# Default environment variables for silent output
SILENT_ENV = CARGO_TERM_QUIET=true CARGO_TERM_PROGRESS_WHEN=never CARGO_TERM_VERBOSE=false

# Default targets run silently with warnings disabled
all: check test build
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "✅ All checks passed successfully!"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Silent versions (default) - smart output with fail-fast
check:
	@echo "━━━ Checking thoughts_tool ━━━"
	@echo "Checking code formatting..."
	@./hack/cargo-smart.sh fmt-check
	@echo "Running clippy..."
	@./hack/cargo-smart.sh clippy

test:
	@echo "━━━ Testing thoughts_tool ━━━"
	@./hack/cargo-smart.sh test

build:
	@echo "━━━ Building thoughts_tool ━━━"
	@./hack/cargo-smart.sh build

# Normal versions - standard output
check-normal:
	cargo fmt -- --check
	cargo clippy

test-normal:
	cargo test

build-normal:
	cargo build

# Verbose versions - extra output
check-verbose:
	cargo clippy --verbose

test-verbose:
	cargo test --verbose -- --nocapture

build-verbose:
	cargo build --verbose

# Additional useful targets
clean:
	cargo clean

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

doc:
	cargo doc --no-deps --open

# Test targets
test-unit:
	@echo "Running unit tests..."
	cargo test --lib

test-integration:
	@echo "Running integration tests..."
	THOUGHTS_INTEGRATION_TESTS=1 cargo test --test mount_integration -- --nocapture

test-all:
	@echo "Running all tests..."
	cargo test --all-features

# Dependency management
audit:
	cargo audit

outdated:
	cargo outdated

# Release build
release:
	cargo build --release

# Run the application
run:
	cargo run

# Install the application
install:
	cargo install --path .

# Help target
help:
	@echo "Available targets:"
	@echo "  make check       - Run formatting and clippy checks (silent)"
	@echo "  make test        - Run tests (silent with warnings disabled)"
	@echo "  make build       - Build project (silent with warnings disabled)"
	@echo ""
	@echo "  make check-normal    - Run clippy with normal output"
	@echo "  make test-normal     - Run tests with normal output"
	@echo "  make build-normal    - Build project with normal output"
	@echo ""
	@echo "  make check-verbose   - Run clippy with verbose output"
	@echo "  make test-verbose    - Run tests with verbose output"
	@echo "  make build-verbose   - Build project with verbose output"
	@echo ""
	@echo "  make clean           - Clean build artifacts"
	@echo "  make fmt             - Format code"
	@echo "  make fmt-check       - Check code formatting"
	@echo "  make doc             - Build and open documentation"
	@echo ""
	@echo "  make test-unit       - Run unit tests only"
	@echo "  make test-integration - Run integration tests (requires permissions)"
	@echo "  make test-all        - Run all tests with all features"
	@echo ""
	@echo "  make audit           - Check for security vulnerabilities"
	@echo "  make outdated        - Check for outdated dependencies"
	@echo "  make release         - Build release version"
	@echo "  make run             - Run the application"
	@echo "  make install         - Install the application"
	@echo ""
	@echo "  make all             - Run check, test, and build (default)"
	@echo "  make help            - Show this help message"
