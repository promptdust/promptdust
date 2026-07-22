# Privacy Statement & Threat Model

PromptDust is a tool about privacy, so it holds itself to a strict standard.

## The four guarantees

1. **Read-only.** It never creates, modifies, moves, renames, or deletes any file it
   scans. Enforced by a test that hashes a fixture tree before and after a scan and
   asserts they are byte-for-byte identical (INV-1).
2. **Local-only scan.** The scan makes zero network calls — nothing you match or measure
   ever touches the network, and this holds regardless of any other setting. By default
   the app sends **nothing** off your machine at all; the only things it can ever send are
   opt-in and consent-gated (see *What the app can send*, below). Enforced by a CI check
   that fails if any networking crate is linked into the engine, plus a runtime scan with
   the network blocked (INV-2).
3. **Metadata-only.** It records sizes, counts, timestamps, and structural facts (like
   a row count) — never the content of your conversations. This holds for everything the
   app emits — its report, any export, **and any optional diagnostics or telemetry**.
   Enforced by a test that plants a canary string inside fixture files and asserts it
   never appears in any output (INV-3).
4. **Inventory, not a verdict.** It reports what is present and why it might matter. It
   never tells you that you are "safe," "secure," or "clean." Enforced by a lint that
   fails the build on reassurance language (FR-5).

## What the app can send (and the consent rules)

By default, PromptDust **sends nothing** off your machine — nothing is uploaded, no
phone-home. It has three feedback features, and **none of them sends anything without your
explicit action**:

- **Anonymous usage statistics** — **off unless you turn them on** (opt-in, never opt-out).
  Aggregate counts only, tagged with a **per-run random identifier** regenerated every run,
  so runs can't be linked to each other or back to you. The exact payload is documented
  field-by-field in [`docs/TELEMETRY.md`](TELEMETRY.md), and `promptdust telemetry preview`
  prints it verbatim.
- **Crash reports** — if the app crashes it writes a **redacted** report to a *local*
  temporary file and tells you where it is. The file stays on your machine; it is sent only
  if **you** choose to attach it to a bug report. It holds a technical backtrace, your OS,
  and the app version — never your scanned files, their paths, or any conversation content.
  Even writing the local file is **opt-out**: `DO_NOT_TRACK`, the `PROMPTDUST_NO_CRASH_REPORT`
  switch, or CI turns it off.
- **Diagnostics bundle** — you generate it yourself with `promptdust diagnostics`; it is
  never sent for you.

Each is bound by these rules, without exception:

- **You see it first.** Nothing leaves before you see exactly what it is.
- **Never anything sensitive.** No file contents, no scanned-file paths, nothing that
  identifies you or your machine.
- **Always disableable.** Usage statistics and crash reports honor the `DO_NOT_TRACK`
  environment variable and a single off switch, and neither runs in CI.
- **Open and inspectable.** The code lives in this open-source repository, and every payload
  is documented in full.

Identified or managed reporting for organizations is a separate product — not this app.

## What data the tool touches

- It reads the **metadata** of files in a curated set of known AI-tool locations
  (path, size, modification time) and, for some structured stores, a **count** (JSONL
  lines, SQLite rows) obtained without reading content columns.
- It queries **system state** read-only to assess exposure: whether full-disk
  encryption is on (`fdesetup`/`manage-bde`/`lsblk`), whether a path is excluded from
  Time Machine (`tmutil`), and which folders are cloud-synced.
- It runs as **your user account** and never requests administrator/root privileges.

It does **not** read message content and does **not** open or transmit your files. The only
things it writes to disk are: a report when you explicitly export one; if you enable
telemetry, the one-time record of that choice; and, if the app ever crashes, a redacted
crash report in a temporary folder — which stays on your machine unless you choose to share
it.

## What it protects against (threat model)

The concern is the **confidentiality of unexpectedly-persisted local AI data**: AI
tools keep the content of your conversations on disk, usually in plaintext, in places
you never chose and mostly don't know about. Anything with read access to your home
directory — local malware, a borrowed or stolen unlocked laptop, an over-broad backup,
a misconfigured cloud sync — can read it. The tool's job is **discovery and exposure
assessment**: it makes that invisible footprint visible so you can manage it.

## What it deliberately does NOT do

- It is not an antivirus, a data-loss-prevention agent, or a cleaner.
- It does not stop data from being sent *to* AI models (that's a different problem).
- It does not scan phones directly (mobile OS sandboxes prevent it); a future version
  may scan unencrypted phone backups that already sit on your computer.
- It does not remove anything or "fix" anything automatically. Remediation is your
  choice, guided by the suggestions it prints.

## The exported report is sensitive

If you export a report, it contains a map of where sensitive AI data lives on your
machine. Treat that file as sensitive: keep it off shared/synced locations, and delete
it when you're done. The tool warns you about this every time you export.
