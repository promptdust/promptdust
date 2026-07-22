#!/usr/bin/env bash
# FR-5 / AC-5.2: promptdust reports an inventory, never a security verdict.
# Fail the build if user-facing text makes a safety/reassurance claim.
set -euo pipefail

# Verdict phrases (not bare words like "secure"/"fail" or the verbs "clear"/"clean", which
# appear legitimately — e.g. "clear the cache"). We block "clean"/"clear" only in a *verdict*
# context (the dual-score interpretation must never read as "credibly clean").
PATTERNS="you are secure|you're secure|you are safe|you're safe|your (machine|computer|device|data) (is|are) safe|no risk|risk-free|fully protected|you are protected|you're protected|you are clean|you're clean|(is|looks|appears|reads|credibly|comes? back) clean\b|all clear|in the clear|nothing to worry|100% (safe|secure)"

roots=(cli/src core/src core/definitions)
[ -d desktop/ui/src ] && roots+=(desktop/ui/src)

hits="$(grep -RniE "$PATTERNS" "${roots[@]}" 2>/dev/null || true)"

if [ -n "$hits" ]; then
  echo "VERDICT-LANGUAGE LINT FAILED: the tool must never claim safety or a verdict." >&2
  echo "$hits" >&2
  exit 1
fi

echo "OK: no verdict/reassurance language found."
