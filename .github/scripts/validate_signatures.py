#!/usr/bin/env python3
"""Validate bundled promptdust definition files against the definition schema.

Pure standard-library (no `jsonschema` dependency) so it runs anywhere Python 3
runs, on every CI OS, with zero setup. It reads the enum/required constraints from
`core/definitions/schema/definition.schema.json` (single source of truth) and applies
the subset of JSON-Schema checks we care about:

  * required top-level fields present
  * no unknown top-level fields (catches typos — additionalProperties:false)
  * enum membership (schema-driven: every scalar and array-of-enum field; paths below)
  * id kebab-case pattern; schema_version const
  * each path entry well-formed
  * global: no duplicate `id` across all files

Files whose name starts with `_` (e.g. `_template.json`) are treated as examples
and skipped. The `schema/` directory is skipped.

Exit code 0 on success, 1 on any violation.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SIG_DIR = REPO_ROOT / "core" / "definitions"
SCHEMA_PATH = SIG_DIR / "schema" / "definition.schema.json"


def load_schema() -> dict:
    with SCHEMA_PATH.open(encoding="utf-8") as fh:
        return json.load(fh)


def enum_of(schema: dict, *path: str) -> list:
    node = schema["properties"]
    for i, key in enumerate(path):
        node = node[key]
        if i < len(path) - 1:
            node = node.get("properties", node)
    return node["enum"]


def validate_definition(sig: dict, schema: dict, errors: list[str], where: str) -> None:
    props = schema["properties"]
    required = schema["required"]
    allowed = set(props.keys())

    for field in required:
        if field not in sig:
            errors.append(f"{where}: missing required field '{field}'")

    for key in sig:
        if key not in allowed:
            errors.append(f"{where}: unknown field '{key}' (typo?)")

    if "schema_version" in sig and sig["schema_version"] != props["schema_version"]["const"]:
        errors.append(f"{where}: schema_version must be {props['schema_version']['const']}")

    if "id" in sig and not re.fullmatch(props["id"]["pattern"], str(sig["id"])):
        errors.append(f"{where}: id '{sig['id']}' is not kebab-case")

    # Enum membership — schema-driven: every top-level property that declares a scalar
    # `enum` (category, format, sensitivity, confidence, max_evidence_class, volatility)
    # or an array-of-enum (platforms, sensitivity_types) is checked, so a new enum field
    # needs no code change here. (paths.match is validated in the paths block below.)
    for field, spec in props.items():
        if field not in sig or not isinstance(spec, dict):
            continue
        if "enum" in spec:
            if sig[field] not in spec["enum"]:
                errors.append(f"{where}: {field} '{sig[field]}' not in {spec['enum']}")
        elif spec.get("type") == "array" and "enum" in spec.get("items", {}):
            item_enum = spec["items"]["enum"]
            values = sig[field] if isinstance(sig[field], list) else []
            for v in values:
                if v not in item_enum:
                    errors.append(f"{where}: {field} item '{v}' not in {item_enum}")

    path_item = props["paths"]["items"]
    match_enum = path_item["properties"]["match"]["enum"]
    os_enum = path_item["properties"]["os"]["enum"]
    for i, path in enumerate(sig.get("paths", [])):
        pw = f"{where}: paths[{i}]"
        if not isinstance(path, dict):
            errors.append(f"{pw}: not an object")
            continue
        for req in path_item["required"]:
            if req not in path:
                errors.append(f"{pw}: missing '{req}'")
        for key in path:
            if key not in path_item["properties"]:
                errors.append(f"{pw}: unknown field '{key}'")
        if "match" in path and path["match"] not in match_enum:
            errors.append(f"{pw}: match '{path['match']}' not in {match_enum}")
        if "os" in path and path["os"] not in os_enum:
            errors.append(f"{pw}: os '{path['os']}' not in {os_enum}")
        if "pattern" in path and not str(path.get("pattern", "")).strip():
            errors.append(f"{pw}: empty pattern")


def main() -> int:
    schema = load_schema()
    errors: list[str] = []
    seen_ids: dict[str, str] = {}
    files = sorted(
        f
        for f in SIG_DIR.glob("*.json")
        if not f.name.startswith("_")
    )
    if not files:
        print("no definition files found", file=sys.stderr)
        return 1

    for path in files:
        try:
            with path.open(encoding="utf-8") as fh:
                data = json.load(fh)
        except json.JSONDecodeError as exc:
            errors.append(f"{path.name}: invalid JSON: {exc}")
            continue
        sigs = data if isinstance(data, list) else [data]
        for idx, sig in enumerate(sigs):
            where = f"{path.name}[{idx}]"
            if not isinstance(sig, dict):
                errors.append(f"{where}: definition is not an object")
                continue
            validate_definition(sig, schema, errors, where)
            sid = sig.get("id")
            if isinstance(sid, str):
                if sid in seen_ids:
                    errors.append(
                        f"{where}: duplicate id '{sid}' (also in {seen_ids[sid]})"
                    )
                else:
                    seen_ids[sid] = where

    if errors:
        print("Definition validation FAILED:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(f"OK: {len(files)} file(s), {len(seen_ids)} definition(s) valid.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
