.PHONY: all check test build clean

all: check test build

check:
	@cargo fmt --check
	@cargo clippy -- -D warnings

test:
	@cargo test

build:
	@cargo build

clean:
	@cargo clean

# Verbose variants
check-verbose:
	cargo fmt --check
	cargo clippy -- -D warnings

test-verbose:
	cargo test -- --nocapture

build-verbose:
	cargo build

# Normal output variants
check-normal: check-verbose
test-normal: test-verbose
build-normal: build-verbose
