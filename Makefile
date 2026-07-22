# promptdust developer task runner (works without `just`).
# Usage: make <target>   e.g.  make test   |   make run-cli ARGS="scan --json"

ARGS ?=

.PHONY: build test lint fmt fmt-check audit schema-check no-network no-telemetry verdict-lint \
        checks run-cli run-gui package coverage clean all hooks

all: fmt-check lint test checks ## fmt-check + lint + test + checks

hooks: ## Install the git commit-message hook (Conventional Commits via Commitizen)
	pre-commit install --hook-type commit-msg

build: ## Build the whole workspace
	cargo build --workspace

test: ## Run unit + integration tests
	cargo test --workspace --all-features

lint: ## Clippy with warnings denied
	cargo clippy --all-targets --all-features -- -D warnings

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting (what CI runs)
	cargo fmt --all --check

audit: ## Dependency license + advisory audit
	cargo deny check

schema-check: ## Validate bundled definition files
	python3 .github/scripts/validate_signatures.py

no-network: ## INV-2: assert no networking crates in core
	bash .github/scripts/check_no_network.sh

no-telemetry: ## INV-5: assert no analytics/telemetry crates in core
	bash .github/scripts/check_no_telemetry.sh

verdict-lint: ## FR-5: assert no verdict/reassurance language
	bash .github/scripts/verdict_lint.sh

gui-audit: ## Assert the desktop app is read/reveal/export-only
	bash .github/scripts/check_gui_readonly.sh

gui-test: ## Run the desktop UI renderer tests (node --test)
	cd desktop/ui && node --test

gui-build: ## Build the desktop (Tauri) crate
	cargo build --manifest-path desktop/src-tauri/Cargo.toml

checks: schema-check no-network no-telemetry verdict-lint gui-audit ## All non-cargo guard checks

run-cli: ## Run the CLI, e.g. make run-cli ARGS="scan --json"
	cargo run --bin promptdust -- $(ARGS)

run-gui: ## Run the desktop app in dev mode (from M5)
	cd desktop && cargo tauri dev

package: ## Build distributable installers (from M6)
	cd desktop && cargo tauri build

coverage: ## Line coverage (requires cargo-llvm-cov)
	cargo llvm-cov --workspace --all-features --summary-only

clean: ## Remove build artifacts
	cargo clean
