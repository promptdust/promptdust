# AGENTS.md

A fast orientation to the PromptDust codebase for contributors and coding agents.
This is the map; the code and its tests are the contract.

## What this is

**PromptDust** — *"the dust your prompts leave behind."* A small, **read-only**
desktop + CLI tool that maps where AI tools (Claude Code, Cursor, ChatGPT/Claude
desktop, Copilot, Ollama, Continue, Aider, Windsurf, …) have quietly stored
conversations, caches, and credentials on the local machine, and reports **what
amplifies the exposure** (cloud sync, backups, git repos, world-readable perms,
unencrypted disk). It is an *inventory / footprint mapper*, not a security scanner,
DLP, secret scanner, or cleaner.

The binary is `promptdust`; crates are `promptdust-core`, `promptdust-cli`,
`promptdust-telemetry`, `promptdust-desktop`.

## The four principles (non-negotiable — enforced by tests & CI guards)

1. **Read-only** — never create, modify, move, or delete a scanned file. (INV-1,
   `core/tests/invariants.rs::scan_does_not_modify_the_filesystem`; GUI guard
   `.github/scripts/check_gui_readonly.sh`.)
2. **Local-only** — zero network calls in the scan path; no networking crate may
   link into `promptdust-core`. (INV-2, `.github/scripts/check_no_network.sh`.)
3. **Inventory, not a verdict** — never claim the user is "safe/secure/clean".
   (FR-5, `.github/scripts/verdict_lint.sh` blocks reassurance phrases.)
4. **Metadata-only (output-side)** — report existence, size, timestamps, counts,
   structural shape. Conversation *content* is never **emitted** into output, logs, or
   exports — reading bytes to derive a count is allowed, emitting content is not (INV-3 /
   ADR-017, the CANARY test in
   `core/tests/invariants.rs::scan_output_never_contains_conversation_content`).

When editing, treat these as hard constraints — a change that trips a guard fails CI.

## Repo layout

```
core/                 promptdust-core — the engine (no I/O beyond read-only FS + probes)
  src/
    lib.rs            crate root; re-exports; SCHEMA_VERSION (=1); version()
    model.rs          Definition data model (deserializes the JSON DB); enums (Category,
                      Format, Sensitivity, Confidence, Platform, MatchKind, PathPattern)
    definitions.rs     load bundled (compiled-in via include_dir!) + user-dir definitions;
                      parse_str(); malformed files warn, never abort (FR-9)
    resolve.rs        expand ~ / $ENV / ${ENV}; has_glob; split_base; forward-slash norm
    scan.rs           orchestration: ScanConfig, scan() -> ScanResult, the FS walk
    inspect.rs        metadata-only inspectors: jsonl_linecount, sqlite_rowcount (INV-3)
    detect.rs         amplifier detection (pure fn + precomputed system facts)
    score.rs          exposure model: ExposureLevel, additive weights, score()
    platform.rs       SystemProbe trait + per-OS probes; home/definitions dir; run() w/ timeout
    report.rs         ScanResult / Finding / ScanWarning / Summary (the core output types)
    output.rs         OutputDocument — the versioned JSON contract (core stays clockless)
  definitions/         the definitions DB (JSON), bundled at compile time
    *.json            claude-code, cursor, coding-tools, vscode-copilot, desktop-apps
    _template.json    authoring template (leading _ ⇒ NOT loaded)
    VERSION           definitions-DB CalVer (currently 2026.07.5)
    schema/definition.schema.json
  tests/              integration: invariants, amplifiers, catalog, reliability, perf(ignored)
cli/                  promptdust-cli — thin front-end over the engine
  src/main.rs         clap CLI: scan / diagnostics / telemetry / definitions list|validate / version
  src/render.rs       human-readable inventory rendering + HTML wrapper
  src/output.rs       builds the JSON contract (supplies timestamp + host to core)
  tests/cli.rs        integration (assert_cmd) against a synthetic PROMPTDUST_HOME
telemetry/            promptdust-telemetry — opt-in anonymous usage-telemetry client (front-end
                      only): consent store, env gate, payload, stubbed no-op sender (no network)
desktop/              Tauri v2 app (EXCLUDED from the default cargo workspace)
  src-tauri/src/lib.rs   thirteen user-initiated commands: run_scan / diagnostics /
                         telemetry_status / telemetry_set_enabled / telemetry_preview / reveal /
                         export_report / save_scan / list_scans / load_scan / mark_scan_read /
                         set_finding_state / share — none ever mutate a scanned file
  ui/                    plain-ESM UI; render.mjs is a tested pure renderer (node --test)
docs/                 user + trust docs (INSTALL, USER-GUIDE, PRIVACY, TELEMETRY)
.github/              CI (ci.yml), release (release.yml), guard scripts, templates
```

## Data flow (one scan)

`ScanConfig` → `scan()`:
1. `definitions::load_bundled()` (+ optional user dir + `extra_definitions`).
2. Build the OS `SystemProbe`; compute cloud roots + disk encryption **once**.
3. For each definition applicable to `Platform::current()`, for each applicable
   path: `resolve::expand` → if glob, `split_base` + `WalkDir` + globset matcher;
   else literal. Fault-tolerant (`NotFound` is silent; other errors → warnings).
4. Per match: `measure` (class/size/count/mtime) → `inspect::inspect` (shape) →
   `detect::detect` (amplifiers) → `score::score` (exposure level) → push `Finding`.
5. `recompute_summary()` → `ScanResult`. A front-end wraps it in `OutputDocument`
   with `generated_at` + `host` (the core never reads the wall clock).

## Amplifiers & exposure model

Amplifiers (`detect.rs`): `cloud_sync`, `in_git_repo`, `world_readable`,
`backup_swept`, `unencrypted_disk`, `large_growth`. Detection is a **pure function**
of the path + precomputed `AmpInputs`, so it's tested with real fixtures, no mocks.

Scoring (`score.rs`) is deterministic and additive — base sensitivity (low=1/med=2/
high=3) + amplifier weights (cloud & git = 2; world-readable & unencrypted-disk = 1;
`backup_swept` & `large_growth` = 0, **informational** — reported but never raise the
level) → `Info<Low<Medium<High<Critical` (High=4, Critical≥5). So **Critical requires
off-machine exposure** (cloud sync or a git working tree). Change weights in that one
place and update the guard tests deliberately; it ranks attention, never pass/fail.

## Platform probes (`platform.rs`)

`SystemProbe` trait, one impl per OS, chosen by `cfg!(target_os)`:
- **macOS**: iCloud/CloudStorage/Dropbox/OneDrive roots; `tmutil isexcluded`
  (backup); `fdesetup status` (FileVault).
- **Linux**: `lsblk` TYPE=crypt heuristic (LUKS); no standard per-file backup ⇒ Unknown.
- **Windows**: `manage-bde -status C:` (BitLocker); `%OneDrive%` env roots.
- Fallback (other OS): everything `Unknown`.

Every probe is read-only, runs external tools with a 5s timeout via `run()`, and
**degrades to `Unknown`/`Tri::Unknown` on any failure — never a false answer**
(AC-4.4). `--no-slow` / `ScanConfig::no_slow` skips the shell-out probes (used by
tests for speed + determinism).

## Definitions (the knowledge base)

A definition is a declarative JSON record (see `model.rs::Definition` and
`definitions/_template.json`). Key fields: `schema_version` (=1), `id` (kebab-case,
unique), `tool`, `platforms`, `paths` (`pattern` supports `~`/`$ENV`/globs +
per-path `os`), `category`, `format`, `sensitivity`, optional `inspector`
(+`inspector_args`), `why` (required, one sentence), `guidance`, `confidence`
(`verified`/`likely`/`unverified`), `references`.

To add coverage: add/extend a file under `core/definitions/*.json` (a file may hold
one object or an array), bump `definitions/VERSION` (CalVer) if it's a DB release,
and run `make schema-check`. `verified`-tier definitions must match a synthesized
fixture — `core/tests/catalog.rs` enforces this. Serde **ignores unknown fields**
on purpose (forward-compat); typos are caught by the CI validator, not by parsing.

**Match at the right altitude:** prefer one finding per *store/project* (a directory)
over one-per-file — matching recursive contents (`**/*`) produced ~1,350 rows for one
tool. E.g. Claude Code matches `~/.claude/projects/*` as a **dir**; the engine
aggregates size/count for free. **Sensitivity rubric:** verbatim
transcripts + secret configs = `high`; derived leveldb/IndexedDB caches = `medium`;
model/derived data = `low`.

## Build / test / run

`make` (or `just`) is the task runner; both mirror each other.

```sh
make build          # cargo build --workspace  (core + cli only; desktop is separate)
make test           # cargo test --workspace --all-features   (71 Rust tests + 1 doctest)
make lint           # clippy --all-targets --all-features -D warnings
make fmt-check      # rustfmt --check (what CI runs)
make checks         # schema-check + no-network + verdict-lint + gui-audit
make coverage       # cargo llvm-cov (needs cargo-llvm-cov); ~96% lines
make run-cli ARGS="scan --json"
make run-gui        # cd desktop && cargo tauri dev   (needs tauri-cli + webkit deps)
make gui-test       # cd desktop/ui && node --test    (7 renderer tests)

# Direct:
cargo run --bin promptdust -- scan
cargo test --manifest-path desktop/src-tauri/Cargo.toml   # desktop crate (not in workspace)
```

The desktop crate is **excluded** from the workspace (`Cargo.toml` `exclude`) so
`cargo build/test --workspace` stays fast and cross-platform (no webkit toolchain).
Build/test it explicitly with `--manifest-path desktop/src-tauri/Cargo.toml`.

Test env overrides (from `platform.rs`): `PROMPTDUST_HOME` points `~` at a fixture
tree; `PROMPTDUST_DEFINITIONS_DIR` isolates the user-definitions dir. CLI integration
tests rely on both so they never touch the real `$HOME`/`~/.config`.

## CLI surface (`promptdust`)

- `scan` (default): `--json`, `--only`/`--exclude` (id or tool, comma-sep),
  `--path <subtree>`, `--no-slow`, `--large-threshold <bytes>`, `--output <.json|.html>`
  (prints a "store it carefully" sensitivity warning to stderr). With `--json`,
  **stdout is pure JSON**; warnings go to stderr.
- `diagnostics` (`--no-slow`): prints a **redacted, path-free** bug-report bundle
  (`DiagnosticsDocument` — counts/versions/OS only, no paths or content) to stdout; a
  "review before sharing" note goes to stderr.
- `telemetry status|enable|disable|preview` — manage **opt-in, off-by-default** anonymous
  usage telemetry (`promptdust-telemetry`). `preview` prints the exact anonymous payload
  (per-run random id, no paths/content). Honors `DO_NOT_TRACK` / `PROMPTDUST_TELEMETRY` / CI.
- `definitions list` / `definitions validate <file>`.
- `version` — tool + definitions-DB version.

## Desktop surface

Thirteen user-initiated commands: `run_scan` (off the UI thread), `diagnostics` (returns the
redacted bug-report bundle, off-thread), `telemetry_status` / `telemetry_set_enabled` /
`telemetry_preview` (read/set the opt-in consent — writes only the consent file — and preview
the anonymous payload), `reveal` (opens the *folder* in the OS file manager, never file
content), `export_report` (writes to `~/Downloads/promptdust-report-<ts>.json`), the local
scan-history store `save_scan` / `list_scans` / `load_scan` / `mark_scan_read` /
`set_finding_state` (persist past runs + per-item read/pin/flag state under the app's own
config dir — ADR-022, INV-4 carve-out), and `share` (native macOS Share sheet). Every write
targets the app's own config/export dirs; there is deliberately **no** command that mutates a
scanned file, and CI audits this. UI logic that turns report data → HTML lives in
`ui/render.mjs` (pure, HTML-escaped, unit-tested); `ui/main.js` only wires events.

## CI / workflows (`.github/workflows/`)

- **`ci.yml`** — *minutes-frugal* (the org runs on a limited Actions budget):
  `push`+`pull_request` both scoped to `main` (one run per PR, no double-run);
  docs-only changes skip via `paths-ignore`. Cheap **Linux** jobs (fmt/clippy,
  `test-linux`, `audit`, `checks`, `no-network-runtime`) run on every push+PR; the
  **expensive legs** (`test-cross` macOS+Windows, `coverage`, `desktop`) are gated
  `if: github.event_name != 'push'` → **only on PRs + manual `workflow_dispatch`**,
  never on direct pushes to `main`. Trigger a full matrix with `gh workflow run ci.yml`.
- **`no-network-runtime`** runs the CLI inside `sudo unshare -n` (no-connectivity
  namespace) to prove the scan needs no network (behavioral INV-2 complement).
- **`canary.yml`** — nightly path-drift canary: re-validates the catalog + checks every
  definition `references` URL is live (`.github/scripts/path_drift_canary.py`).
- **`release.yml`** (on `v*` tags) — desktop installers via `tauri-action` (auto-signs
  when secrets present, unsigned otherwise per Q-14) + CLI binaries → draft release.

## Conventions & gotchas

- **`unsafe_code = "forbid"`**, `clippy::all = "deny"` workspace-wide; CI runs
  `-D warnings`. Keep functions `#[must_use]` where the result matters (existing style).
- **Core is clockless & host-agnostic** — never call `now()`/read env for host info
  in `core`; the front-end supplies `generated_at`/`host` via `OutputDocument`.
- **Cross-OS globbing** — match against forward-slash-normalized paths
  (`resolve::to_match_string`, `literal_separator(true)`) so one pattern works on
  Windows too. Don't hardcode `/` assumptions.
- **Non-Unix `world_readable`** returns `None` (ACL detection deferred), not a false
  positive. SQLite inspection opens read-only with an `immutable=1` URI fallback for
  WAL/locked DBs, and degrades to `None` on any error.
- **Never widen the desktop command set** or add destructive fs calls to the Tauri
  layer without expecting the GUI audit to fail.
- **Commit messages must be well-structured — [Conventional Commits](https://www.conventionalcommits.org/),
  enforced by Commitizen** (`.cz.toml` + the `commit-msg` hook — `make hooks` installs it,
  and `--no-verify` is blocked). Format `<type>(<scope>)!: <subject>`; type is one of
  feat/fix/docs/style/refactor/perf/test/build/ci/chore/revert (definition changes are
  `feat(definitions)`, not a `sig:` type). A malformed message is rejected at commit time.
  History is M0–M8. Cargo.lock is committed (workspace ships binaries).
