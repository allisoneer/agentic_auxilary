.PHONY: all check test build clean fmt help
.PHONY: check-normal test-normal build-normal
.PHONY: check-verbose test-verbose build-verbose
.PHONY: thoughts-check thoughts-test thoughts-build thoughts-all
.PHONY: claude-check claude-test claude-build claude-all
.PHONY: universal-check universal-test universal-build universal-all
.PHONY: fmt-all clean-all status

# Default target
.DEFAULT_GOAL := help

# Tools to build
TOOLS := thoughts_tool claudecode_rs universal_tool

# Colors for output
RED := \033[0;31m
GREEN := \033[0;32m
YELLOW := \033[0;33m
BLUE := \033[0;34m
BOLD := \033[1m
NC := \033[0m

# Main targets - run all tools in parallel
all: check test build
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "✅ All tools passed all checks!"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

check:
	@echo "━━━ Checking all tools ━━━"
	@echo ""
	@failures=0; \
	for tool in $(TOOLS); do \
		echo -e "$(BLUE)▶$(NC) Checking $$tool..."; \
		if $(MAKE) -C $$tool check > /dev/null 2>&1; then \
			echo -e "  $(GREEN)✓$(NC) $$tool: clean"; \
		else \
			echo -e "  $(RED)✗$(NC) $$tool: failed"; \
			echo -e "  Run 'make $$(echo $$tool | cut -d_ -f1)-check' for details"; \
			failures=$$((failures + 1)); \
		fi; \
	done; \
	echo ""; \
	if [ $$failures -gt 0 ]; then \
		echo -e "$(RED)✗ $$failures tool(s) failed checks$(NC)"; \
		exit 1; \
	else \
		echo -e "$(GREEN)✓ All tools passed clippy checks$(NC)"; \
	fi

test:
	@echo "━━━ Testing all tools ━━━"
	@echo ""
	@failures=0; \
	for tool in $(TOOLS); do \
		echo -e "$(BLUE)▶$(NC) Testing $$tool..."; \
		if $(MAKE) -C $$tool test > /dev/null 2>&1; then \
			echo -e "  $(GREEN)✓$(NC) $$tool: tests passed"; \
		else \
			echo -e "  $(RED)✗$(NC) $$tool: tests failed"; \
			echo -e "  Run 'make $$(echo $$tool | cut -d_ -f1)-test' for details"; \
			failures=$$((failures + 1)); \
		fi; \
	done; \
	echo ""; \
	if [ $$failures -gt 0 ]; then \
		echo -e "$(RED)✗ $$failures tool(s) failed tests$(NC)"; \
		exit 1; \
	else \
		echo -e "$(GREEN)✓ All tools passed tests$(NC)"; \
	fi

build:
	@echo "━━━ Building all tools ━━━"
	@echo ""
	@failures=0; \
	for tool in $(TOOLS); do \
		echo -e "$(BLUE)▶$(NC) Building $$tool..."; \
		if $(MAKE) -C $$tool build > /dev/null 2>&1; then \
			echo -e "  $(GREEN)✓$(NC) $$tool: built successfully"; \
		else \
			echo -e "  $(RED)✗$(NC) $$tool: build failed"; \
			echo -e "  Run 'make $$(echo $$tool | cut -d_ -f1)-build' for details"; \
			failures=$$((failures + 1)); \
		fi; \
	done; \
	echo ""; \
	if [ $$failures -gt 0 ]; then \
		echo -e "$(RED)✗ $$failures tool(s) failed to build$(NC)"; \
		exit 1; \
	else \
		echo -e "$(GREEN)✓ All tools built successfully$(NC)"; \
	fi

# Normal output versions
check-normal:
	@for tool in $(TOOLS); do \
		echo "━━━ Checking $$tool ━━━"; \
		$(MAKE) -C $$tool check-normal; \
		echo ""; \
	done

test-normal:
	@for tool in $(TOOLS); do \
		echo "━━━ Testing $$tool ━━━"; \
		$(MAKE) -C $$tool test-normal; \
		echo ""; \
	done

build-normal:
	@for tool in $(TOOLS); do \
		echo "━━━ Building $$tool ━━━"; \
		$(MAKE) -C $$tool build-normal; \
		echo ""; \
	done

# Verbose output versions
check-verbose:
	@for tool in $(TOOLS); do \
		echo "━━━ Checking $$tool (verbose) ━━━"; \
		$(MAKE) -C $$tool check-verbose; \
		echo ""; \
	done

test-verbose:
	@for tool in $(TOOLS); do \
		echo "━━━ Testing $$tool (verbose) ━━━"; \
		$(MAKE) -C $$tool test-verbose; \
		echo ""; \
	done

build-verbose:
	@for tool in $(TOOLS); do \
		echo "━━━ Building $$tool (verbose) ━━━"; \
		$(MAKE) -C $$tool build-verbose; \
		echo ""; \
	done

# Individual tool targets - thoughts_tool
thoughts-check:
	@$(MAKE) -C thoughts_tool check

thoughts-test:
	@$(MAKE) -C thoughts_tool test

thoughts-build:
	@$(MAKE) -C thoughts_tool build

thoughts-all:
	@$(MAKE) -C thoughts_tool all

# Individual tool targets - claudecode_rs
claude-check:
	@$(MAKE) -C claudecode_rs check

claude-test:
	@$(MAKE) -C claudecode_rs test

claude-build:
	@$(MAKE) -C claudecode_rs build

claude-all:
	@$(MAKE) -C claudecode_rs all

# Individual tool targets - universal_tool
universal-check:
	@$(MAKE) -C universal_tool check

universal-test:
	@$(MAKE) -C universal_tool test

universal-build:
	@$(MAKE) -C universal_tool build

universal-all:
	@$(MAKE) -C universal_tool all

# Workspace-wide commands
fmt-all:
	@echo "━━━ Formatting all code ━━━"
	@for tool in $(TOOLS); do \
		echo "Formatting $$tool..."; \
		$(MAKE) -C $$tool fmt; \
	done
	@echo -e "$(GREEN)✓ All code formatted$(NC)"

clean-all:
	@echo "━━━ Cleaning all build artifacts ━━━"
	@for tool in $(TOOLS); do \
		echo "Cleaning $$tool..."; \
		$(MAKE) -C $$tool clean; \
	done
	@echo -e "$(GREEN)✓ All artifacts cleaned$(NC)"

# Status command - show tool versions and status
status:
	@echo "━━━ Tool Status ━━━"
	@echo ""
	@echo -e "$(BOLD)Repository:$(NC) $$(basename $$(pwd))"
	@echo -e "$(BOLD)Branch:$(NC) $$(git branch --show-current 2>/dev/null || echo 'not a git repo')"
	@echo -e "$(BOLD)Rust:$(NC) $$(rustc --version)"
	@echo ""
	@echo -e "$(BOLD)Tools:$(NC)"
	@for tool in $(TOOLS); do \
		if [ "$$tool" = "universal_tool" ]; then \
			if [ -f $$tool/universal-tool-core/Cargo.toml ]; then \
				core_version=$$(grep '^version' $$tool/universal-tool-core/Cargo.toml | head -1 | cut -d'"' -f2); \
				echo "  • universal-tool-core: v$$core_version (library)"; \
			fi; \
			if [ -f $$tool/universal-tool-macros/Cargo.toml ]; then \
				macros_version=$$(grep '^version' $$tool/universal-tool-macros/Cargo.toml | head -1 | cut -d'"' -f2); \
				echo "  • universal-tool-macros: v$$macros_version (proc-macro)"; \
			fi; \
		elif [ -f $$tool/Cargo.toml ]; then \
			version=$$(grep '^version' $$tool/Cargo.toml | head -1 | cut -d'"' -f2); \
			echo "  • $$tool: v$$version"; \
		fi; \
	done
	@echo ""
	@echo "Run 'make help' for available commands"

# Help target
help:
	@echo -e "$(BOLD)Agentic Auxiliary - Monorepo Makefile$(NC)"
	@echo ""
	@echo -e "$(BOLD)Quick Commands:$(NC)"
	@echo "  make all         - Check, test, and build all tools"
	@echo "  make check       - Run clippy on all tools"
	@echo "  make test        - Test all tools"
	@echo "  make build       - Build all tools"
	@echo ""
	@echo -e "$(BOLD)Output Variants:$(NC)"
	@echo "  make check-normal    - Normal output"
	@echo "  make test-verbose    - Verbose output"
	@echo ""
	@echo -e "$(BOLD)Individual Tools:$(NC)"
	@echo "  make thoughts-all    - Build thoughts_tool"
	@echo "  make claude-test     - Test claudecode_rs"
	@echo "  make universal-check - Check universal_tool"
	@echo ""
	@echo -e "$(BOLD)Workspace Commands:$(NC)"
	@echo "  make fmt-all     - Format all code"
	@echo "  make clean-all   - Clean all artifacts"
	@echo "  make status      - Show tool versions"
	@echo ""
	@echo -e "$(BOLD)Tools:$(NC) thoughts_tool, claudecode_rs, universal_tool"