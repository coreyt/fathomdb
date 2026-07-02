#!/usr/bin/env python3
"""EXP-COV-1 — coverage->outcome sufficiency sweep: shared pure helpers.

0.8.12 priced sweep (HITL-approved $20). Authoritative design: Memex
``dev/fathomdb/OPP-6-experiments.md`` (EXP-COV-1, §2 sweep, §4 metric, §7 rule).
Prior finding (Slice 5, ``EXP-COV-results.md``): entity coverage is solved; the gap
is EDGES/RELATIONS. This sweep tests **sufficiency** — does closing the edge/relation
coverage gap move a DOWNSTREAM retrieval metric above the ~0.571 embedder ceiling,
holding the retrieval + CE-rerank stack FIXED?

This module holds the *deterministic, `$0`, network-free* core:

* the **relation-focused extraction prompt** (the priced lever) + its stable
  ``PROMPT_VERSION`` (cache-namespacing key);
* the **C0-floor heuristic extractor** (capitalized spans + intra-sentence
  co-occurrence edges) emitting the ``fathomdb.extract.v1`` record shape;
* the **extraction cache** (NDJSON, atomic write, resume by
  ``doc_id + model + prompt_version``) + the **completeness guard**;
* the **$ ledger** (actual-usage tokens x a pinned price map) with a HARD
  auto-stop ceiling;
* the **downstream scorers** (gold-in-pool / recall@k / MRR) + a **paired
  bootstrap** delta CI and the **pre-registered decision rule**.

Everything here is import-light (stdlib + numpy) and has **no** ``fathomdb`` import,
so the unit tests run without the native build. The live engine ingest + search and
the priced LLM call live in the sibling runner modules.
"""

from __future__ import annotations

import json
import os
import re
import tempfile
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional, Sequence

import numpy as np

# --------------------------------------------------------------------------- #
# Pre-registration constants
# --------------------------------------------------------------------------- #

#: The pre-registered sufficiency threshold (OPP-6 §7): a coverage increase is the
#: binding lever iff the paired-bootstrap **CI lower bound** of the downstream delta
#: strictly exceeds this on >=1 powered class.
SUFFICIENCY_CI_LO_THRESHOLD: float = 0.04

#: The embedder-bound IR-relevance ceiling of record (eu8 / 0.8.3-mem0-parity).
#: Held fixed across all coverage conditions (CLS-corrected bge-small, no swap).
EMBEDDER_CEILING: float = 0.571

#: Per-class power floor (min scored questions) below which a cell makes no claim.
POWER_MIN_QUESTIONS: int = 30

#: The priced-run HARD spend ceiling (the user personally approved $20). The
#: extraction runner AUTO-STOPS when the cumulative ledger reaches this.
HARD_DOLLAR_CEILING: float = 20.0

#: Cache-namespacing prompt version for the relation-focused lever. Bump on any
#: prompt change so cached extractions never collide across prompt revisions.
PROMPT_VERSION: str = "cov1-relation-1"

#: Held-fixed retrieval knobs (the CE stack; identical across all conditions).
RERANK_DEPTH: int = 50
POOL_N: int = 50
ALPHA: float = 1.0  # the measured Mem0-parity CE-blend (0.8.3); best-shot for coverage

#: Bootstrap config (paired percentile CI; fixed seed = deterministic).
BOOT_SEED: int = 0xC0F1
BOOT_RESAMPLES: int = 2000

#: Multi_session hit requires the FULL gold session set in top-K (LME/LOCOMO rule).
MULTI_GOLD_FULLSET_CLASSES = frozenset({"multi_session"})


# --------------------------------------------------------------------------- #
# The relation-focused extraction prompt (the priced lever)
# --------------------------------------------------------------------------- #
# The census found the gap is edges/relations (~1/3-1/2 of the miss is relation-LABEL
# disagreement, the rest genuinely-missed facts). This prompt is a relation-MAXIMIZING
# variant of the ELPS system prompt: it keeps the entity contract but pushes hard on
# extracting *every* directed fact-edge, especially cross-turn temporal / attribute /
# event / preference relations, with a controlled lowercase relation vocabulary so the
# graph arm's edges connect the right endpoints. Extraction is Memex-side per the seam,
# but the sweep owns the prompt (coverage is the SUT).

RELATION_SYSTEM_PROMPT = """\
You are a relation-extraction engine specialized in building a dense, connected \
fact-graph from a single conversational document. Your PRIMARY objective is RELATION \
RECALL: extract every directed fact-edge the text supports. Entities exist only to \
anchor edges.

ENTITIES
- Identify the real-world entities named or clearly referred to (people, organizations, \
places, artifacts, events, dates, concepts). Resolve pronouns/speakers to the named \
entity when the text makes it unambiguous.
- Each entity: `name`, `type`, optional `aliases`. Be conservative about identity \
(do not merge homonyms) but DO resolve obvious co-reference within the document.

EDGES (directed, dated facts) — MAXIMIZE THESE
- Emit one edge per fact: `from_entity` -> `to_entity` with a short, lowercase, \
snake_case `relation`. Prefer this controlled vocabulary when it fits, else coin a \
concise snake_case label:
  works_at, lives_in, located_in, born_in, member_of, part_of, founded, owns, \
  created, attended, visited, met, married_to, parent_of, child_of, sibling_of, \
  friend_of, likes, dislikes, prefers, wants, plans_to, decided_to, bought, sold, \
  gave, received, has, is_a, happened_on, scheduled_for, caused, related_to.
- Extract facts that span turns: if speaker A states something about entity X in one \
turn and it is elaborated later, emit the full relation. Capture temporal facts \
(when something happened / is planned) and preference/attribute facts explicitly.
- `body` = the minimal source span stating the fact. Reference entities by the exact \
`name` used in the entities list.

TEMPORAL
- `t_valid` = when the fact became true as stated; if unstated, use the document's \
`created_at`. Never use ingestion/now. Set `t_invalid` only on explicit end evidence.

CONFIDENCE
- `confidence` in [0,1]. Emit ALL facts you find — do not self-censor low-confidence \
facts; report low confidence instead. Favor recall.

Return ONLY valid JSON (no markdown fences, no prose) matching exactly:
{
  "entities": [{"name": str, "type": str, "aliases": list[str]}],
  "edges": [
    {"from_entity": str, "to_entity": str, "relation": str, "body": str,
     "t_valid": str, "t_invalid": str | null, "confidence": float,
     "source_doc_id": str, "source_span": [int, int] | null}
  ],
  "warnings": []
}"""

RELATION_USER_TEMPLATE = """\
Extract from this single document. Maximize directed fact-edges.
doc_id: {doc_id}
created_at: {created_at}
body:
{body}"""


# --------------------------------------------------------------------------- #
# Canonicalization (mirrors exp_cov_census)
# --------------------------------------------------------------------------- #
def canon(text: str) -> str:
    if not text:
        return ""
    t = unicodedata.normalize("NFKD", text)
    t = "".join(ch for ch in t if not unicodedata.combining(ch))
    return re.sub(r"\s+", " ", t).strip().casefold()


# --------------------------------------------------------------------------- #
# C0-floor heuristic extractor -> fathomdb.extract.v1 record shape
# --------------------------------------------------------------------------- #
_CAP_SPAN = re.compile(r"\b[A-Z][a-zA-Z0-9]*(?:\s+[A-Z][a-zA-Z0-9]*)*\b")
_SENT_SPLIT = re.compile(r"(?<=[.!?])\s+|\n+")
_STOP_CAPS = {
    "the", "a", "an", "i", "we", "he", "she", "they", "it", "this", "that", "these",
    "those", "my", "our", "his", "her", "their", "monday", "tuesday", "wednesday",
    "thursday", "friday", "saturday", "sunday", "january", "february", "march",
    "april", "may", "june", "july", "august", "september", "october", "november",
    "december", "today", "tomorrow", "yesterday", "and", "but", "so", "if", "when",
}


def c0_floor_extract(doc_id: str, body: str, created_at: str) -> dict[str, Any]:
    """Deterministic low-anchor extractor: capitalized spans as entities, ordered
    intra-sentence co-occurring pairs as ``co_occurs`` edges. Emits the
    ``fathomdb.extract.v1`` per-doc record shape (entities/edges/warnings)."""
    entities: list[dict[str, Any]] = []
    seen_e: set[str] = set()
    edges: list[dict[str, Any]] = []
    seen_edge: set[tuple[str, str]] = set()
    for sent in _SENT_SPLIT.split(body or ""):
        uniq: list[str] = []
        for m in _CAP_SPAN.finditer(sent):
            span = m.group(0).strip()
            if not span or canon(span) in _STOP_CAPS:
                continue
            if span not in uniq:
                uniq.append(span)
        for s in uniq:
            ck = canon(s)
            if ck and ck not in seen_e:
                seen_e.add(ck)
                entities.append({"name": s, "type": "concept", "aliases": []})
        for i in range(len(uniq)):
            for j in range(i + 1, len(uniq)):
                if canon(uniq[i]) == canon(uniq[j]):
                    continue
                key = (canon(uniq[i]), canon(uniq[j]))
                if key in seen_edge:
                    continue
                seen_edge.add(key)
                edges.append(
                    {
                        "from_entity": uniq[i],
                        "to_entity": uniq[j],
                        "relation": "co_occurs",
                        "body": sent.strip()[:200],
                        "t_valid": created_at,
                        "t_invalid": None,
                        "confidence": 0.3,
                        "source_doc_id": doc_id,
                        "source_span": None,
                    }
                )
    return {"entities": entities, "edges": edges, "warnings": []}


# --------------------------------------------------------------------------- #
# Extraction cache (NDJSON; atomic; resume by stable key)
# --------------------------------------------------------------------------- #
def cache_key(doc_id: str, model: str, prompt_version: str) -> str:
    return f"{doc_id}||{model}||{prompt_version}"


@dataclass
class ExtractionCache:
    """A resumable, atomically-persisted extraction cache.

    Each record: ``{key, doc_id, model, prompt_version, status, entities, edges,
    warnings, usage}`` where ``status`` in {``ok``, ``failed``}. A ``failed`` unit is
    RECORDED (not silently dropped) so the completeness guard can distinguish
    "present-and-failed" from "missing". Persisted as NDJSON; every mutation rewrites
    the whole file via temp-file + ``os.replace`` (atomic; at most the in-flight unit
    is lost on a crash)."""

    path: Path
    records: dict[str, dict[str, Any]] = field(default_factory=dict)

    @classmethod
    def load(cls, path: str | Path) -> "ExtractionCache":
        p = Path(path)
        recs: dict[str, dict[str, Any]] = {}
        if p.exists():
            for line in p.read_text(encoding="utf-8").splitlines():
                line = line.strip()
                if not line:
                    continue
                r = json.loads(line)
                recs[r["key"]] = r
        return cls(path=p, records=recs)

    def has_ok(self, key: str) -> bool:
        r = self.records.get(key)
        return bool(r and r.get("status") == "ok")

    def put(self, record: dict[str, Any]) -> None:
        self.records[record["key"]] = record
        self._flush()

    def _flush(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        fd, tmp = tempfile.mkstemp(dir=str(self.path.parent), suffix=".tmp")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as fh:
                for k in sorted(self.records):
                    fh.write(json.dumps(self.records[k], sort_keys=True) + "\n")
                fh.flush()
                os.fsync(fh.fileno())
            os.replace(tmp, self.path)
        finally:
            if os.path.exists(tmp):
                os.unlink(tmp)

    def completeness(
        self, expected_keys: Sequence[str]
    ) -> dict[str, Any]:
        """The completeness guard: every expected unit must be present-or-explicitly
        -failed. Returns a report; ``ok`` is False if any unit is missing."""
        present = [k for k in expected_keys if k in self.records]
        missing = [k for k in expected_keys if k not in self.records]
        ok_units = [k for k in expected_keys if self.has_ok(k)]
        failed = [
            k for k in expected_keys
            if k in self.records and self.records[k].get("status") != "ok"
        ]
        return {
            "n_expected": len(expected_keys),
            "n_present": len(present),
            "n_ok": len(ok_units),
            "n_failed": len(failed),
            "n_missing": len(missing),
            "missing_keys": missing[:20],
            "ok": len(missing) == 0,  # scoring REFUSES until nothing is missing
        }


# --------------------------------------------------------------------------- #
# $ ledger (actual-usage tokens x a pinned price map) + HARD auto-stop
# --------------------------------------------------------------------------- #
#: USD per 1M tokens (input, output). Conservative, pinned in the artifact. The
#: ledger multiplies ACTUAL response-usage tokens by these; unknown models fall back
#: to the most-expensive row so the ceiling is never under-counted.
PRICE_PER_MTOK: dict[str, tuple[float, float]] = {
    "claude-haiku": (1.00, 5.00),
    "claude-sonnet": (3.00, 15.00),
    "claude-opus": (15.00, 75.00),
    "gpt-5": (1.25, 10.00),
    "gpt-5-mini": (0.25, 2.00),
    "gpt-5-nano": (0.05, 0.40),
    "gemini-3.1-pro": (1.25, 10.00),
    "gemini-3.1-flash-lite": (0.10, 0.40),
    "gemini-flash-lite": (0.10, 0.40),
    "gemini-flash": (0.30, 2.50),
    # local vLLM (metered $0 — user-owned GPU)
    "qwen3.6-27b": (0.0, 0.0),
    "gemma-4": (0.0, 0.0),
    "qwen3-32b": (0.0, 0.0),
}
_FALLBACK_PRICE = (15.00, 75.00)  # most-expensive => never under-count the ceiling


def model_price(model: str) -> tuple[float, float]:
    key = model.split("/")[-1]
    return PRICE_PER_MTOK.get(key, _FALLBACK_PRICE)


def usage_cost(model: str, prompt_tokens: int, completion_tokens: int) -> float:
    pin, pout = model_price(model)
    return (prompt_tokens * pin + completion_tokens * pout) / 1_000_000.0


@dataclass
class DollarLedger:
    """Running $ ledger with a HARD auto-stop ceiling. Appended per unit."""

    path: Path
    ceiling: float = HARD_DOLLAR_CEILING
    entries: list[dict[str, Any]] = field(default_factory=list)
    total: float = 0.0

    @classmethod
    def load(cls, path: str | Path, ceiling: float = HARD_DOLLAR_CEILING) -> "DollarLedger":
        p = Path(path)
        entries: list[dict[str, Any]] = []
        total = 0.0
        if p.exists():
            for line in p.read_text(encoding="utf-8").splitlines():
                line = line.strip()
                if not line:
                    continue
                e = json.loads(line)
                entries.append(e)
                total += float(e.get("cost_usd", 0.0))
        return cls(path=p, ceiling=ceiling, entries=entries, total=round(total, 6))

    def add(
        self, *, doc_id: str, model: str, prompt_tokens: int, completion_tokens: int
    ) -> float:
        cost = usage_cost(model, prompt_tokens, completion_tokens)
        self.total = round(self.total + cost, 6)
        e = {
            "doc_id": doc_id,
            "model": model,
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "cost_usd": round(cost, 6),
            "cumulative_usd": self.total,
        }
        self.entries.append(e)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(e) + "\n")
            fh.flush()
            os.fsync(fh.fileno())
        return cost

    def would_exceed(self, projected_next: float) -> bool:
        return (self.total + projected_next) > self.ceiling

    def at_ceiling(self) -> bool:
        return self.total >= self.ceiling


# --------------------------------------------------------------------------- #
# Downstream scorers (gold-in-pool / recall@k / MRR) — pure
# --------------------------------------------------------------------------- #
def hit_at_k(
    gold: Sequence[str], retrieved: Sequence[str], k: int, reporting_class: str
) -> Optional[float]:
    """Recall@K hit. multi_session => FULL gold set in top-K; else any-hit.
    Abstention (empty gold) => None (excluded)."""
    if not gold:
        return None
    topk = list(retrieved)[:k]
    gold_set = set(gold)
    if reporting_class in MULTI_GOLD_FULLSET_CLASSES:
        return 1.0 if gold_set.issubset(set(topk)) else 0.0
    return 1.0 if any(g in topk for g in gold_set) else 0.0


def reciprocal_rank(gold: Sequence[str], retrieved: Sequence[str]) -> Optional[float]:
    if not gold:
        return None
    gold_set = set(gold)
    for i, d in enumerate(retrieved):
        if d in gold_set:
            return 1.0 / (i + 1)
    return 0.0


# --------------------------------------------------------------------------- #
# Paired bootstrap delta CI + the pre-registered decision rule
# --------------------------------------------------------------------------- #
def paired_bootstrap_delta(
    treatment: Sequence[float],
    baseline: Sequence[float],
    *,
    seed: int = BOOT_SEED,
    n: int = BOOT_RESAMPLES,
    ci: float = 0.95,
) -> dict[str, Any]:
    """Paired bootstrap of the mean per-query delta ``treatment - baseline`` (both
    aligned on the SAME query order). Returns point + percentile CI + n."""
    t = np.asarray(treatment, dtype=np.float64)
    b = np.asarray(baseline, dtype=np.float64)
    if t.shape != b.shape:
        raise ValueError(f"paired arrays misaligned: {t.shape} vs {b.shape}")
    m = t.shape[0]
    if m == 0:
        return {"point": None, "lo": None, "hi": None, "n": 0}
    d = t - b
    point = float(d.mean())
    rng = np.random.default_rng(seed)
    idx = rng.integers(0, m, size=(n, m))
    means = d[idx].mean(axis=1)
    lo_p = (1.0 - ci) / 2.0 * 100.0
    hi_p = (1.0 + ci) / 2.0 * 100.0
    return {
        "point": round(point, 4),
        "lo": round(float(np.percentile(means, lo_p)), 4),
        "hi": round(float(np.percentile(means, hi_p)), 4),
        "n": m,
    }


def decision_for_class(delta_ci: dict[str, Any], *, n_scored: int) -> str:
    """Per-class verdict under the pre-registered rule."""
    if n_scored < POWER_MIN_QUESTIONS:
        return "UNDERPOWERED"
    lo = delta_ci.get("lo")
    if lo is None:
        return "NO_DATA"
    if lo > SUFFICIENCY_CI_LO_THRESHOLD:
        return "SUFFICIENT"
    return "CEILING_ABSORBED"


def overall_verdict(per_class_verdicts: dict[str, str]) -> str:
    """SUFFICIENT iff >=1 powered class is SUFFICIENT; else CEILING_ABSORBED if any
    powered class was scored; else INCONCLUSIVE."""
    vals = list(per_class_verdicts.values())
    if any(v == "SUFFICIENT" for v in vals):
        return "SUFFICIENT"
    if any(v == "CEILING_ABSORBED" for v in vals):
        return "CEILING_ABSORBED"
    return "INCONCLUSIVE"
