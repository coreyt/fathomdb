#!/usr/bin/env python3
"""LOCOMO loader — converts ``snap-research/locomo`` ``data/locomo10.json`` into
the 0.8.3 gold-query + document schema, as the **second real agentic-memory
source** for the corpus-adequacy analysis (the paired-power proxy) and a
candidate Slice-10 supplement to the LongMemEval re-pin on the underpowered
``multi_session`` / ``temporal`` classes.

**LICENSE — EVAL-ONLY, NON-COMMERCIAL, DO NOT COMMIT.** LOCOMO is
CC-BY-**NC** 4.0 (Maharana et al. 2024, ACL ``arXiv:2402.17753``). It lives
gitignored under ``data/corpus-data/`` (see ``data/corpus-data/raw/
locomo10.LICENSE.txt``) and is reproduced on demand by
``tests/corpus/scripts/acquire_locomo.py``. It is never committed, never shipped
in the library, and used only as an offline measurement corpus — the same
EVAL-ONLY footprint posture as the priced answerer.

**Class mapping** (LOCOMO ``category`` → FathomDB reporting class, mirroring
:data:`eval.decision_rule_083.MEMORY_CLASSES`):

============  ===================  ===========================================
 category      reporting class      note
============  ===================  ===========================================
 4 single     ``factoid``          single-hop fact recall
 2 temporal   ``temporal``         temporal reasoning
 1 multi-hop  ``multi_session``    **caveat**: LOCOMO multi-hop does NOT
                                   guarantee evidence spanning ≥2 distinct
                                   sessions; apply the same ≥2-session predicate
                                   as the LME re-pin (codex Slice-5 [P1#1])
                                   before any *gate* use.
 3 open-dom.  (excluded)           world-knowledge, not a memory class
 5 adversar.  (excluded)           abstention/negative (``answer`` is null)
============  ===================  ===========================================

``knowledge_update`` has **no LOCOMO analog** — it stays LongMemEval-only.

**Documents** are SESSION-level: ``doc_id = f"{sample_id}:session_{n}"`` with the
body = the session's turns joined ``"Speaker: text"``. An evidence ``dia_id``
``"D{n}:{k}"`` resolves to the session doc ``f"{sample_id}:session_{n}"`` (the
strict-recall join key).

This module is pure stdlib (no ``fathomdb`` / ``datasets`` import) so it runs
without the native build. Deterministic: stable id assignment in input order.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

#: LOCOMO ``category`` int → FathomDB reporting class. Only the positive memory
#: classes that LOCOMO actually covers; 3 (open-domain) and 5 (adversarial) are
#: deliberately absent (excluded from the four-class gate).
LOCOMO_CLASS_MAP: dict[int, str] = {
    4: "factoid",
    2: "temporal",
    1: "multi_session",
}

_DEFAULT_PATH = "data/corpus-data/raw/locomo10.json"
_DIA_RE = re.compile(r"^D(\d+):\d+$")


def session_doc_id(sample_id: str, session_n: int | str) -> str:
    """The corpus doc id for a LOCOMO session (the strict-recall join key)."""
    return f"{sample_id}:session_{session_n}"


def _session_body(turns: list[dict[str, Any]]) -> str:
    """Render a session's turns as ``"Speaker: text"`` lines (FTS-friendly)."""
    lines: list[str] = []
    for t in turns:
        speaker = str(t.get("speaker", "")).strip()
        text = str(t.get("text", "")).strip()
        if text:
            lines.append(f"{speaker}: {text}" if speaker else text)
    return "\n".join(lines)


def _evidence_doc_ids(sample_id: str, evidence: Any) -> list[str]:
    """Resolve a LOCOMO ``evidence`` list of ``dia_id`` strings to *session*
    doc ids, de-duplicated, preserving first-seen order. A non-``"D{n}:{k}"``
    entry (some rows carry malformed/empty evidence) is skipped."""
    out: list[str] = []
    seen: set[str] = set()
    for ev in evidence or []:
        m = _DIA_RE.match(str(ev).strip())
        if not m:
            continue
        did = session_doc_id(sample_id, m.group(1))
        if did not in seen:
            seen.add(did)
            out.append(did)
    return out


def load_locomo(
    path: str | Path = _DEFAULT_PATH,
    *,
    classes: tuple[str, ...] | None = None,
) -> tuple[dict[str, str], list[dict[str, Any]]]:
    """Return ``(documents, gold_queries)`` from a LOCOMO ``locomo10.json``.

    * ``documents`` — ``{session_doc_id: body}`` over every session of every
      conversation (the full retrieval haystack).
    * ``gold_queries`` — gold-file query dicts (the ``gold_repin`` schema:
      ``query_id`` / ``query`` / ``query_class`` / ``required_evidence`` /
      ``answers`` / ``_provenance`` / ``_source``), restricted to the mapped
      positive memory classes, with evidence resolvable to ≥1 corpus session and
      a non-empty answer. ``classes`` (default :data:`LOCOMO_CLASS_MAP` values)
      filters which reporting classes are emitted.
    """
    keep = set(classes) if classes is not None else set(LOCOMO_CLASS_MAP.values())
    raw = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(raw, list):
        raise ValueError(f"LOCOMO file {path!s} is not a top-level list of conversations")

    documents: dict[str, str] = {}
    gold_queries: list[dict[str, Any]] = []
    counter = 0

    for conv in raw:
        sample_id = str(conv.get("sample_id") or f"conv-{len(documents)}")
        conversation = conv.get("conversation") or {}
        for key, turns in conversation.items():
            m = re.match(r"^session_(\d+)$", key)
            if not m or not isinstance(turns, list):
                continue  # skip session_<n>_date_time and non-session keys
            documents[session_doc_id(sample_id, m.group(1))] = _session_body(turns)

        for q in conv.get("qa", []):
            cls = LOCOMO_CLASS_MAP.get(q.get("category"))
            if cls is None or cls not in keep:
                continue
            ev_ids = [d for d in _evidence_doc_ids(sample_id, q.get("evidence")) if d in documents]
            answer = q.get("answer")
            ans = [str(answer)] if answer is not None and str(answer).strip() else []
            qtext = str(q.get("question", "")).strip()
            if not qtext or not ev_ids or not ans:
                continue  # positive-class gold requires evidence + an answer
            counter += 1
            gold_queries.append(
                {
                    "query_id": f"locomo-{counter:04d}",
                    "query": qtext,
                    "query_class": cls,
                    "required_evidence": [{"doc_id": d} for d in ev_ids],
                    "answers": ans,
                    "_provenance": "locomo-real",
                    "_source": "locomo",
                    "_locomo_category": q.get("category"),
                }
            )

    return documents, gold_queries


def corpus_hash(documents: dict[str, str]) -> str:
    """Deterministic sha256 over the sorted ``doc_id\\nbody`` corpus (mirrors
    :func:`eval.gold_repin.corpus_hash`)."""
    h = hashlib.sha256()
    for did in sorted(documents):
        h.update(did.encode("utf-8"))
        h.update(b"\n")
        h.update(documents[did].encode("utf-8"))
        h.update(b"\n")
    return h.hexdigest()


def build_manifest(
    documents: dict[str, str], gold_queries: list[dict[str, Any]]
) -> dict[str, Any]:
    """A provenance + per-class-count manifest (for the corpus-adequacy note)."""
    by_class: dict[str, int] = defaultdict(int)
    for q in gold_queries:
        by_class[str(q["query_class"])] += 1
    return {
        "schema": "0.8.3-locomo-manifest-v1",
        "generated_by": "src/python/eval/locomo_loader.py",
        "source": "snap-research/locomo data/locomo10.json",
        "license": "CC-BY-NC-4.0 (Maharana et al. 2024) — EVAL-ONLY, not committed",
        "corpus_hash": corpus_hash(documents),
        "n_sessions": len(documents),
        "n_conversations": len({d.split(":session_")[0] for d in documents}),
        "per_class_gold_counts": dict(sorted(by_class.items())),
        "class_map": {str(k): v for k, v in LOCOMO_CLASS_MAP.items()},
        "excluded_categories": {"3": "open-domain (not a memory class)", "5": "adversarial (negative)"},
        "caveat_multi_session": (
            "LOCOMO multi-hop (cat 1) does not guarantee evidence spanning >=2 "
            "distinct sessions; apply the LME re-pin >=2-session predicate "
            "(codex Slice-5 [P1#1]) before gate use."
        ),
    }


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description="LOCOMO → 0.8.3 gold/document loader + manifest")
    p.add_argument("--path", default=_DEFAULT_PATH)
    p.add_argument("--out-gold", default=None, help="optional: write gold queries JSON")
    p.add_argument("--out-manifest", default=None, help="optional: write manifest JSON")
    a = p.parse_args(argv)

    documents, gold = load_locomo(a.path)
    manifest = build_manifest(documents, gold)
    print(json.dumps(manifest, indent=2), file=sys.stderr)

    if a.out_gold:
        Path(a.out_gold).parent.mkdir(parents=True, exist_ok=True)
        Path(a.out_gold).write_text(
            json.dumps(
                {
                    "version": "0.8.3-locomo-v1",
                    "corpus_hash": manifest["corpus_hash"],
                    "source": manifest["source"],
                    "license": manifest["license"],
                    "queries": gold,
                },
                indent=2,
            ),
            encoding="utf-8",
        )
    if a.out_manifest:
        Path(a.out_manifest).parent.mkdir(parents=True, exist_ok=True)
        Path(a.out_manifest).write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
