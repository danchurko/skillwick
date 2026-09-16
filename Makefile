TEST_BINARY = $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)/debug/skillwick

.PHONY: build check dependency-assurance docs-check fmt hooks lint release-preflight test test-benchmark test-cli test-filesystem test-inference test-source-install test-trust dist

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
	sh tests/cli_acceptance.sh "$(TEST_BINARY)"
	python3 tests/setup_contract.py "$(TEST_BINARY)"
	python3 tests/discovery_contract.py "$(TEST_BINARY)"
	python3 tests/invocation_contract.py "$(TEST_BINARY)"
	sh tests/distribution_smoke.sh "$(TEST_BINARY)"

test-benchmark:
	python3 tests/benchmark_contract.py
	python3 tests/local_corpus_contract.py

test-filesystem: build
	sh tests/filesystem_integration.sh "$(TEST_BINARY)"

test-inference: build
	sh tests/inference_smoke.sh "$(TEST_BINARY)"

test-source-install:
	sh scripts/verify-source-install.sh

docs-check: build
	sh tests/docs_check.sh "$(TEST_BINARY)"

test-trust: build
	sh tests/trust_boundary.sh "$(TEST_BINARY)"

check: fmt lint test test-benchmark test-cli docs-check test-trust

dependency-assurance:
	./scripts/dependency-assurance.sh

release-preflight: check test-filesystem test-inference dependency-assurance test-source-install

dist:
	dist build --artifacts=local --target=aarch64-apple-darwin --target=x86_64-apple-darwin --target=aarch64-unknown-linux-musl --target=x86_64-unknown-linux-musl
