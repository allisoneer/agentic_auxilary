.PHONY: all check test build check-normal check-verbose test-normal test-verbose build-normal build-verbose \
        clean fmt fmt-check doc test-unit test-integration test-all audit outdated help \
        test-examples test-macros test-core

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
	@echo "━━━ Checking universal-tool packages ━━━"
	@echo "Checking code formatting..."
	@./hack/cargo-smart.sh fmt-check --all
	@echo "Running clippy..."
	@./hack/cargo-smart.sh clippy -p universal-tool-core --all-features
	@./hack/cargo-smart.sh clippy -p universal-tool-macros --all-features
	@echo ""
	@echo "━━━ Checking examples (individually) ━━━"
	@$(MAKE) -s check-examples

test:
	@echo "━━━ Testing universal-tool packages ━━━"
	@./hack/cargo-smart.sh test -p universal-tool-core --all-features
	@./hack/cargo-smart.sh test -p universal-tool-macros
	@echo ""
	@echo "━━━ Testing examples ━━━"
	@$(MAKE) -s test-examples

build:
	@echo "━━━ Building universal-tool packages ━━━"
	@./hack/cargo-smart.sh build -p universal-tool-core --all-features
	@./hack/cargo-smart.sh build -p universal-tool-macros --all-features
	@echo ""
	@echo "━━━ Building examples ━━━"
	@$(MAKE) -s build-examples

# Normal versions - standard output
check-normal:
	cargo fmt --all -- --check
	cargo clippy -p 'universal-tool-*' --all-features
	@$(MAKE) check-examples-normal

test-normal:
	cargo test -p 'universal-tool-*' --all-features
	@$(MAKE) test-examples-normal

build-normal:
	cargo build -p 'universal-tool-*' --all-features
	@$(MAKE) build-examples-normal

# Verbose versions - extra output
check-verbose:
	cargo clippy -p 'universal-tool-*' --all-features --verbose
	@$(MAKE) check-examples-verbose

test-verbose:
	cargo test -p 'universal-tool-*' --all-features --verbose -- --nocapture
	@$(MAKE) test-examples-verbose

build-verbose:
	cargo build -p 'universal-tool-*' --all-features --verbose
	@$(MAKE) build-examples-verbose

# Additional useful targets
clean:
	cargo clean

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

doc:
	cargo doc -p 'universal-tool-*' --no-deps --open

# Test targets
test-unit:
	@echo "Running unit tests..."
	cargo test -p 'universal-tool-*' --lib

test-integration:
	@echo "Running integration tests..."
	cargo test -p 'universal-tool-*' --test '*'

test-all:
	@echo "Running all tests..."
	cargo test -p 'universal-tool-*' --all-features
	@$(MAKE) test-examples

# Component-specific test targets
test-core:
	@echo "Testing universal-tool-core..."
	cargo test -p universal-tool-core --all-features

test-macros:
	@echo "Testing universal-tool-macros..."
	cargo test -p universal-tool-macros

test-examples:
	@set -e; \
	for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			./hack/cargo-smart.sh test --manifest-path "$$example/Cargo.toml" || exit 1; \
		fi \
	done

test-examples-normal:
	@echo "Testing examples..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Testing $$example..."; \
			cargo test --manifest-path "$$example/Cargo.toml"; \
		fi \
	done

test-examples-verbose:
	@echo "Testing examples (verbose)..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Testing $$example..."; \
			cargo test --manifest-path "$$example/Cargo.toml" --verbose -- --nocapture; \
		fi \
	done

check-examples:
	@set -e; \
	for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			./hack/cargo-smart.sh clippy --manifest-path "$$example/Cargo.toml" || exit 1; \
		fi \
	done

check-examples-normal:
	@echo "Checking examples with clippy..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Checking $$example..."; \
			cargo clippy --manifest-path "$$example/Cargo.toml"; \
		fi \
	done

check-examples-verbose:
	@echo "Checking examples with clippy (verbose)..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Checking $$example..."; \
			cargo clippy --manifest-path "$$example/Cargo.toml" --verbose; \
		fi \
	done

build-examples:
	@set -e; \
	for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			./hack/cargo-smart.sh build --manifest-path "$$example/Cargo.toml" || exit 1; \
		fi \
	done

build-examples-normal:
	@echo "Building examples..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Building $$example..."; \
			cargo build --manifest-path "$$example/Cargo.toml"; \
		fi \
	done

build-examples-verbose:
	@echo "Building examples (verbose)..."
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Building $$example..."; \
			cargo build --manifest-path "$$example/Cargo.toml" --verbose; \
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
	cargo build -p 'universal-tool-*' --all-features --release
	@for example in examples/*/; do \
		if [ -f "$$example/Cargo.toml" ]; then \
			echo "Building release for $$example..."; \
			cargo build --manifest-path "$$example/Cargo.toml" --release; \
		fi \
	done

# Example runners
run-cli-simple:
	cargo run --manifest-path examples/01-cli-simple/Cargo.toml -- $(ARGS)

run-cli-advanced:
	cargo run --manifest-path examples/02-cli-advanced-params/Cargo.toml -- $(ARGS)

run-rest-simple:
	cargo run --manifest-path examples/03-rest-simple/Cargo.toml

run-rest-idiomatic:
	cargo run --manifest-path examples/04-rest-idiomatic/Cargo.toml

run-mcp:
	cargo run --manifest-path examples/05-mcp-basic/Cargo.toml

run-kitchen-sink:
	cargo run --manifest-path examples/06-kitchen-sink/Cargo.toml -- $(ARGS)

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
