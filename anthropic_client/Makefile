SHELL := /bin/bash

.PHONY: all check test build fmt clippy

all: check test build

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

check: fmt clippy

test:
	cargo test --all-features

build:
	cargo build --all-features
