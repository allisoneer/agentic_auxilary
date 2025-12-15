.PHONY: all check test build check-normal check-verbose test-normal test-verbose build-normal build-verbose \
        clean fmt fmt-check doc audit outdated help install run release

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
	@echo "Checking code formatting..."
	@cargo fmt -- --check
	@echo "Running clippy..."
	@./hack/cargo-smart.sh clippy --all-features --all-targets -- -D warnings

test:
	@echo "━━━ Testing anthropic_async ━━━"
	@./hack/cargo-smart.sh test --all-features

build:
	@echo "━━━ Building anthropic_async ━━━"
	@./hack/cargo-smart.sh build

# Normal versions - standard output
check-normal:
	cargo fmt -- --check
	cargo clippy --all-targets --all-features -- -D warnings

test-normal:
	cargo test --all-features

build-normal:
	cargo build

# Verbose versions - extra output
check-verbose:
	cargo clippy --verbose --all-targets --all-features -- -D warnings

test-verbose:
	cargo test --verbose --all-features -- --nocapture

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

# Dependency management
audit:
	cargo audit

outdated:
	cargo outdated

# Release build
release:
	cargo build --release

# Run the application (library, so show example instructions)
run:
	@echo "anthropic-async is a library crate. Run examples instead:"
	@echo "  cd examples/01-basic-completion && cargo run"
	@echo "  cd examples/04-model-listing && cargo run"
	@echo "  cd examples/05-kitchen-sink && cargo run"

# Install the application (installs to cargo registry cache)
install:
	cargo install --path .

# Help target
help:
	@echo "Available targets:"
	@echo "  make check       - Run clippy (silent with warnings disabled)"
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
	@echo "  make audit           - Check for security vulnerabilities"
	@echo "  make outdated        - Check for outdated dependencies"
	@echo "  make release         - Build release version"
	@echo "  make run             - Show how to run examples"
	@echo "  make install         - Install to cargo registry"
	@echo ""
	@echo "  make all             - Run check, test, and build (default)"
	@echo "  make help            - Show this help message"
