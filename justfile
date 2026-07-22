# promptdust developer task runner (optional; mirrors the Makefile).
# Install `just` with `cargo install just` or `brew install just`.

_default:
    @just --list

# fmt-check + lint + test + guard checks
all: fmt-check lint test checks

build:
    cargo build --workspace

test:
    cargo test --workspace --all-features

lint:
    cargo clippy --all-targets --all-features -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

audit:
    cargo deny check

schema-check:
    python3 .github/scripts/validate_signatures.py

no-network:
    bash .github/scripts/check_no_network.sh

verdict-lint:
    bash .github/scripts/verdict_lint.sh

checks: schema-check no-network verdict-lint

# Run the CLI, e.g. `just run-cli scan --json`
run-cli *ARGS:
    cargo run --bin promptdust -- {{ARGS}}

# Run the desktop app in dev mode (from M5)
run-gui:
    cd desktop && cargo tauri dev

# Build distributable installers (from M6)
package:
    cd desktop && cargo tauri build

# Line coverage (requires cargo-llvm-cov)
coverage:
    cargo llvm-cov --workspace --all-features --summary-only
