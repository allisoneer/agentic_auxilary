SHELL := /bin/bash

.PHONY: all check test build doc fmt clippy clean publish-dry

all: check test build doc

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

check: fmt clippy

test:
	cargo test --all-features

build:
	cargo build --all-features
	cargo build --examples

doc:
	cargo doc --all-features --no-deps

clean:
	cargo clean

publish-dry:
	cargo publish --dry-run
