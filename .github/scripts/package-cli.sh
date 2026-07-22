#!/usr/bin/env bash
# Build the promptdust CLI and package it for distribution.
#
# For the current OS this produces a Homebrew-shaped archive (where applicable) plus the
# bare binary and a .sha256 sidecar for each:
#   macOS    → promptdust-macos-universal.tar.gz  (arm64 + x86_64 via lipo)  + promptdust-macos
#   Linux    → promptdust-linux.tar.gz             (x86_64)                  + promptdust-linux
#   Windows  → promptdust-windows.exe  (bare .exe only — Windows is not a Homebrew target)
#
# Each .tar.gz contains a single top-level binary named `promptdust`, so a Homebrew formula
# can simply `bin.install "promptdust"`. The bare binary keeps the direct-download asset
# name in docs/INSTALL.md; on macOS that bare asset is the universal binary, so it also
# runs on Intel.
#
# Run per-OS by .github/workflows/release.yml's `cli` job. Kept here — not inline in the
# workflow — so the packaging logic is committed, reviewable, and runnable locally from the
# repo root:
#     bash .github/scripts/package-cli.sh
# A local macOS run needs both apple targets installed; if a target is missing the
# cross-build fails loudly rather than shipping a binary that is not truly universal.
# Idempotent: re-running overwrites its own outputs.
set -euo pipefail

# Operate from the repo root, so the documented invocation works from any CWD and the
# artifacts land where the release job's upload globs expect them.
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

# Write "<sha256>  <file>" to <file>.sha256, portable across coreutils and BSD.
write_sha() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" >"$1.sha256"
  else
    shasum -a 256 "$1" >"$1.sha256"
  fi
}

os="$(uname -s)"
case "$os" in
  Darwin)
    targets=(aarch64-apple-darwin x86_64-apple-darwin)
    # On CI the toolchain action already installs both targets; this makes a local run
    # self-sufficient. Without rustup (e.g. a Homebrew rust) it is skipped and the
    # cross-build below is the loud failure if a target's std is absent.
    if command -v rustup >/dev/null 2>&1; then
      rustup target add "${targets[@]}"
    fi
    for t in "${targets[@]}"; do
      cargo build --release --bin promptdust --target "$t"
    done
    lipo -create -output promptdust \
      "target/${targets[0]}/release/promptdust" \
      "target/${targets[1]}/release/promptdust"
    # Refuse to ship a mislabeled "universal" binary: both arches must be present.
    archs="$(lipo -archs promptdust)"
    if ! grep -qw arm64 <<<"$archs" || ! grep -qw x86_64 <<<"$archs"; then
      echo "package-cli.sh: universal binary is missing an architecture (lipo -archs: '$archs')" >&2
      exit 1
    fi
    tar -czf promptdust-macos-universal.tar.gz promptdust
    cp promptdust promptdust-macos
    write_sha promptdust-macos
    write_sha promptdust-macos-universal.tar.gz
    ;;
  Linux)
    # The Linux archive is x86_64-only; refuse to mislabel a native build on another arch
    # (e.g. an aarch64 Linux runner or dev box) — mirrors the macOS universal check above.
    # The archive is named promptdust-linux.tar.gz (no arch) on purpose: an "x86_64" in the
    # filename makes Homebrew's version scanner read the version as "86.64" instead of the
    # release tag, so a formula would install/upgrade under a bogus version.
    arch="$(uname -m)"
    if [ "$arch" != "x86_64" ]; then
      echo "package-cli.sh: Linux build must be x86_64 (uname -m: '$arch')" >&2
      exit 1
    fi
    cargo build --release --bin promptdust
    tar -czf promptdust-linux.tar.gz -C target/release promptdust
    cp target/release/promptdust promptdust-linux
    write_sha promptdust-linux
    write_sha promptdust-linux.tar.gz
    ;;
  MINGW* | MSYS* | CYGWIN*)
    # Windows is not a Homebrew target, so there is no archive — just the bare .exe and its
    # checksum, matching what the release has always shipped. Handled here (rather than a
    # separate workflow step) so the `cli` job runs one uniform command for every OS.
    cargo build --release --bin promptdust
    cp target/release/promptdust.exe promptdust-windows.exe
    write_sha promptdust-windows.exe
    ;;
  *)
    echo "package-cli.sh: unsupported OS '$os'." >&2
    exit 1
    ;;
esac

echo "package-cli.sh: produced —"
ls -l promptdust-macos promptdust-linux promptdust-windows.exe promptdust-*.tar.gz ./*.sha256 2>/dev/null || true
