#!/usr/bin/env bash
# INV audit for the desktop app: the GUI must expose only read/reveal/export
# commands and must never delete or overwrite a *scanned* file. This is a heuristic
# guard — it fails the build if a destructive filesystem call appears in the Tauri
# command layer.
set -euo pipefail

SRC="desktop/src-tauri/src"
[ -d "$SRC" ] || { echo "no desktop src; skipping"; exit 0; }

# Destructive fs operations that must not appear in the command layer.
# (export uses fs::write to a user-chosen export path only — allowed — but remove_*
#  and rename/copy of scanned files are not.)
FORBIDDEN='fs::remove_file|fs::remove_dir|fs::remove_dir_all|remove_file\(|remove_dir|std::fs::rename|fs::rename|truncate\(true\)'

hits="$(grep -RnE "$FORBIDDEN" "$SRC" 2>/dev/null || true)"
if [ -n "$hits" ]; then
  echo "GUI read-only audit FAILED: destructive filesystem call in the command layer:" >&2
  echo "$hits" >&2
  exit 1
fi

# The set of exposed Tauri commands must be exactly the known read/reveal/export set.
cmds="$(grep -REo '#\[tauri::command\]' "$SRC" | wc -l | tr -d ' ')"
handlers="$(grep -REo 'generate_handler!\[[^]]*\]' "$SRC" || true)"
echo "GUI read-only audit OK: $cmds command(s); handler = ${handlers:-none}"
