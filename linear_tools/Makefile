.PHONY: all check test build clean

all: check test build

check:
	@echo "--- Checking linear_tools ---"
	cargo fmt --all -- --check
	cargo clippy -p linear-schema -p linear-queries -p linear-tools -- -D warnings

test:
	@echo "--- Testing linear_tools ---"
	cargo test -p linear-queries -p linear-tools --all-features

build:
	@echo "--- Building linear_tools ---"
	cargo build -p linear-tools --all-features

clean:
	cargo clean
