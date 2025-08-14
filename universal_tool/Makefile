.PHONY: all check test build check-normal check-verbose test-normal test-verbose build-normal build-verbose \
        clean fmt fmt-check doc test-unit test-integration test-all audit outdated help \
        test-examples test-macros test-core

# Default environment variables for silent output
SILENT_ENV = CARGO_TERM_QUIET=true CARGO_TERM_PROGRESS_WHEN=never CARGO_TERM_VERBOSE=false

# TODO: Remove RUSTFLAGS="-A warnings" once we fix all warnings
SILENT_FLAGS = RUSTFLAGS="-A warnings"

# Default targets run silently with warnings disabled
all: check test build

# Silent versions (default) - uses script to show success/failure
check:
	@$(SILENT_ENV) $(SILENT_FLAGS) ./hack/run-silent.sh cargo clippy -p 'universal-tool-*' -p 'example-*' --all-features

test:
	@echo "Running tests..."
	@$(SILENT_ENV) $(SILENT_FLAGS) ./hack/run-silent.sh cargo test -p 'universal-tool-*' -p 'example-*' --all-features

build:
	@$(SILENT_ENV) $(SILENT_FLAGS) ./hack/run-silent.sh cargo build -p 'universal-tool-*' -p 'example-*' --all-features

# Normal versions - standard output
check-normal:
	cargo clippy -p 'universal-tool-*' -p 'example-*' --all-features

test-normal:
	cargo test -p 'universal-tool-*' -p 'example-*' --all-features

build-normal:
	cargo build -p 'universal-tool-*' -p 'example-*' --all-features

# Verbose versions - extra output
check-verbose:
	cargo clippy -p 'universal-tool-*' -p 'example-*' --all-features --verbose

test-verbose:
	cargo test -p 'universal-tool-*' -p 'example-*' --all-features --verbose -- --nocapture

build-verbose:
	cargo build -p 'universal-tool-*' -p 'example-*' --all-features --verbose

# Additional useful targets
clean:
	cargo clean

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

doc:
	cargo doc -p 'universal-tool-*' -p 'example-*' --no-deps --open

# Test targets
test-unit:
	@echo "Running unit tests..."
	cargo test -p 'universal-tool-*' -p 'example-*' --lib

test-integration:
	@echo "Running integration tests..."
	cargo test -p 'universal-tool-*' -p 'example-*' --test '*'

test-all:
	@echo "Running all tests..."
	cargo test -p 'universal-tool-*' -p 'example-*' --all-features

# Component-specific test targets
test-core:
	@echo "Testing universal-tool-core..."
	cargo test -p universal-tool-core --all-features

test-macros:
	@echo "Testing universal-tool-macros..."
	cargo test -p universal-tool-macros

test-examples:
	@echo "Testing examples..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Testing $$example..."; \
			cargo test --manifest-path "$$example/Cargo.toml"; \
		fi \
	done

# Dependency management
audit:
	cargo audit

outdated:
	cargo outdated

update:
	cargo update

# Release build
release:
	cargo build -p 'universal-tool-*' -p 'example-*' --all-features --release

# Example runners
run-cli-simple:
	cargo run --example 01-cli-simple -- $(ARGS)

run-cli-advanced:
	cargo run --example 02-cli-advanced-params -- $(ARGS)

run-rest-simple:
	cargo run --example 03-rest-simple

run-rest-idiomatic:
	cargo run --example 04-rest-idiomatic

run-mcp:
	cargo run --example 05-mcp-basic

run-kitchen-sink:
	cargo run --example 06-kitchen-sink -- $(ARGS)

# Install examples as binaries
install-examples:
	@echo "Installing examples..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Installing $$example..."; \
			cargo install --path "$$example"; \
		fi \
	done

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
	@echo "  make test-unit       - Run unit tests only"
	@echo "  make test-integration - Run integration tests"
	@echo "  make test-all        - Run all tests with all features"
	@echo "  make test-core       - Test universal-tool-core"
	@echo "  make test-macros     - Test universal-tool-macros"
	@echo "  make test-examples   - Test all examples"
	@echo ""
	@echo "  make audit           - Check for security vulnerabilities"
	@echo "  make outdated        - Check for outdated dependencies"
	@echo "  make update          - Update dependencies"
	@echo "  make release         - Build release version"
	@echo ""
	@echo "Example runners:"
	@echo "  make run-cli-simple ARGS='add 5 3'     - Run CLI simple example"
	@echo "  make run-cli-advanced ARGS='...'       - Run CLI advanced example"
	@echo "  make run-rest-simple                   - Run REST simple example"
	@echo "  make run-rest-idiomatic                - Run REST idiomatic example"
	@echo "  make run-mcp                           - Run MCP example"
	@echo "  make run-kitchen-sink ARGS='...'       - Run kitchen sink example"
	@echo ""
	@echo "  make install-examples - Install all examples as binaries"
	@echo ""
	@echo "  make all             - Run check, test, and build (default)"
	@echo "  make help            - Show this help message"