.PHONY: build check fmt lint test test-cli test-codex test-inference dist

build:
	cargo build --locked

fmt:
	cargo fmt --check

lint:
	cargo clippy --all-targets --locked -- -D warnings

test:
	cargo test --all-targets --locked

test-cli: build
	sh tests/cli_acceptance.sh target/debug/skillwick
	sh tests/distribution_smoke.sh

test-codex: build
	sh tests/codex_integration.sh target/debug/skillwick codex

test-inference: build
	sh tests/inference_smoke.sh target/debug/skillwick

check: fmt lint test test-cli

dist:
	dist build --artifacts=local --target=aarch64-apple-darwin --target=x86_64-apple-darwin
