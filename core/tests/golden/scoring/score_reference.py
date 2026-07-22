#!/usr/bin/env python3
"""
score_reference.py — reference implementation of scoring_model.yaml.

Purpose: (1) verify the policy is internally consistent, (2) generate the exact
numbers used in SCORING_RUBRIC.md worked examples. NOT the product engine — a
minimal, faithful transcription of the policy so the rubric quotes real figures.
"""
import yaml, math
from functools import reduce

M = yaml.safe_load(open("scoring_model.yaml"))
CLASS_MULT = M["evidence_class_multiplier"]
SENS_W = M["content_factor"]["confirmed_sensitivity_weight"]
HALF = M["recency_decay"]["half_life_days"]
FLOOR = M["recency_decay"]["floor"]
NO_TS = M["recency_decay"]["no_timestamp_default"]
NORM = M["aggregation"]["normalizer"]
PCAP = M["aggregation"]["per_finding_cap"]

def recency(age_days):
    if age_days is None:
        return NO_TS
    return max(FLOOR, 0.5 ** (age_days / HALF))

def content_factor(evidence_class, confirmed_types):
    if evidence_class != "content":
        return 1.0
    if not confirmed_types:
        return M["content_factor"]["potential"]           # 1.0
    return max(SENS_W[t] for t in confirmed_types)

def contribution(base_weight, evidence_class, age_days=None, confirmed_types=None):
    cm = CLASS_MULT[evidence_class]
    rm = recency(age_days)
    cf = content_factor(evidence_class, confirmed_types or [])
    c = base_weight * cm * rm * cf
    return c, dict(class_mult=cm, recency=round(rm, 3), content_factor=cf, contribution=round(c, 3))

def exposure(findings):
    """findings: list of dicts with base_weight, evidence_class, age_days, confirmed_types"""
    ps, detail = [], []
    for f in findings:
        c, comp = contribution(f["base_weight"], f["evidence_class"],
                               f.get("age_days"), f.get("confirmed_types"))
        p = min(PCAP, c / NORM)
        comp["p"] = round(p, 3)
        detail.append((f["name"], comp))
        ps.append(p)
    prod = reduce(lambda a, b: a * (1 - b), ps, 1.0)
    return round(100 * (1 - prod)), detail

def band(score, which):
    for b in M["scores"][which]["bands"]:
        if b["min"] <= score <= b["max"]:
            return b["label"]

def assurance(coverage_ids, evasion_ids, corroborated_findings):
    cov = {c["id"]: c["penalty"] for c in M["coverage_gaps"]}
    eva = {e["id"]: e["penalty"] for e in M["evasion_signals"]}
    cpen = min(M["assurance"]["total_coverage_penalty_cap"], sum(cov[i] for i in coverage_ids))
    epen = min(M["assurance"]["total_evasion_penalty_cap"], sum(eva[i] for i in evasion_ids))
    bonus = min(M["correlation"]["corroboration"]["assurance_bonus_cap"],
                corroborated_findings * M["correlation"]["corroboration"]["assurance_bonus_per_finding"])
    a = max(0, min(100, M["assurance"]["base"] - cpen - epen + bonus))
    return a, dict(coverage=cpen, evasion=epen, corroboration=bonus)

def show(title, findings, coverage, evasion, corroborated):
    exp, detail = exposure(findings)
    asr, abkt = assurance(coverage, evasion, corroborated)
    print(f"\n### {title}")
    for name, comp in detail:
        print(f"  - {name}: contribution={comp['contribution']} (cls×{comp['class_mult']} "
              f"rec×{comp['recency']} content×{comp['content_factor']}) -> p={comp['p']}")
    print(f"  EXPOSURE = {exp} ({band(exp,'exposure')})")
    print(f"  ASSURANCE = {asr} ({band(asr,'assurance')})  [base100 -cov{abkt['coverage']} "
          f"-eva{abkt['evasion']} +corr{abkt['corroboration']}]")
    print(f"  -> {interpret(exp, asr)}")

def interpret(exp, asr):
    hi_e = exp >= 60; hi_a = asr >= 70
    key = f"{'high' if hi_e else 'low'}_exposure__{'high' if hi_a else 'low'}_assurance"
    return M["interpretation_matrix"][key]

# ---------------------------------------------------------------------------
# THREE SCENARIOS (numbers land in the rubric)
# ---------------------------------------------------------------------------
print("Policy loaded OK. schema", M["schema_version"])

# Scenario A — lightly-used machine: AI tools installed, no stored sensitive content
show("A. Lightly-used machine (capability present, no confirmed content)",
     findings=[
        dict(name="Ollama (port+proc seen, non-sensitive runtime)", base_weight=3, evidence_class="usage", age_days=5),
        dict(name="VS Code + Copilot (installed, chat store empty/clean)", base_weight=6, evidence_class="presence", age_days=10),
        dict(name="HF model cache (presence)", base_weight=3, evidence_class="presence", age_days=30),
     ],
     coverage=[], evasion=[], corroborated=1)

# Scenario B — real exposure, well corroborated
show("B. Confirmed live secret in transcripts (recent)",
     findings=[
        dict(name="Claude Code transcripts (content, CONFIRMED secret)", base_weight=8,
             evidence_class="content", age_days=2, confirmed_types=["secret","source_code"]),
        dict(name="Cursor composer chats (content, potential)", base_weight=8,
             evidence_class="content", age_days=7),
        dict(name="ChatGPT desktop (content, CONFIRMED pii)", base_weight=7,
             evidence_class="content", age_days=20, confirmed_types=["pii"]),
        dict(name="API keys in shell history (content, secret)", base_weight=7,
             evidence_class="content", age_days=15, confirmed_types=["secret"]),
     ],
     coverage=[], evasion=[], corroborated=3)

# Scenario C — THE TRAP: computed exposure ~ same as clean scenario A, but evaded/blind.
# Same low exposure as A -> the ONLY thing that separates "clean" from "cleaned-up"
# is the Assurance score. That is the entire argument for two scores.
show("C. Looks as empty as A - but evaded/blind (dual score earns its keep)",
     findings=[
        dict(name="ChatGPT desktop installed, store SEALED (presence; content not readable)", base_weight=7,
             evidence_class="presence", age_days=3),
        dict(name="Windsurf installed, store obfuscated (presence)", base_weight=6,
             evidence_class="presence", age_days=8),
     ],
     coverage=["encrypted_at_rest_store","obfuscated_store_present","encrypted_swap_or_fde","incognito_unobservable"],
     evasion=["shell_history_disabled","app_present_store_absent","history_present_but_empty"],
     corroborated=0)
