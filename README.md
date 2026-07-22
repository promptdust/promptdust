# PromptDust

*The dust your prompts leave behind.*

> A small, read-only desktop tool that shows people **where AI tools have quietly
> stored their conversations, caches, and credentials on their own machine — and
> what is amplifying the exposure** (cloud sync, backups, git repos, world-readable
> permissions).

**Status:** Implemented (v0.2.0). The engine, CLI, and desktop app are built and
tested (~96% line coverage on the engine + CLI). Remaining before a signed 1.0.0:
Developer ID / Windows code-signing certificates and a full green run of the CI
matrix on Windows/Linux runners.

## Install

Prebuilt, read-only binaries for macOS, Windows, and Linux are on the
[latest release](https://github.com/promptdust/promptdust/releases/latest). The
builds are honestly unsigned for now and **verifiable from day one**: every
artifact ships with per-file SHA-256 checksums, a consolidated `SHA256SUMS`,
CycloneDX SBOMs, and SLSA build-provenance attestations. See
[`docs/INSTALL.md`](docs/INSTALL.md) for per-OS steps, how to get past the
first-launch OS warning, and [how to verify a download](docs/INSTALL.md#verify-before-you-run-recommended).

| Platform | Desktop app | CLI |
|----------|-------------|-----|
| macOS 12+ (universal) | `PromptDust_*_universal.dmg` | `promptdust-macos` |
| Windows 10+ (x64) | `PromptDust_*_x64-setup.exe` / `.msi` | `promptdust-windows.exe` |
| Linux (Ubuntu 22.04+) | `.AppImage` / `.deb` / `.rpm` | `promptdust-linux` |

The CLI binaries are version-less, so their download links are permanent, e.g.
`releases/latest/download/promptdust-macos`. Homebrew (`brew install promptdust`)
and winget manifests are authored and go live shortly.

### Build from source

```sh
cargo run --bin promptdust -- scan      # CLI, read-only
cd desktop && cargo tauri dev          # desktop app (needs tauri-cli)
```

---

## What this is (in one paragraph)

Modern AI tools — Claude Code, Cursor, the ChatGPT/Claude desktop apps, GitHub
Copilot, Ollama, and many others — persist the *content* of your conversations to
local disk, usually in plaintext, in locations you never chose and mostly do not
know about. This tool makes that invisible footprint **visible**. It is an
**inventory / footprint mapper**, not a "security scanner." It never modifies or
deletes anything, never reads the actual message content, and sends nothing off your
machine unless you opt in. Its job is to answer two questions: *what AI data is sitting on my
machine*, and *what is making it more exposed than it needs to be*. Out of the box it maps
**60+ AI tools and tool families** (definition DB `2026.07.5`) and degrades any check it
cannot run on your OS to `unknown`, never a false answer.

## The four principles (non-negotiable)

1. **Read-only.** The tool never modifies, moves, or deletes a file. Ever.
2. **Local-only.** The scan makes zero network calls — nothing leaves the device during
   a scan. Anything the app ever sends is opt-in, off by default, and never carries
   content or anything that identifies you.
3. **Inventory, not a verdict.** It reports *what is there and why it might matter*.
   It never claims you are "secure" or "clean."
4. **Metadata-only.** It reports existence, size, timestamps, and structural facts.
   It does **not** read, print, or store the contents of your conversations.

## Documentation

| Doc | Purpose |
|-----|---------|
| [`docs/INSTALL.md`](docs/INSTALL.md) | How to install the desktop app and the CLI on each OS. |
| [`docs/USER-GUIDE.md`](docs/USER-GUIDE.md) | How to use the app/CLI and read the results. |
| [`docs/PRIVACY.md`](docs/PRIVACY.md) | The privacy statement and threat model. |
| [`docs/TELEMETRY.md`](docs/TELEMETRY.md) | What the opt-in, off-by-default telemetry does and does not send. |

## Positioning (what it is NOT)

- Not a **secret scanner** (Gitleaks/TruffleHog) — those match secrets inside files.
- Not a **DLP / egress tool** — those stop data going *to* the model.
- Not a **cleaner** — it never deletes.
- Not a **security score** — it never issues a pass/fail verdict.

It occupies the open niche: a local, read-only, cross-tool **AI data footprint
mapper for individuals**, with an exposure-amplifier analysis that no existing tool
provides.

## Contributing

PromptDust is open source so you can read and audit it, but it is developed in-house by
the PromptDust team and **outside code contributions are not accepted**. The one thing you can
contribute is **coverage**: tell it about an AI tool it does not map yet (or a wrong
detail) by opening an issue. Bug reports are welcome too. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

PromptDust is licensed under the [Apache License 2.0](LICENSE) (see also [`NOTICE`](NOTICE)).

The engine — `promptdust-core`, `promptdust-cli`, `promptdust-desktop` — and the bundled
definitions database are open-source under Apache-2.0, permanently. The open/private
boundary (and why future paid layers, PromptDust Pro and an Enterprise tier, stay *outside* the core) is documented in
[`LICENSING.md`](LICENSING.md); the decision is recorded as ADR-009 and ADR-016.
