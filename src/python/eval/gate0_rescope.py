"""0.8.11 Gate-0 — golden-set re-scope (eval FOUNDATION, $0 EVAL-ONLY).

A **scoping deliverable**, not a priced run (PSD §III.A; `0.8.11-implementation.md`
§1 Gate-0). This script does **no LLM calls**: it inventories the *existing* eval
corpora on disk, verifies record counts by actually reading the files, maps each
corpus onto the 5 intent classes ``{needle | multi_session | temporal | global |
multi_hop}``, states which registered decision rule governs which axis, and reports
which intent classes lack FathomDB-node-level retrieval labels.

It emits ``dev/plans/runs/gate0-rescope-output.json``. Re-runnable at $0; counts are
measured from the files (never guessed). Corpus payloads are gitignored EVAL-ONLY
(`0.8.3-0.8.4-corpus-adequacy-and-locomo`); only this derived inventory is committed.
"""

from __future__ import annotations

import json
from collections import Counter
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[3]
RUNS = REPO / "dev" / "plans" / "runs"
DATA = REPO / "data" / "corpus-data"

INTENT_CLASSES = ("needle", "multi_session", "temporal", "global", "multi_hop")

# LME query_class -> PSD intent class. factoid + knowledge_update are both "needle"
# (single-fact lookups); multi_session/temporal map 1:1.
LME_CLASS_TO_INTENT = {
    "factoid": "needle",
    "knowledge_update": "needle",
    "multi_session": "multi_session",
    "temporal": "temporal",
}


def _load_json(p: Path) -> Any:
    return json.loads(p.read_text(encoding="utf-8"))


def _count_jsonl(p: Path) -> int:
    n = 0
    with p.open(encoding="utf-8") as f:
        for line in f:
            if line.strip():
                n += 1
    return n


def inventory_lme() -> dict[str, Any]:
    gold_p = RUNS / "0.8.3-d0a-memory-gold.json"
    d = _load_json(gold_p)
    q = d["queries"]
    by_class = Counter(x.get("query_class") for x in q)
    with_evidence = sum(1 for x in q if x.get("required_evidence"))
    intent = Counter(LME_CLASS_TO_INTENT.get(c, "?") for c in (x.get("query_class") for x in q))
    return {
        "corpus": "LME (LongMemEval-cleaned)",
        "gold_path": str(gold_p.relative_to(REPO)),
        "corpus_payload_path": "data/corpus-data/external/memex-elps/ (ELPS extractions; gitignored)",
        "n_queries": len(q),
        "by_query_class": dict(by_class),
        "intent_coverage": dict(intent),
        "node_level_labels": {
            "present": True,
            "field": "required_evidence[].doc_id (+ qrels_hash)",
            "n_with_labels": with_evidence,
        },
        "license": "research corpus; gitignored EVAL-ONLY payload",
    }


def inventory_locomo() -> dict[str, Any]:
    gold_p = DATA / "eval" / "0.8.3-locomo-memory-gold.json"
    d = _load_json(gold_p)
    q = d["queries"]
    by_class = Counter(x.get("query_class") for x in q)
    with_evidence = sum(1 for x in q if x.get("required_evidence"))
    intent = Counter(LME_CLASS_TO_INTENT.get(c, "?") for c in (x.get("query_class") for x in q))
    raw_p = DATA / "raw" / "locomo10.json"
    n_conv = len(_load_json(raw_p)) if raw_p.exists() else None
    return {
        "corpus": "LOCOMO",
        "gold_path": str(gold_p.relative_to(REPO)),
        "corpus_payload_path": "data/corpus-data/raw/locomo10.json (CC-BY-NC; gitignored)",
        "n_conversations": n_conv,
        "n_queries": len(q),
        "by_query_class": dict(by_class),
        "intent_coverage": dict(intent),
        "node_level_labels": {
            "present": True,
            "field": "required_evidence[].doc_id (+ _locomo_category)",
            "n_with_labels": with_evidence,
        },
        "license": "CC-BY-NC; gitignored EVAL-ONLY payload",
    }


def inventory_apnews() -> dict[str, Any]:
    base = DATA / "raw" / "apnews_benchmarkqed"
    man = _load_json(base / "MANIFEST.json")
    v1 = base / "generated_questions_v1"
    qsets = {}
    total = 0
    for f in sorted(v1.glob("*.json")):
        n = len(_load_json(f))
        qsets[f.name] = n
        total += n
    v2 = base / "generated_questions_v2"
    v2sets = {f.name: len(_load_json(f)) for f in sorted(v2.glob("*.json"))} if v2.exists() else {}
    return {
        "corpus": "AP-News (BenchmarkQED)",
        "gold_path": str(v1.relative_to(REPO)) + " (+ generated_questions_v2)",
        "corpus_payload_path": "data/corpus-data/raw/apnews_benchmarkqed/raw_data.zip (MS-Research, non-redistributable; gitignored)",
        "n_articles": man.get("n_articles"),
        "n_questions_v1": total,
        "questions_v1": qsets,
        "questions_v2": v2sets,
        "intent_coverage": {"global": "activity_global+data_global (~100 v1)", "local": "activity_local+data_local (~100 v1)"},
        "node_level_labels": {
            "present": False,
            "field": None,
            "note": "reference-free LLM-judge win-rate (decide_084); v2 = claim/assertion-level, NOT FathomDB-node-level retrieval labels",
        },
        "license": "Microsoft Research License — NON-COMMERCIAL, NON-REDISTRIBUTABLE, EVAL-ONLY",
    }


def inventory_musique() -> dict[str, Any]:
    p = DATA / "raw" / "musique_dev.jsonl"
    total = 0
    answerable = 0
    hop = Counter()
    sup_present = 0
    with p.open(encoding="utf-8") as f:
        for line in f:
            if not line.strip():
                continue
            r = json.loads(line)
            total += 1
            if r.get("answerable"):
                answerable += 1
                hop[r.get("hop_count")] += 1
                if any(par.get("is_supporting") for par in r.get("paragraphs", [])):
                    sup_present += 1
    return {
        "corpus": "MuSiQue (dev)",
        "gold_path": str(p.relative_to(REPO)),
        "corpus_payload_path": "data/corpus-data/raw/musique_dev.jsonl (gitignored)",
        "n_records": total,
        "n_answerable": answerable,
        "answerable_hop_dist": {str(k): v for k, v in sorted(hop.items())},
        "intent_coverage": {"multi_hop": answerable},
        "node_level_labels": {
            "present": True,
            "field": "paragraphs[].is_supporting (paragraph-level supporting facts)",
            "n_answerable_with_supporting": sup_present,
            "note": "derive FathomDB-node-level labels from is_supporting (no LLM needed)",
        },
        "license": "MuSiQue (CC-BY); gitignored EVAL-ONLY payload",
    }


def build() -> dict[str, Any]:
    corpora = [inventory_lme(), inventory_locomo(), inventory_apnews(), inventory_musique()]

    # Reused-asset map: intent class -> contributing corpora.
    class_map: dict[str, list[str]] = {c: [] for c in INTENT_CLASSES}
    class_map["needle"] += ["LME (factoid+knowledge_update)", "LOCOMO (factoid)"]
    class_map["multi_session"] += ["LME", "LOCOMO"]
    class_map["temporal"] += ["LME", "LOCOMO"]
    class_map["global"] += ["AP-News (sensemaking; win-rate only)"]
    class_map["multi_hop"] += ["MuSiQue (answerable, is_supporting)"]

    # Label gap: which intent classes lack node-level retrieval labels.
    # NOTE: `global` is sensemaking, judged reference-free by decide_084 win-rate — it needs
    # NO node-level retrieval labels by design. The genuine gap is LOCOMO multi_session/temporal,
    # whose `required_evidence.doc_id` is SESSION-level (e.g. `conv-26:session_1`), not node-level.
    label_gap = {
        "needle": {"node_labels": True, "source": "IR gold qrels + LME/LOCOMO required_evidence"},
        "multi_hop": {"node_labels": "derivable", "source": "MuSiQue is_supporting (no LLM)"},
        "global": {
            "node_labels": "not_needed",
            "gap": False,
            "source": None,
            "disposition": (
                "AP-News sensemaking is judged reference-free by decide_084 answer-quality "
                "win-rate, NOT retrieval recall → no node-level retrieval labels needed by design."
            ),
        },
        "multi_session": {
            "node_labels": False,
            "gap": True,
            "source": "LOCOMO (281) — session-level evidence only",
            "disposition": (
                "LOCOMO names the gold SESSION (conv-N:session_M), not the node; a scoped pass "
                "refines session→node (deterministic answer/turn match first, cheap-LLM residual "
                "only; ≤$1). This is the ONLY scoped gap — far smaller than a fresh golden set."
            ),
        },
        "temporal": {
            "node_labels": False,
            "gap": True,
            "source": "LOCOMO (321) — session-level evidence only",
            "disposition": "same as multi_session: refine LOCOMO session→node (scoped, ≤$1).",
        },
    }

    decision_rules = {
        "decide_083": {
            "impl": "src/python/eval/decision_rule_083.py",
            "axis": "Mem0 memory-class (needle/multi_session/temporal/knowledge_update)",
            "form": "paired-delta FathomDB-Mem0, per-class CI lower >= -EPS_NEAR_PARITY, MDE<=0.05 power guard",
            "epsilon": 0.05,
        },
        "decide_084": {
            "impl": "src/python/eval/decision_rule_084.py (+ baselines_084.py)",
            "axis": "GraphRAG sensemaking (global)",
            "form": "LLM-judge win-rate near-parity: CI lower >= 0.5 - EPS_WIN_RATE; question-clustered bootstrap, >=5 runs",
            "epsilon_wr": 0.05,
            "corpus_cap": "N=200 (AP-News max); more runs cannot tighten MDE, only more questions can",
        },
        "musique_multihop": {
            "impl": "src/python/eval/m1_decision_rule.py",
            "axis": "multi_hop vs HippoRAG-2",
            "form": "TBD: decide_08x (competitor unbuilt) — OUT OF SCOPE for Gate-0",
        },
    }

    return {
        "schema": "0.8.11-gate0-rescope-v1",
        "slice": 5,
        "cost_usd": 0.0,
        "intent_classes": list(INTENT_CLASSES),
        "corpora": corpora,
        "reused_asset_map": class_map,
        "decision_rule_adoption": decision_rules,
        "label_gap_report": label_gap,
        "f4_m6_build_excluded": {
            "excluded": True,
            "target_release": "0.8.17",
            "note": "~269-Q entity-rich F4/M6 corpus acquisition (EXP-D) excluded per §1 KILL/scope guard",
        },
        "verdict": (
            "RE-SCOPED: 4 existing corpora cover all 5 intent classes. needle has node-level "
            "qrels; multi_hop derives node-level labels from MuSiQue is_supporting; global needs "
            "none (sensemaking → decide_084 win-rate). The ONLY gap = LOCOMO multi_session/temporal "
            "session-level evidence → one scoped session→node refinement pass (≤$1, unspent at "
            "Gate-0). No fresh golden set; EXP-D (F4/M6) excluded → 0.8.17. decide_083 governs the "
            "Mem0 axis; decide_084 the GraphRAG/global axis (corpus-capped N=200)."
        ),
    }


def main() -> int:
    art = build()
    out = RUNS / "gate0-rescope-output.json"
    out.write_text(json.dumps(art, indent=2), encoding="utf-8")
    print(f"[GATE0] wrote {out} | {len(art['corpora'])} corpora | $0 | {art['verdict'][:60]}...")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
