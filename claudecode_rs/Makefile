# Default to silent mode
.DEFAULT_GOAL := all

# Silent mode configuration
CARGO_TERM_QUIET := true
CARGO_FLAGS := --quiet
export CARGO_TERM_QUIET

# Platform detection
UNAME := $(shell uname)

# Core targets (silent by default)
.PHONY: all
all: check test build
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "✅ All checks passed successfully!"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

.PHONY: check
check:
	@echo "━━━ Checking claudecode_rs ━━━"
	@echo "Checking code formatting..."
	@./hack/cargo-smart.sh fmt-check
	@echo "Running clippy..."
	@./hack/cargo-smart.sh clippy --all-targets --all-features -- -D warnings

.PHONY: test
test:
	@echo "━━━ Testing claudecode_rs ━━━"
	@./hack/cargo-smart.sh test --all-features

.PHONY: build
build:
	@echo "━━━ Building claudecode_rs ━━━"
	@./hack/cargo-smart.sh build --release --all-features

# Normal output variants
.PHONY: check-normal
check-normal:
	cargo fmt -- --check
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: test-normal
test-normal:
	cargo test --all-features

.PHONY: build-normal
build-normal:
	cargo build --release --all-features

# Verbose output variants
.PHONY: check-verbose
check-verbose:
	cargo fmt -- --check
	cargo clippy --verbose --all-targets --all-features -- -D warnings

.PHONY: test-verbose
test-verbose:
	cargo test --verbose --all-features

.PHONY: build-verbose
build-verbose:
	cargo build --verbose --release --all-features

# Test categories
.PHONY: test-unit
test-unit:
	@./hack/run-silent.sh cargo test $(CARGO_FLAGS) --lib

.PHONY: test-integration
test-integration:
	@./hack/run-silent.sh cargo test $(CARGO_FLAGS) --test '*'

.PHONY: test-ignored
test-ignored:
	@./hack/run-silent.sh cargo test $(CARGO_FLAGS) -- --ignored

.PHONY: test-all
test-all: test test-ignored

# Development utilities
.PHONY: fmt
fmt:
	cargo fmt

.PHONY: fmt-check
fmt-check:
	cargo fmt -- --check

.PHONY: doc
doc:
	cargo doc --no-deps --all-features

.PHONY: doc-open
doc-open:
	cargo doc --no-deps --all-features --open

.PHONY: audit
audit:
	cargo audit

.PHONY: outdated
outdated:
	cargo outdated

.PHONY: clean
clean:
	cargo clean

# Example runners
.PHONY: example-basic
example-basic:
	cargo run --example basic

.PHONY: example-streaming
example-streaming:
	cargo run --example streaming

.PHONY: example-mcp
example-mcp:
	cargo run --example mcp

# Publishing preparation
.PHONY: publish-dry-run
publish-dry-run:
	cargo publish --dry-run

.PHONY: publish-check
publish-check: fmt-check test doc
	cargo publish --dry-run

# Help target
.PHONY: help
help:
	@echo "ClaudeCode-RS Makefile targets:"
	@echo ""
	@echo "  all            - Run check, test, and build (silent)"
	@echo "  check          - Run formatting and clippy checks (silent)"
	@echo "  test           - Run all tests (silent)"
	@echo "  build          - Build release binary (silent)"
	@echo ""
	@echo "  *-normal       - Run with normal output"
	@echo "  *-verbose      - Run with verbose output"
	@echo ""
	@echo "  test-unit      - Run unit tests only"
	@echo "  test-integration - Run integration tests only"
	@echo "  test-ignored   - Run ignored tests"
	@echo "  test-all       - Run all tests including ignored"
	@echo ""
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Check formatting"
	@echo "  doc            - Generate documentation"
	@echo "  doc-open       - Generate and open documentation"
	@echo "  audit          - Run security audit"
	@echo "  outdated       - Check for outdated dependencies"
	@echo "  clean          - Clean build artifacts"
	@echo ""
	@echo "  example-*      - Run specific examples"
	@echo "  publish-check  - Pre-publishing checks"
	@echo "  help           - Show this help message"
