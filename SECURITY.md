# Security Policy

`promptdust` reads sensitive locations on a user's machine, so a vulnerability in it is
high-impact. We take disclosure seriously.

## Reporting a vulnerability

Please report privately — do **not** open a public issue for a security problem.

- Preferred: open a GitHub **Security Advisory** ("Report a vulnerability") on this
  repository.
- Alternatively, contact the team via the GitHub organization profile.

Include: affected version, platform, a description, and a minimal reproduction. Please
**redact any real paths or data** in your report.

We aim to acknowledge within a few days and to coordinate a fix and disclosure timeline
with you.

## Scope — what we consider a vulnerability

Because of the tool's design, the following are treated as serious defects:

- Any violation of the **invariants**: a write/modify/delete of a scanned file
  (INV-1), a network call in the scan path (INV-2), conversation content appearing in
  any output/log (INV-3), an unrequested write of sensitive data (INV-4), telemetry that
  is on by default, in the engine, or carrying content or identifying data (INV-5), or a
  privilege-escalation requirement (INV-6).
- A path-handling bug that could cause the tool to read or act outside the intended
  scope.
- Supply-chain issues in dependencies (we run `cargo-deny`/advisories in CI).

## What the tool does *not* do (by design)

By default it transmits nothing off the device, never modifies files, and never reads
conversation contents. Any telemetry is opt-in and off unless you enable it, and never
carries content or anything that identifies you. If you observe behavior contradicting this, that itself is the
vulnerability — please report it.
