# Contributing to PromptDust

PromptDust is **open source** (Apache-2.0): you can read every line, audit exactly what
it does on your machine, and fork it. It is developed in-house by the PromptDust team, and to keep
the project (and the paid layers built on top of it) under one CLA-free copyright,
**outside code changes to the engine or UI are not accepted.**

There is one thing you *can* contribute, and it is the most valuable one: **coverage** —
telling the team about an AI tool PromptDust does not map yet, or a detail it gets
wrong. That is *data, not code*, and it is very welcome. Bug reports are welcome too.

## The golden rule

**Never include real conversation data, real secrets, or real personal data** in an
issue. When pasting tool output, redact paths and values. All fixtures are synthetic.

## Report a tool PromptDust should map (the most useful contribution)

PromptDust knows where an AI tool stores data from small declarative records (its
"coverage database"). You do not write one — you **describe the tool in an issue** and the
team adds it. Open a **New definition (add a tool)** issue with:

| Field | Notes |
|-------|-------|
| Tool name / vendor | display name |
| Operating system(s) | macOS / Linux / Windows |
| Storage path(s) | the *location* (globs, `~`, `$ENV` allowed), never the contents |
| Category | transcript / cache / embedding_index / config_with_secrets / log / attachment |
| Format | jsonl / sqlite / leveldb / plaintext / plist / json / binary / dir |
| Why it matters | one sentence |
| How you confirmed it | real install (with app version + date) / documented / reported |

### How to *safely* find where a tool stores data

- Do it **read-only**. Use `ls`, `find`, `du`, or a SQLite viewer opened read-only —
  never edit or delete.
- Look under the OS app-data dirs: macOS `~/Library/Application Support/<app>/`,
  Linux `~/.config/<app>/` or `~/.<app>/`, Windows `%APPDATA%\<app>\`.
- Note the *structure*, never the contents.

### Confidence tiers

Coverage entries are labelled by how well the location is confirmed:

- **verified** — confirmed on a real install (note the app version + date).
- **likely** — documented or strongly implied, but not personally confirmed.
- **unverified** — reported; the tool labels these in its output.

## Found a bug?

Open a **Bug report** issue with what happened, what you expected, and steps to reproduce.
Running `promptdust diagnostics` gives a redacted, path-free bundle that is safe to attach.
The team takes it from there.

## Security issues

Please report vulnerabilities **privately** — see [`SECURITY.md`](SECURITY.md) — not as a
public issue.
