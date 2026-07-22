# PromptDust — User Guide

PromptDust shows you **where AI tools have stored your data on this computer** — the
conversations, caches, and credentials that tools like Claude Code, Cursor, the
ChatGPT/Claude desktop apps, GitHub Copilot, and others keep locally — and **what is
making that data more exposed** than it needs to be.

It is **read-only**: it never changes, moves, or deletes anything. It sends nothing
off your device unless you opt in. And it never reads the actual content of your
conversations — only metadata like file sizes and counts.

## Installing

**Desktop app (recommended for most people).** Download the installer for your OS
from the Releases page and open it.
- *macOS:* open the `.dmg` and drag promptdust to Applications.
- *Windows:* run the installer.
- *Linux:* use the `.AppImage` or `.deb`.

> Until the app is signed with a Developer ID / Authenticode certificate, your OS may
> warn that it's from an unidentified developer. See the project README for the
> current signing status.

**Command line (for developers).** Build from source:
```sh
cargo build --release --bin promptdust
./target/release/promptdust scan
```

## Using the desktop app

1. Open promptdust.
2. Read the one-screen explanation and click **Scan this computer**.
3. Review the results, grouped by tool. Each item shows an exposure level, what's
   amplifying it, and plain-language guidance.
4. Optionally **Export report…** to save a copy (it goes to your Downloads folder).
   The report maps where sensitive data lives, so store it carefully.

Clicking **Reveal** opens the *folder* containing an item in your file manager — it
never opens the file's contents.

The **ⓘ** button (top-right) opens About, where **Check for updates** is available.
PromptDust never checks or downloads updates on its own — only when you click. Any
update is cryptographically verified before it installs. (Installed from Homebrew or
winget instead? Use `brew upgrade` / `winget upgrade`.)

## Using the CLI

```sh
promptdust scan                 # human-readable table
promptdust scan --json          # machine-readable JSON
promptdust scan --only cursor   # just one tool
promptdust scan --path ~/code   # restrict to a subtree
promptdust scan --no-slow       # skip Time Machine / disk-encryption checks
promptdust scan --output report.json   # write a report (it is sensitive)
promptdust diagnostics          # a redacted bundle to paste into a bug report
promptdust telemetry status     # opt-in usage stats: status | enable | disable | preview
promptdust definitions list      # what the tool knows how to find
promptdust version
```

## Understanding the results

**Exposure level** (informational — *not* a security verdict): `info → low → medium →
high → critical`. It combines how sensitive an artifact is with the amplifiers below.
It ranks what to look at first; it never says you are "safe" or "at risk."

**Amplifiers** — what makes an item more exposed:

| Amplifier | Meaning | What you can do |
|-----------|---------|-----------------|
| `cloud_sync` | It's inside iCloud/Dropbox/OneDrive/Google Drive | Move it out of the synced folder |
| `in_git_repo` | It's inside a git working tree | Add it to `.gitignore`; check it wasn't committed |
| `world_readable` | Other local users can read it | Tighten file permissions (`chmod 600`) |
| `backup_swept` | It's included in your system backup | Exclude it if you don't want it in backups |
| `unencrypted_disk` | Full-disk encryption is off | Turn on FileVault / BitLocker / LUKS |
| `large_growth` | The store is unusually large | Use the tool's retention setting or prune it |

Each finding also carries specific **guidance** for that tool (e.g. "set
`cleanupPeriodDays`", "keep `~/.claude` out of synced folders").

## Frequently asked

**Does it send my data anywhere?** Not without your say-so. There is no network activity
during a scan, ever (enforced by tests), and the app sends nothing off your machine by
default. The only things it can send — anonymous usage statistics and crash reports — are
opt-in, off unless you turn them on, and never include your file contents or paths. See
[PRIVACY.md](PRIVACY.md).

**What do the usage statistics contain, if I opt in?** Counts, versions, your OS, and a
fresh random id per run — never file paths or content. You can see the exact payload yourself
with `promptdust telemetry preview`, and it's documented field-by-field in
[TELEMETRY.md](TELEMETRY.md). Manage it with `promptdust telemetry status | enable | disable`
(it stays off until you `enable`, and honors `DO_NOT_TRACK` and CI).

**Does it read my conversations?** No. It reports sizes, counts, and locations —
never the content.

**Will it delete or clean anything?** No. It's an inventory. It only ever reads and
reports; you decide what to act on.

**Is the list complete?** No — it covers a curated set of known tools and will miss
others. It's a starting point, not a guarantee that nothing else is present.

**Why did it flag something in a synced folder as critical?** Because a copy of that
data now lives on the sync provider's servers and every device you sync — a bigger
exposure than a single local file.

See [`PRIVACY.md`](PRIVACY.md) for the full privacy statement and threat model.
