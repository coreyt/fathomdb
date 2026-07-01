#!/usr/bin/env python3
"""OPP-6 EXP-COV-0 — extraction-coverage census ($0, LLM-free scoring).

Slice 5 of 0.8.12 (Memory-quality plumbing). Pre-registration:
``dev/design/0.8.12-coverage-probe-and-value-test.md`` §A. Authoritative arm design:
Memex ``dev/fathomdb/OPP-6-experiments.md`` (EXP-COV-0, §4 metric, §7 rule).

This module measures **fact-coverage = recall of gold facts** (OPP-6 §4) on the frozen
``personal.gold`` slice, for the ``$0`` arms only:

* **C0-floor** — a deterministic heuristic extractor (capitalized-span entity spotter +
  intra-sentence co-occurrence edges). The low anchor; CPU, no LLM.
* **ELPS-baseline** — the CURRENT ELPS extractor, scored from the **pre-computed**
  ``personal.baseline_outputs.jsonl`` (``claude-haiku-4-5``, prompt ``elps-prompt-2``).
  No new LLM call — the outputs are already on disk, so this is ``$0``.

Metrics (per OPP-6 §4): entity coverage (recall) + edge coverage (recall), each paired
with a **precision guard** (a coverage gain that arrives with collapsing precision is
"garbage edges", not a win — the 0.8.3 blind-merge caution). Everything is also broken
down **per doc ``kind``** (the per-class half of OPP-6 D-3); a class with too few gold
facts is reported as under-powered, no claim (R-COV-2).

The gold + baseline inputs are gitignored EVAL-ONLY (never committed); this module reads
them by path (env-overridable) and emits only derived metrics.

Determinism: pure-Python, no RNG, no network, no ``fathomdb`` import.
"""

from __future__ import annotations

import json
import os
import re
import unicodedata
from collections import defaultdict
from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# --------------------------------------------------------------------------- #
# Input locations (gitignored EVAL-ONLY; overridable for a copied worktree input)
# --------------------------------------------------------------------------- #
_DEFAULT_CORPUS_DIR = os.environ.get(
    "EXP_COV_CORPUS_DIR",
    "/home/coreyt/projects/fathomdb/data/corpus-data/external/memex-elps",
)
GOLD_PATH = Path(_DEFAULT_CORPUS_DIR) / "personal.gold.jsonl"
BASELINE_PATH = Path(_DEFAULT_CORPUS_DIR) / "personal.baseline_outputs.jsonl"

# Below this many gold edges (or entities) a per-class cell is under-powered → no claim.
POWER_MIN = 10


# --------------------------------------------------------------------------- #
# Canonicalization
# --------------------------------------------------------------------------- #
def canon(text: str) -> str:
    """Casefold + strip + accent-fold a surface form for matching (surface-form-robust)."""
    if not text:
        return ""
    t = unicodedata.normalize("NFKD", text)
    t = "".join(ch for ch in t if not unicodedata.combining(ch))
    return re.sub(r"\s+", " ", t).strip().casefold()


def canon_relation(rel: str) -> str:
    """Normalize a relation label (casefold, spaces/hyphens → underscore)."""
    return re.sub(r"[\s\-]+", "_", canon(rel))


# --------------------------------------------------------------------------- #
# Gold / extraction record shapes
# --------------------------------------------------------------------------- #
@dataclass
class Extraction:
    """A normalized set of entities + edges for one document (any arm)."""

    # entity keysets: one frozenset of canon forms (name ∪ aliases) per entity
    entity_keysets: list[frozenset[str]] = field(default_factory=list)
    # edges as canonical triples (canon_from, canon_rel, canon_to)
    edge_triples: set[tuple[str, str, str]] = field(default_factory=set)
    # endpoint pairs only (relation-agnostic), for the heuristic-fair view
    edge_pairs: set[tuple[str, str]] = field(default_factory=set)


def _alias_map(gold_entities: Iterable[dict]) -> dict[str, str]:
    """Map every canon surface form (name or alias) → its canonical entity name (canon)."""
    m: dict[str, str] = {}
    for e in gold_entities:
        name = canon(str(e.get("name", "")))
        if not name:
            continue
        m[name] = name
        for a in e.get("aliases", []) or []:
            ca = canon(str(a))
            if ca:
                m.setdefault(ca, name)
    return m


def _resolve(surface: str, amap: dict[str, str]) -> str:
    """Resolve a mention through the gold alias map (fairer edge-endpoint matching)."""
    c = canon(surface)
    return amap.get(c, c)


def gold_to_extraction(doc: dict) -> Extraction:
    ex = Extraction()
    for e in doc.get("entities", []):
        keys = {canon(str(e.get("name", "")))}
        keys |= {canon(str(a)) for a in (e.get("aliases") or [])}
        keys.discard("")
        if keys:
            ex.entity_keysets.append(frozenset(keys))
    amap = _alias_map(doc.get("entities", []))
    for edge in doc.get("edges", []):
        f = _resolve(str(edge.get("from", "")), amap)
        t = _resolve(str(edge.get("to", "")), amap)
        r = canon_relation(str(edge.get("relation", "")))
        if f and t:
            ex.edge_triples.add((f, r, t))
            ex.edge_pairs.add((f, t))
    return ex


def baseline_to_extraction(output: dict, gold_amap: dict[str, str]) -> Extraction:
    """Normalize one pre-computed ELPS output. Endpoints resolved through the GOLD alias
    map so an extractor alias variant still matches the gold entity (fair to the arm)."""
    ex = Extraction()
    for e in output.get("entities", []):
        keys = {canon(str(e.get("name", "")))}
        keys |= {canon(str(a)) for a in (e.get("aliases") or [])}
        keys.discard("")
        if keys:
            ex.entity_keysets.append(frozenset(keys))
    for edge in output.get("edges", []):
        f = _resolve(str(edge.get("from_entity", edge.get("from", ""))), gold_amap)
        t = _resolve(str(edge.get("to_entity", edge.get("to", ""))), gold_amap)
        r = canon_relation(str(edge.get("relation", "")))
        if f and t:
            ex.edge_triples.add((f, r, t))
            ex.edge_pairs.add((f, t))
    return ex


# --------------------------------------------------------------------------- #
# C0-floor heuristic extractor (deterministic, CPU, no LLM)
# --------------------------------------------------------------------------- #
_CAP_SPAN = re.compile(r"\b[A-Z][a-zA-Z0-9]*(?:\s+[A-Z][a-zA-Z0-9]*)*\b")
_SENT_SPLIT = re.compile(r"(?<=[.!?])\s+|\n+")
# words that are capitalized only because they start a sentence / are calendar noise
_STOP_CAPS = {
    "the", "a", "an", "i", "we", "he", "she", "they", "it", "this", "that", "these",
    "those", "my", "our", "his", "her", "their", "monday", "tuesday", "wednesday",
    "thursday", "friday", "saturday", "sunday", "january", "february", "march",
    "april", "may", "june", "july", "august", "september", "october", "november",
    "december", "today", "tomorrow", "yesterday", "and", "but", "so", "if", "when",
}


def c0_floor_extraction(body: str, gold_amap: dict[str, str]) -> Extraction:
    """Heuristic: capitalized spans as entities; ordered co-occurring pairs in a sentence
    as (from, 'co_occurs', to) edges. Deliberately crude — the low anchor. Endpoints are
    resolved through the gold alias map so it is scored on equal footing with ELPS."""
    ex = Extraction()
    seen_entity_keys: set[str] = set()
    for sent in _SENT_SPLIT.split(body or ""):
        spans: list[str] = []
        for m in _CAP_SPAN.finditer(sent):
            span = m.group(0).strip()
            if not span or canon(span) in _STOP_CAPS:
                continue
            # drop a leading sentence-initial single common word (crude noise filter)
            spans.append(span)
        # de-dup within-sentence, preserve order
        uniq: list[str] = []
        for s in spans:
            if s not in uniq:
                uniq.append(s)
        for s in uniq:
            ck = canon(s)
            if ck and ck not in seen_entity_keys:
                seen_entity_keys.add(ck)
                ex.entity_keysets.append(frozenset({ck}))
        # ordered pairs (i<j) within the sentence
        for i in range(len(uniq)):
            for j in range(i + 1, len(uniq)):
                f = _resolve(uniq[i], gold_amap)
                t = _resolve(uniq[j], gold_amap)
                if f and t and f != t:
                    ex.edge_triples.add((f, "co_occurs", t))
                    ex.edge_pairs.add((f, t))
    return ex


# --------------------------------------------------------------------------- #
# C1-gliner (OPTIONAL — entity-only NER; lazy import so the base census stays
# dependency-light + $0/deterministic). GLiNER is a local CPU/GPU model; entity
# coverage only (no relations), so it feeds the entity axis of the census only.
# --------------------------------------------------------------------------- #
# GLiNER is an OPTIONAL local dep (not in the dev/typecheck extras). Type the whole
# path as ``Any`` and shield the lazy import from static analysis so pyright stays
# green whether or not gliner is installed (the base `$0` census never imports it).
_GLINER_SINGLETON: Any = None


def _gliner_model(model_name: str = "urchade/gliner_small-v2.1") -> Any:
    global _GLINER_SINGLETON
    if _GLINER_SINGLETON is None:
        from gliner import GLiNER  # type: ignore  # lazy; only for the C1 arm

        _GLINER_SINGLETON = GLiNER.from_pretrained(model_name)
    return _GLINER_SINGLETON


def c1_gliner_extraction(
    body: str, entity_types: list[str], gold_amap: dict[str, str]
) -> Extraction:
    """GLiNER NER over the doc body, labelled with the doc's gold entity_types.
    Entity-only (no edges). Deterministic given the pinned model + threshold."""
    ex = Extraction()
    labels = entity_types or ["Person", "Organization", "Location"]
    model: Any = _gliner_model()
    seen: set[str] = set()
    for e in model.predict_entities(body or "", labels, threshold=0.5):
        ck = canon(str(e.get("text", "")))
        if ck and ck not in seen:
            seen.add(ck)
            ex.entity_keysets.append(frozenset({ck}))
    return ex


# --------------------------------------------------------------------------- #
# Scoring
# --------------------------------------------------------------------------- #
@dataclass
class Counts:
    gold_entities: int = 0
    covered_entities: int = 0
    extracted_entities: int = 0
    matched_extracted_entities: int = 0
    gold_edges: int = 0
    covered_edges: int = 0  # strict triple
    covered_pairs: int = 0  # relation-agnostic endpoint pair
    extracted_edges: int = 0
    matched_extracted_edges: int = 0

    def add(self, other: "Counts") -> None:
        for k in self.__dataclass_fields__:
            setattr(self, k, getattr(self, k) + getattr(other, k))

    def entity_recall(self) -> float:
        return self.covered_entities / self.gold_entities if self.gold_entities else 0.0

    def entity_precision(self) -> float:
        return (
            self.matched_extracted_entities / self.extracted_entities
            if self.extracted_entities
            else 0.0
        )

    def edge_recall(self) -> float:
        return self.covered_edges / self.gold_edges if self.gold_edges else 0.0

    def pair_recall(self) -> float:
        return self.covered_pairs / self.gold_edges if self.gold_edges else 0.0

    def edge_precision(self) -> float:
        return (
            self.matched_extracted_edges / self.extracted_edges
            if self.extracted_edges
            else 0.0
        )


def score_doc(gold: Extraction, pred: Extraction) -> Counts:
    c = Counts()
    # entities
    c.gold_entities = len(gold.entity_keysets)
    c.extracted_entities = len(pred.entity_keysets)
    for gk in gold.entity_keysets:
        if any(gk & pk for pk in pred.entity_keysets):
            c.covered_entities += 1
    for pk in pred.entity_keysets:
        if any(pk & gk for gk in gold.entity_keysets):
            c.matched_extracted_entities += 1
    # edges (strict triple)
    c.gold_edges = len(gold.edge_triples)
    c.extracted_edges = len(pred.edge_triples)
    c.covered_edges = len(gold.edge_triples & pred.edge_triples)
    c.matched_extracted_edges = len(pred.edge_triples & gold.edge_triples)
    # endpoint-pair (relation-agnostic) recall
    c.covered_pairs = len(gold.edge_pairs & pred.edge_pairs)
    return c


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #
def load_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def run_census(with_gliner: bool = False) -> dict:
    gold_docs = load_jsonl(GOLD_PATH)
    baseline_rows = load_jsonl(BASELINE_PATH)
    baseline_by_id = {r["doc_id"]: r for r in baseline_rows}

    arms = ["c0_floor", "elps_baseline"]
    if with_gliner:
        arms.append("c1_gliner")
    overall: dict[str, Counts] = {a: Counts() for a in arms}
    per_class: dict[str, dict[str, Counts]] = {a: defaultdict(Counts) for a in arms}

    for doc in gold_docs:
        doc_id = doc["doc_id"]
        kind = doc.get("kind", "unknown")
        gold_amap = _alias_map(doc.get("entities", []))
        gold_ex = gold_to_extraction(doc)

        # C0-floor
        c0 = c0_floor_extraction(doc.get("body", ""), gold_amap)
        c0c = score_doc(gold_ex, c0)
        overall["c0_floor"].add(c0c)
        per_class["c0_floor"][kind].add(c0c)

        # C1-gliner (entity-only), if requested
        if with_gliner:
            g1 = c1_gliner_extraction(
                doc.get("body", ""), doc.get("entity_types", []), gold_amap
            )
            g1c = score_doc(gold_ex, g1)
            overall["c1_gliner"].add(g1c)
            per_class["c1_gliner"][kind].add(g1c)

        # ELPS baseline (pre-computed)
        row = baseline_by_id.get(doc_id)
        if row is not None:
            out = row["outputs"][0]
            out = json.loads(out) if isinstance(out, str) else out
            base_ex = baseline_to_extraction(out, gold_amap)
        else:
            base_ex = Extraction()
        bc = score_doc(gold_ex, base_ex)
        overall["elps_baseline"].add(bc)
        per_class["elps_baseline"][kind].add(bc)

    def counts_to_dict(c: Counts) -> dict:
        return {
            "gold_entities": c.gold_entities,
            "entity_recall": round(c.entity_recall(), 4),
            "entity_precision": round(c.entity_precision(), 4),
            "gold_edges": c.gold_edges,
            "edge_recall_strict": round(c.edge_recall(), 4),
            "edge_recall_pair": round(c.pair_recall(), 4),
            "edge_precision": round(c.edge_precision(), 4),
            "extracted_entities": c.extracted_entities,
            "extracted_edges": c.extracted_edges,
            "under_powered": c.gold_edges < POWER_MIN,
        }

    result = {
        "corpus": "personal.gold (memex-elps)",
        "n_docs": len(gold_docs),
        "power_min_edges": POWER_MIN,
        "arms": {a: counts_to_dict(overall[a]) for a in arms},
        "per_class": {
            a: {k: counts_to_dict(per_class[a][k]) for k in sorted(per_class[a])}
            for a in arms
        },
    }
    return result


if __name__ == "__main__":
    import sys

    with_gliner = "--gliner" in sys.argv
    res = run_census(with_gliner=with_gliner)
    json.dump(res, sys.stdout, indent=2)
    sys.stdout.write("\n")
