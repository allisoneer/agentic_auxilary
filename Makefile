# Root Makefile for building all tools in the monorepo

# Text colors
BLACK := \033[30m
RED := \033[31m
GREEN := \033[32m
YELLOW := \033[33m
BLUE := \033[34m
MAGENTA := \033[35m
CYAN := \033[36m
WHITE := \033[37m
GRAY := \033[90m

# Background colors
BG_BLACK := \033[40m
BG_RED := \033[41m
BG_GREEN := \033[42m
BG_YELLOW := \033[43m
BG_BLUE := \033[44m
BG_MAGENTA := \033[45m
BG_CYAN := \033[46m
BG_WHITE := \033[47m

# Text styles
BOLD := \033[1m
DIM := \033[2m
ITALIC := \033[3m
UNDERLINE := \033[4m

# Reset
NC := \033[0m

CHECK := $(GREEN)✓$(NC)
CROSS := $(RED)✗$(NC)
DASH := $(GRAY)-$(NC)

.PHONY: all build clean help clean-thoughts-tool clean-universal-tool clean-claudecode-rs

# Default target builds all tools
all: help ## Run help (default)

# Help target
help: ## The help command - this command
	@echo ""
	@echo "Purpose of this Makefile:"
	@echo "  To make $(GREEN)build$(NC)"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@grep -h -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "$(CYAN)%-30s$(NC) %s\n", $$1, $$2}'
	@echo ""

# Build all tools
build: build-thoughts-tool build-universal-tool build-claudecode-rs ## Build all tools
	@echo "✓ All tools built successfully"

# Build individual tools
build-thoughts-tool: ## Build thoughts_tool only
	@echo "Building thoughts_tool..."
	@$(MAKE) -C thoughts_tool build

build-universal-tool: ## Build universal_tool only
	@echo "Building universal_tool..."
	@$(MAKE) -C universal_tool build

build-claudecode-rs: ## Build claudecode_rs only
	@echo "Building claudecode_rs..."
	@$(MAKE) -C claudecode_rs build

# Clean all tools
clean: clean-thoughts-tool clean-universal-tool clean-claudecode-rs ## Clean all tools
	@echo "✓ All tools cleaned successfully"

# Clean individual tools
clean-thoughts-tool: ## Clean thoughts_tool only
	@echo "Cleaning thoughts_tool..."
	@$(MAKE) -C thoughts_tool clean

clean-universal-tool: ## Clean universal_tool only
	@echo "Cleaning universal_tool..."
	@$(MAKE) -C universal_tool clean

clean-claudecode-rs: ## Clean claudecode_rs only
	@echo "Cleaning claudecode_rs..."
	@$(MAKE) -C claudecode_rs clean
