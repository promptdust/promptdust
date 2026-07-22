#!/usr/bin/env python3
"""Path-drift canary for the PromptDust definition catalog (decision Q-24).

The catalog is knowledge that rots: AI tools move where they store data as they
ship new versions, and the `references` links that justify a definition can 404.
This canary is a *signal*, not a hard gate — it runs on a schedule (see
`.github/workflows/canary.yml`), not on every PR, so it never blocks a merge.

What it checks, using only the Python standard library (no deps, runs anywhere):

  1. Every bundled definition file still parses and re-validates against the schema
     (delegates to validate_signatures.py so the two never drift apart).
  2. Every `references` URL in every definition is reachable. A definitively dead
     link (HTTP 404/410) is a real drift signal and fails the run; transient
     problems (timeouts, 5xx, connection errors) only warn, so the canary is a
     meaningful alarm rather than a flaky one.

Network access here is fine and expected — this is CI tooling, NOT the scan path.
INV-2 ("no network in the scan path") constrains `promptdust-core`, not this script.

Exit code: 0 if no dead links, 1 if any reference is a hard 404/410.
"""

from __future__ import annotations

import json
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SIG_DIR = REPO_ROOT / "core" / "definitions"
SCRIPTS_DIR = Path(__file__).resolve().parent

USER_AGENT = "promptdust-path-drift-canary (+https://github.com/promptdust/promptdust)"
TIMEOUT_SECS = 15
DEAD_STATUSES = {404, 410}


def iter_definition_files() -> list[Path]:
    """Bundled definition files, skipping the `_template.json` example and schema/."""
    return sorted(
        p
        for p in SIG_DIR.glob("*.json")
        if not p.name.startswith("_")
    )


def collect_references() -> list[tuple[str, str, str]]:
    """Return (file, definition_id, url) for every reference URL in the catalog."""
    refs: list[tuple[str, str, str]] = []
    for path in iter_definition_files():
        data = json.loads(path.read_text(encoding="utf-8"))
        sigs = data if isinstance(data, list) else [data]
        for sig in sigs:
            for url in sig.get("references", []) or []:
                refs.append((path.name, sig.get("id", "<no-id>"), url))
    return refs


def check_url(url: str) -> tuple[str, int | None, str]:
    """Probe a URL. Returns (verdict, status, detail) where verdict is one of
    'ok', 'dead', 'warn'. A HEAD is tried first, falling back to GET, because some
    hosts reject HEAD."""
    for method in ("HEAD", "GET"):
        req = urllib.request.Request(
            url, method=method, headers={"User-Agent": USER_AGENT}
        )
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT_SECS) as resp:
                return ("ok", resp.status, "")
        except urllib.error.HTTPError as exc:
            if exc.code in DEAD_STATUSES:
                return ("dead", exc.code, exc.reason)
            if exc.code == 405 and method == "HEAD":
                continue  # method not allowed — retry with GET
            return ("warn", exc.code, exc.reason)  # 403/429/5xx → transient/blocked
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            return ("warn", None, str(exc))
    return ("warn", None, "no response")


def main() -> int:
    # 1. Re-validate the catalog structure (single source of truth is the schema).
    print("== schema re-validation ==")
    sys.stdout.flush()  # keep CI logs in order across the subprocess boundary
    result = subprocess.run(
        [sys.executable, str(SCRIPTS_DIR / "validate_signatures.py")],
        check=False,
    )
    if result.returncode != 0:
        print("CANARY: definition files no longer validate — see above.", file=sys.stderr)
        return 1

    # 2. Reference-link reachability.
    print("\n== reference link check ==")
    refs = collect_references()
    if not refs:
        print("no reference URLs in the catalog; nothing to check.")
        return 0

    dead: list[str] = []
    warned: list[str] = []
    for file_name, sig_id, url in refs:
        verdict, status, detail = check_url(url)
        tag = {"ok": "OK  ", "dead": "DEAD", "warn": "WARN"}[verdict]
        status_str = str(status) if status is not None else "---"
        print(f"  [{tag}] {status_str} {sig_id} :: {url}")
        if verdict == "dead":
            dead.append(f"{file_name} :: {sig_id} :: {url} ({status} {detail})")
        elif verdict == "warn":
            warned.append(f"{file_name} :: {sig_id} :: {url} ({detail})")

    print(
        f"\nchecked {len(refs)} reference(s): "
        f"{len(dead)} dead, {len(warned)} unverifiable, "
        f"{len(refs) - len(dead) - len(warned)} ok."
    )

    if warned:
        print("\nUnverifiable (transient/blocked — not a failure):", file=sys.stderr)
        for w in warned:
            print(f"  - {w}", file=sys.stderr)

    if dead:
        print("\nDEAD reference links (drift signal — please fix):", file=sys.stderr)
        for d in dead:
            print(f"  - {d}", file=sys.stderr)
        return 1

    print("OK: no dead reference links.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
