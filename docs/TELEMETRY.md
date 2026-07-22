# Telemetry

PromptDust can send **anonymous usage statistics** to help improve it. This is **opt-in and
off by default**: nothing is ever sent unless you explicitly run `promptdust telemetry
enable`. There is no live backend yet — the client ships with a no-op sender — but this
documents exactly what it *would* send, so the promise is inspectable before it ships.

The single source of truth is the command itself:

```sh
promptdust telemetry preview        # prints the exact bytes that would be sent
```

## Managing it

```sh
promptdust telemetry status         # enabled/disabled + whether the env forces it off
promptdust telemetry enable         # opt in
promptdust telemetry disable        # opt out (the default)
promptdust telemetry preview        # show the exact anonymous payload
```

Your choice is stored in `<config>/promptdust/consent.json` (e.g.
`~/.config/promptdust/consent.json` on Linux, `~/Library/Application Support/promptdust/` on
macOS). It is the only file the telemetry client writes.

## What is sent (the exact payload)

Every field is a count, a version, a coarse enum, or a fresh random id. There are **no file
paths, no filenames, no conversation content, and no persistent identifier** — the payload is
assembled from the same path-scrubbed [`RedactedSummary`](../core/src/redact.rs) that the
diagnostics bundle uses, and its path/content-freedom is guarded by a canary test
(`telemetry/tests/payload_clean.rs`).

```json
{
  "kind": "promptdust-telemetry",
  "tool_version": "0.1.1",
  "run_id": "a1b2c3d4e5f6…",       // 128 random bits, hex — see below
  "os": "macos",                    // std::env::consts::OS (coarse; no OS version)
  "arch": "aarch64",                // std::env::consts::ARCH
  "scan_duration_ms": 7,            // wall-clock scan time
  "feature_flags": ["no_slow"],     // which flags were active — names only, never values
  "summary": {
    "schema_version": 1,
    "definition_db_version": "2026.07.5",
    "mode": "inventory",
    "disk_encryption": "unknown",   // on / off / unknown — a global amplifier input
    "total_findings": 12,
    "total_bytes": 34567,
    "by_tool":      { "Claude Code": 3, "Cursor": 1 },     // tool display names (definition-declared)
    "by_exposure":  { "high": 2, "medium": 6, "low": 4 },  // counts by exposure level
    "by_definition": { "claude-code-transcripts": 3 },      // definition ids — never a matched path
    "warning_count": 0              // count only — a warning's text may hold a path, so it is dropped
  }
}
```

### `run_id`

A fresh 128-bit random id, regenerated **on every run** and never written to disk. It lets a
future backend de-duplicate the events of a single run **without** linking runs to each other
or to a machine. It is not a device id, a user id, or a fingerprint.

## Turning it off, always

Telemetry stays off unless you opt in, and even when enabled it is **forced off** by any of:

- **`DO_NOT_TRACK`** — set to any non-empty, non-`0` value (the [consoledonottrack.com](https://consoledonottrack.com) convention).
- **`PROMPTDUST_TELEMETRY`** — a kill-switch: a falsy value (`0` / `false` / `no` / `off`)
  forces telemetry off. (Note: it can only force *off* — a truthy value does **not** turn
  telemetry on; only `telemetry enable` does.)
- **CI** — the `CI` environment variable being set. Telemetry never runs in automation.

## First run

The first interactive run prints a one-time note to **stderr** telling you telemetry exists
and how to opt in, then records that it has been shown so it never repeats. It is not an
interactive prompt (it never blocks a pipe or reads stdin), and it never appears when the
environment forces telemetry off or when output is not a terminal.

## Guarantees

- **Opt-in.** Off by default; only `telemetry enable` turns it on. (ADR-021, INV-5.)
- **Anonymous.** No paths, no content, no persistent id — canary-guarded.
- **Front-end only.** The engine (`promptdust-core`) carries no telemetry and links no
  analytics or networking crate (INV-5 / INV-2 guards).
- **Local until consent.** The client writes only your consent choice; nothing leaves your
  machine without an active opt-in (and there is no live backend yet).
