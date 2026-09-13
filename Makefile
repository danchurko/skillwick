.PHONY: build check dependency-assurance docs-check fmt hooks lint test test-cli test-codex test-inference test-trust dist

build:
	cargo build --locked

fmt:
	cargo fmt --check

hooks:
	git config core.hooksPath .githooks

lint:
	cargo clippy --all-targets --locked -- -D warnings

test:
	cargo test --all-targets --locked

test-cli: build
	sh tests/cli_acceptance.sh target/debug/skillwick
	sh tests/distribution_smoke.sh target/debug/skillwick

test-codex: build
	sh tests/codex_integration.sh target/debug/skillwick codex

test-inference: build
	sh tests/inference_smoke.sh target/debug/skillwick

docs-check: build
	sh tests/docs_check.sh target/debug/skillwick

test-trust: build
	sh tests/trust_boundary.sh target/debug/skillwick

check: fmt lint test test-cli docs-check test-trust

dependency-assurance:
	./scripts/dependency-assurance.sh

dist:
	dist build --artifacts=local --target=aarch64-apple-darwin --target=x86_64-apple-darwin
