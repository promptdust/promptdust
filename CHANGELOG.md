# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). The bundled definition
database has its own CalVer, tracked in `core/definitions/VERSION`.

## [Unreleased]

## [0.3.0] - 2026-07-23

The desktop app is rebuilt around a scan **Inbox** that remembers your runs.

### Added
- **Scan history (the Inbox).** Every scan is saved locally, so you can revisit past runs, keep per-item read state, and pin or flag findings. All on-device, read-only, metadata-only.
- **Three-pane workspace** replacing the single-scan flow: an Inbox rail, a collapsible findings list with attention filters, and a persistent detail pane.
- **Native Share** for a finding's metadata summary (the macOS Share sheet; clipboard elsewhere).
- **Settings › Privacy**: preview the exact anonymous-usage payload before enabling it, and create a redacted, path-free diagnostics bundle to inspect before sharing.
- **Opt-in self-updater.** "Check for updates" downloads, verifies, and installs an update in place, with no full re-download. It only checks when you click, never in the background.

### Changed
- The second score is now labeled **Confidence** (how complete the look was, not a grade), and "places" are now **Traces**.

### Notes
- Builds are still honestly unsigned; macOS and Windows warn on first launch (see the install guide). Code signing is tracked separately and changes nothing about what the tool does.

## [0.2.0] - 2026-07-22

First public, open-source release under Apache-2.0.

### Added
- `promptdust definitions list --json` emits the **public catalog** (`catalog.json`) — the
  data contract for the catalog website: per-definition public fields only, metadata-only,
  with internal scoring/detection inputs (`base_weight`, `inspector*`, epochs, …) omitted.
- Dual **exposure/assurance** score with a plain-English interpretation; every point is
  traceable to its inputs.
- Opt-in, off-by-default anonymous telemetry; opt-out crash reporting; a redacted,
  path-free `diagnostics` bundle for bug reports.
- Homebrew formula and a verifiable release pipeline (SBOM, build provenance, checksums).

### Changed
- Re-licensed the project from MIT to **Apache-2.0** (ADR-009), for the explicit patent
  grant suited to a security tool; documented the open-core open/private boundary in
  `LICENSING.md` and `NOTICE` (ADR-016).
- Renamed the "signature" data records to **"definitions"** throughout: the CLI command
  (`promptdust definitions`), the catalog and scan-output JSON fields, and the
  `PROMPTDUST_DEFINITIONS_DIR` env override.

## [0.1.0] - 2026-07-15

First implemented release: engine + CLI + desktop app, built and tested end to end.
Distribution installers are currently unsigned — a signed 1.0.0 follows once
code-signing certificates are in place.

### Added
- M0 scaffolding: Cargo workspace (`core`, `cli`), strict lints, and the initial
  test harness.
- Definitions database format: JSON schema, a `_template.json`, the first definitions
  (`claude-code`, `cursor`), and a dependency-free validator.
- M1 engine: definition model + bundled/user loader, path resolution (`~`/`$ENV`/glob),
  read-only fault-tolerant walk, and metadata capture.
- M2 engine: metadata-only inspectors (JSONL line count, SQLite row count), amplifier
  detection (cloud_sync, in_git_repo, world_readable, backup_swept, unencrypted_disk,
  large_growth) with a real macOS system probe, and the deterministic exposure model.
- M3 CLI (`promptdust`): `scan` (human table + `--json`), `--only`/`--exclude`/`--path`/
  `--no-slow`/`--large-threshold`/`--output` (with a sensitivity warning),
  `definitions list`/`validate`, and `version`.
- M4 catalog: expanded definitions (VS Code/Copilot, ChatGPT/Claude desktop, Windsurf,
  Continue, Aider, Ollama) across confidence tiers; SQLite inspector hardened for
  WAL/locked databases (immutable-open fallback); streaming large-JSONL counting;
  a catalog test that every `verified` definition matches its own synthesized fixture.
  (Aider is scoped to common code roots to keep the default scan fast; use `--path`
  for projects elsewhere.)
- M5 desktop app (Tauri v2): welcome/consent → scan (off-thread, progress) → grouped
  results with exposure badges, amplifiers, and guidance → reveal-in-file-manager
  (location only) and export (to Downloads, with a sensitivity note). Three read-only
  commands (`run_scan`/`reveal`/`export_report`) and nothing that mutates a scanned
  file. Shared output contract via `promptdust_core::output`. Dependency-light
  plain-ESM UI with a tested pure renderer; GUI read-only audit in CI.
- Refactor: the JSON output document moved into `promptdust_core::output` so the CLI
  and desktop app share one contract definition (core stays clockless).
- M6 packaging: `release` workflow builds desktop installers via `tauri-action`
  (auto-signs/notarizes when secrets are present, unsigned otherwise per Q-14) and
  standalone CLI binaries for macOS/Windows/Linux, attached to a draft release with
  checksums. Verified locally: a 4.3 MB `.dmg` + `.app`
  build (ad-hoc signed; Developer ID signing pending certificates).
- M7 cross-platform: real Linux (LUKS via `lsblk`) and Windows (BitLocker via
  `manage-bde`, OneDrive roots) system probes replacing the Unknown-only fallback;
  Windows/Linux definition paths for Claude Code, Cursor, and VS Code/Copilot;
  forward-slash glob normalization so one pattern matches on all OSes. Unsupported
  checks still degrade to `unknown`, never a false answer. Windows/Linux code is
  run-verified by the CI matrix (macOS-only session compiles the macOS paths).
- M8 hardening & docs: fuzz-lite robustness tests (thousands of adversarial inputs to
  path resolution and the inspectors, asserting no panic), a performance baseline
  (10k-file tree scans in well under a second), coverage raised to ~96% lines with an
  informational coverage CI job, a user guide, and a privacy/threat-model statement.
- CI workflows (fmt/clippy, test matrix, `cargo-deny`, definition validation) and the
  invariant guard scripts (no-network INV-2, verdict-language lint FR-5).
- Repo hygiene: MIT license, contributing guide with a definition-authoring section,
  security policy, issue/PR templates, `.editorconfig`, Dependabot.
- Full project specification under `docs/` (context, requirements, architecture,
  build plan, environment, CI, decisions, definition catalog).

[Unreleased]: https://github.com/promptdust/promptdust/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/promptdust/promptdust/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/promptdust/promptdust/releases/tag/v0.2.0
