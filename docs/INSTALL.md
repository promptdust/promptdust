# Installing promptdust

promptdust ships **honestly unsigned for now** and **verifiable from day one**. Rather
than ask you to trust an installer because an OS vendor blessed it, every release
carries the material you need to check it yourself: per-file SHA-256 checksums, a
consolidated `SHA256SUMS`, a CycloneDX **SBOM**, and cryptographic **build-provenance
attestations** tying each artifact back to the exact GitHub Actions run that produced it.

> Code signing (macOS Developer ID + notarization, Windows Azure Trusted Signing) is
> tracked separately and wires in when the certificates land. It removes the OS warnings
> below — it does not change what the tool does, and **signing gates nothing**: this
> verifiable path ships independently.

Because the builds aren't signed yet, macOS Gatekeeper and Windows SmartScreen will warn
on first launch. That warning means "the OS can't confirm the publisher," **not** that
anything is wrong with the download. Here's how to get past it — and, better, how to
verify the download cryptographically so you don't have to take our word for it.

## Download

Grab the artifact for your platform from the [latest release][releases]:

| Platform | Minimum OS | Desktop app | CLI |
|----------|------------|-------------|-----|
| macOS    | 12 Monterey (Intel or Apple silicon) | `PromptDust_*_universal.dmg` | `promptdust-macos` |
| Windows  | 10 21H2 (x64) | `PromptDust_*_x64-setup.exe` / `*.msi` | `promptdust-windows.exe` |
| Linux    | Ubuntu 22.04+ (glibc 2.35+, x64) | `*.AppImage` / `*.deb` | `promptdust-linux` |

The desktop app uses the system WebView (WebView2 on Windows — preinstalled on Windows 11,
and auto-distributed by Microsoft to current Windows 10; the installer adds it if missing.
WebKitGTK on Linux). The CLI has no such requirement.

Each binary and installer has a matching `<name>.sha256` sidecar, and the release carries
a single `SHA256SUMS` covering all of them. (The SBOM files are checked via their own
provenance attestation — see below.)

## Verify before you run (recommended)

### 1. Checksums

Confirm the bytes you downloaded match what the release published:

```sh
# One file against its sidecar:
shasum -a 256 -c promptdust-macos.sha256

# Or every binary + installer at once, against the consolidated manifest:
shasum -a 256 -c SHA256SUMS       # macOS/Linux
# sha256sum -c SHA256SUMS         # Linux (coreutils)
```

### 2. Build provenance (proves *where* it was built)

Every binary and installer has a SLSA build-provenance attestation. With the
[GitHub CLI][gh] you can confirm an artifact was built by this repository's release
workflow — not re-uploaded or tampered with:

```sh
gh attestation verify promptdust-macos --repo promptdust/promptdust
```

A successful verification prints the workflow and commit the artifact came from.

### 3. SBOM (what's inside)

The release includes CycloneDX SBOMs (`promptdust-*.cdx.json`) enumerating every Rust
dependency in the CLI, core, and desktop crates — feed them to your own SBOM/vuln tooling.

## Get past the first-launch warning

### macOS

The app isn't notarized yet, so double-clicking shows *"can't be opened because Apple
cannot check it for malicious software."* Either:

- **Right-click** (or Control-click) the app → **Open** → **Open** in the dialog. macOS
  remembers the choice for that copy; or
- clear the quarantine flag after you've verified the checksum:

  ```sh
  xattr -dr com.apple.quarantine /Applications/PromptDust.app
  ```

### Windows

SmartScreen shows *"Windows protected your PC."* Click **More info → Run anyway**. If
you verified the checksum and provenance above, you already know more about this build
than SmartScreen does.

### Linux

No signing prompt. Make the AppImage executable, or install the `.deb`:

```sh
chmod +x PromptDust_*.AppImage && ./PromptDust_*.AppImage
# or
sudo dpkg -i promptdust_*_amd64.deb
```

## Package managers (planned)

Homebrew — `brew install promptdust` (the CLI) and `brew install --cask promptdust` (the
desktop app) — and winget (`winget install PromptDust.PromptDust`) are planned to reduce
this friction further. The manifests are authored but
**not published yet** — they go live with the first release. Until then, use the release
artifacts above.

[releases]: https://github.com/promptdust/promptdust/releases
[gh]: https://cli.github.com/
