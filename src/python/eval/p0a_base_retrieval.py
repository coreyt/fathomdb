"""P0-A base-retrieval ablation runner (LME, LLM-free retrieval + airlock E2E).

Binding contract: ``dev/design/0.8.1-graph-experiment-plan.md`` §1/§2/§3 and the
diagnosis ``dev/design/fathomdb-graph-vs-mem0-zep-and-longmemeval-diagnosis.md``
(Cause C — base retrieval trails BM25, upstream of any graph). This module
isolates *why*, LLM-free, before any graph work. **No graph arm, no extraction.**

Two loops (plan §1):

* **Retrieval (LLM-free, deterministic):** three variants — ``naive_bm25``,
  ``fathomdb_fts_only``, ``fathomdb_fused`` — scored per-class Recall@K vs the
  pre-labeled ``answer_session_id`` gold, with the multi_session full-gold-set
  rule, abstention exclusion, and a graded metric (MRR / nDCG@10).
* **End-to-end (airlock reader):** each variant's retrieved session bodies feed an
  airlock answerer (reusing the ELPS proxy); answer accuracy scored by
  normalized substring match. Run two readers to separate retrieval delta from
  reader contribution (plan §7.2).

Scorer rules (the point of this slice) live in the pure functions
``hit_at_k`` / ``reciprocal_rank`` / ``ndcg_at_k`` / ``aggregate`` — DB- and
LLM-free, TDD'd in ``tests/test_p0a_scorer.py``.

HARD limit: the full 500-question / 19,195-session run requires an explicit
``--full`` flag (HITL-gated). The default is a small, class-balanced smoke.
"""

from __future__ import annotations

import json
import math
import os
import random
import time
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional, Sequence

from eval.r2_parity_eval import (
    BaseAnswerer,
    FathomDBAdapter,
    NaiveRAGAdapter,
    RetrievalAdapter,
    _format_lme_session,
    _match,
    normalize_answer,
)

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #

#: LME question_type -> reporting class (P0-A operates on the four memory classes).
LME_CLASS_MAP: dict[str, str] = {
    "temporal-reasoning": "temporal",
    "knowledge-update": "knowledge_update",
    "multi-session": "multi_session",
    "single-session-user": "factoid",
    "single-session-assistant": "factoid",
    "single-session-preference": "factoid",
    "temporal-reasoning_abs": "abstention",
    "knowledge-update_abs": "abstention",
    "multi-session_abs": "abstention",
    "single-session-user_abs": "abstention",
    "single-session-assistant_abs": "abstention",
    "single-session-preference_abs": "abstention",
}

#: Classes whose Recall@K "hit" requires the FULL gold session set in top-K
#: (plan §1 caveat a). Every other class uses any-hit.
MULTI_GOLD_FULLSET_CLASSES = frozenset({"multi_session"})

#: The four memory classes the smoke draws balanced from.
SMOKE_CLASSES: tuple[str, ...] = ("factoid", "temporal", "knowledge_update", "multi_session")

#: Fixed seed so the smoke set is drawn ONCE, reproducibly (plan §3.5
#: selection-bias guard).
DEFAULT_SEED = 20260614

DEFAULT_DATASET = "xiaowu0162/longmemeval-cleaned"
DEFAULT_SPLIT = "longmemeval_s_cleaned"

#: Airlock reader model ids (discovered from the live proxy / elps_live_harness
#: env). ``gpt-5`` is the closest available id to the requested ``gpt-5.4``.
DEFAULT_READERS: tuple[str, ...] = ("claude-haiku", "gpt-5")

_AIRLOCK_BASE_URL = os.environ.get("ELPS_LLM_BASE_URL", "http://localhost:4000/v1")
_AIRLOCK_API_KEY = os.environ.get("ELPS_LLM_API_KEY", "sk-airlock-mk")


# --------------------------------------------------------------------------- #
# Scorer — pure functions (no DB, no LLM). The heart of P0-A.
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class RetrievalRecord:
    """One scored question: its class, gold session ids (empty => abstention),
    and the variant's ranked retrieved session ids."""

    reporting_class: str
    gold_sessions: tuple[str, ...]
    retrieved_ids: tuple[str, ...]


def hit_at_k(
    gold: Sequence[str],
    retrieved_ids: Sequence[str],
    k: int,
    reporting_class: str,
) -> Optional[float]:
    """Recall@K "hit" (plan §1).

    Returns ``None`` for abstention questions (empty ``gold``) so the caller can
    *exclude* them rather than score them 0 (caveat b). For ``multi_session`` a
    hit requires the FULL gold set in top-K (caveat a); every other class uses
    any-hit.
    """
    if not gold:
        return None
    topk = list(retrieved_ids)[:k]
    gold_set = set(gold)
    if reporting_class in MULTI_GOLD_FULLSET_CLASSES:
        topk_set = set(topk)
        return 1.0 if gold_set.issubset(topk_set) else 0.0
    return 1.0 if any(g in topk for g in gold_set) else 0.0


def reciprocal_rank(gold: Sequence[str], retrieved_ids: Sequence[str]) -> Optional[float]:
    """Reciprocal rank of the first gold session (graded; abstention => None)."""
    if not gold:
        return None
    gold_set = set(gold)
    for i, d in enumerate(retrieved_ids):
        if d in gold_set:
            return 1.0 / (i + 1)
    return 0.0


def ndcg_at_k(gold: Sequence[str], retrieved_ids: Sequence[str], k: int) -> Optional[float]:
    """nDCG@K with binary relevance over the gold set (abstention => None).

    Ideal DCG normalizes by placing ``min(|gold|, k)`` relevant items at the top,
    so multi-gold classes are graded fairly.
    """
    if not gold:
        return None
    gold_set = set(gold)
    topk = list(retrieved_ids)[:k]
    dcg = sum(1.0 / math.log2(i + 2) for i, d in enumerate(topk) if d in gold_set)
    ideal_n = min(len(gold_set), k)
    idcg = sum(1.0 / math.log2(i + 2) for i in range(ideal_n))
    return (dcg / idcg) if idcg > 0 else 0.0


def _mean(values: list[float]) -> Optional[float]:
    return round(sum(values) / len(values), 4) if values else None


def aggregate(
    records: Sequence[RetrievalRecord],
    *,
    ks: tuple[int, ...] = (5, 10, 20),
    graded_k: int = 10,
) -> dict[str, Any]:
    """Aggregate per-class Recall@K (abstention-excluded) + graded MRR / nDCG.

    ``recall_at_<k>`` and the graded metrics are means over *scored* (non-
    abstention) questions only; ``None`` when a class has no scored questions.
    Abstention is counted in ``n_abstention`` and the top-level
    ``abstention_total``.
    """
    by_class: dict[str, list[RetrievalRecord]] = defaultdict(list)
    for r in records:
        by_class[r.reporting_class].append(r)

    per_class: dict[str, dict[str, Any]] = {}
    abstention_total = 0
    for cls, recs in by_class.items():
        recalls: dict[int, list[float]] = {k: [] for k in ks}
        rrs: list[float] = []
        ndcgs: list[float] = []
        n_abst = 0
        for r in recs:
            if not r.gold_sessions:
                n_abst += 1
                continue
            for k in ks:
                h = hit_at_k(r.gold_sessions, r.retrieved_ids, k, cls)
                if h is not None:
                    recalls[k].append(h)
            rr = reciprocal_rank(r.gold_sessions, r.retrieved_ids)
            nd = ndcg_at_k(r.gold_sessions, r.retrieved_ids, graded_k)
            if rr is not None:
                rrs.append(rr)
            if nd is not None:
                ndcgs.append(nd)
        abstention_total += n_abst
        entry: dict[str, Any] = {
            "n_total": len(recs),
            "n_scored": len(recs) - n_abst,
            "n_abstention": n_abst,
            "mrr": _mean(rrs),
            f"ndcg_at_{graded_k}": _mean(ndcgs),
        }
        for k in ks:
            entry[f"recall_at_{k}"] = _mean(recalls[k])
        per_class[cls] = entry

    return {
        "ks": list(ks),
        "graded_k": graded_k,
        "per_class": per_class,
        "abstention_total": abstention_total,
    }


# --------------------------------------------------------------------------- #
# LME smoke loader — class-balanced subsample with full per-question haystacks
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class SmokeQuestion:
    qid: str
    reporting_class: str
    question: str
    answer: str
    gold_sessions: tuple[str, ...]
    haystack_session_ids: tuple[str, ...]

    @property
    def is_abstention(self) -> bool:
        return not self.gold_sessions


@dataclass
class SmokeSet:
    questions: list[SmokeQuestion]
    documents: dict[str, str]  # session_id -> formatted body (union of haystacks)
    per_question_h: dict[str, int] = field(default_factory=dict)


def _select_balanced_qids(
    dataset_name: str,
    split: str,
    *,
    per_class: int,
    classes: tuple[str, ...],
    seed: int,
) -> tuple[dict[str, str], dict[str, list[str]]]:
    """Pass 1 (column-projected, cheap): map every question_id -> reporting class,
    then sample ``per_class`` ids from each requested class with a fixed seed.

    Returns ``(chosen_qid -> class, by_class_all_qids)``.
    """
    from datasets import load_dataset  # type: ignore[import]

    ds = load_dataset(dataset_name, split=split, streaming=True)
    ds = ds.select_columns(["question_id", "question_type"])

    by_class: dict[str, list[str]] = defaultdict(list)
    for inst in ds:
        qtype = str(inst.get("question_type") or "")
        cls = LME_CLASS_MAP.get(qtype, "unknown")
        qid = str(inst.get("question_id") or "")
        if qid:
            by_class[cls].append(qid)

    rng = random.Random(seed)
    chosen: dict[str, str] = {}
    for cls in classes:
        pool = sorted(by_class.get(cls, []))
        take = pool if len(pool) <= per_class else rng.sample(pool, per_class)
        for qid in take:
            chosen[qid] = cls
    return chosen, by_class


def load_lme_smoke(
    dataset_name: str = DEFAULT_DATASET,
    split: str = DEFAULT_SPLIT,
    *,
    per_class: int = 4,
    classes: tuple[str, ...] = SMOKE_CLASSES,
    seed: int = DEFAULT_SEED,
) -> SmokeSet:
    """Draw a small, class-balanced smoke set with FULL per-question haystacks.

    Plan §1.5 / §3.1: never shrink the per-question haystack (difficulty); only
    subsample *questions*. Ingests the union of the chosen questions' haystacks.
    """
    from datasets import load_dataset  # type: ignore[import]

    chosen, _all = _select_balanced_qids(
        dataset_name, split, per_class=per_class, classes=classes, seed=seed
    )

    ds = load_dataset(dataset_name, split=split, streaming=True)
    questions: list[SmokeQuestion] = []
    documents: dict[str, str] = {}
    per_question_h: dict[str, int] = {}
    remaining = set(chosen)

    for inst in ds:
        qid = str(inst.get("question_id") or "")
        if qid not in remaining:
            continue
        remaining.discard(qid)

        session_ids: list[str] = list(inst.get("haystack_session_ids") or [])
        sessions: list[list[dict]] = list(inst.get("haystack_sessions") or [])
        for sid, turns in zip(session_ids, sessions):
            if sid not in documents:
                documents[sid] = _format_lme_session(turns)

        per_question_h[qid] = len(session_ids)
        questions.append(
            SmokeQuestion(
                qid=qid,
                reporting_class=chosen[qid],
                question=str(inst.get("question") or ""),
                answer=str(inst.get("answer") or ""),
                gold_sessions=tuple(str(s) for s in (inst.get("answer_session_ids") or [])),
                haystack_session_ids=tuple(str(s) for s in session_ids),
            )
        )
        if not remaining:
            break

    questions.sort(key=lambda q: (q.reporting_class, q.qid))
    return SmokeSet(questions=questions, documents=documents, per_question_h=per_question_h)


# --------------------------------------------------------------------------- #
# Retrieval variants
# --------------------------------------------------------------------------- #


def _drain_until_embedded(
    db_path: Path,
    engine: Any,
    *,
    expected_docs: int,
    kind: str = "doc",
    chunk_s: float = 300.0,
    stall_chunks: int = 3,
    log: Any = print,
) -> Any:
    """Resumably drain the projection queue until every doc carries a vector.

    Polls in ``chunk_s`` slices: ``drain()`` returns when the queue goes idle, or
    raises on timeout (still working). After each slice the verifier reports true,
    WAL-visible coverage. Returns the final ``VerifyReport`` — complete (``rep.ok``),
    or the best snapshot if the embed goes idle-but-incomplete (stuck: kind
    unregistered / failed projections) or stalls (no progress for ``stall_chunks``).
    Resumable: on a fresh process, reopen + call this again → it continues from the
    persisted projection state. Fixes the prior single ``drain(3600)`` which would
    raise after 1 h while a multi-hour embed kept running in the background."""
    from eval.verify_embed_db import inspect_embed_db

    last_embedded = -1
    stalls = 0
    while True:
        idle = False
        try:
            engine.drain(timeout_s=chunk_s)
            idle = True
        except Exception as exc:  # noqa: BLE001 - drain timeout == "still working"; verify below
            log(f"[embed] drain slice not idle ({type(exc).__name__}); checking progress")
        rep = inspect_embed_db(str(db_path), expected_docs=expected_docs, kind=kind)
        log(f"[embed] coverage={rep.coverage:.4f} ({rep.n_docs_embedded}/{rep.n_docs}) idle={idle}")
        if rep.ok:
            return rep
        if idle:
            # Queue idle but coverage<1.0 -> stuck (kind unregistered / failed
            # projections). Waiting won't help; return the snapshot (caller -> blocker).
            log("[embed] queue idle but coverage<1.0 -> stopping (not resolvable by waiting)")
            return rep
        if rep.n_docs_embedded > last_embedded:
            last_embedded = rep.n_docs_embedded
            stalls = 0
        else:
            stalls += 1
            if stalls >= stall_chunks:
                log(f"[embed] no progress for {stall_chunks} slices -> stopping")
                return rep


def _build_fathomdb_variant(
    documents: dict[str, str],
    db_path: Path,
    *,
    use_embedder: bool,
    register_doc_vector_kind: bool,
    batch: int = 500,
) -> tuple[Optional[FathomDBAdapter], Optional[dict[str, str]]]:
    """Ingest doc nodes (one per LME session) and return a retrieval adapter.

    ``register_doc_vector_kind`` configures ``doc`` as a vector kind so the dense
    arm actually covers doc nodes — without it the engine vector-projects only
    auto-registered ``edge_fact`` rows, so ``use_embedder=True`` over doc-only
    ingest is identical to FTS-only. There is **no production Python surface** to
    register a node vector kind; this uses the ``test-hooks`` seam
    ``_configure_vector_kind_for_test`` (see reserved_followups). The vectors and
    the engine are real — nothing is mocked.
    """
    try:
        from fathomdb.engine import Engine
    except Exception as exc:  # noqa: BLE001
        return None, {
            "id": "fathomdb-sdk-unavailable",
            "description": f"could not import fathomdb SDK ({exc})",
            "resolution": "UNRESOLVED — variant skipped",
        }
    db_path.parent.mkdir(parents=True, exist_ok=True)
    if db_path.exists():
        db_path.unlink()
    try:
        engine = Engine.open(str(db_path), use_default_embedder=use_embedder)
        if register_doc_vector_kind:
            engine._native._configure_vector_kind_for_test("doc")  # type: ignore[attr-defined]

        cursor_to_doc: dict[int, str] = {}
        items = list(documents.items())
        for start in range(0, len(items), batch):
            chunk = items[start : start + batch]
            # 0.8.20 (R-20-E3): provenance is mandatory; the corpus doc id IS it.
            receipt = engine.write(
                [{"kind": "doc", "body": body, "source_id": doc_id} for doc_id, body in chunk]
            )
            for (sid, _body), cursor in zip(chunk, receipt.row_cursors):
                cursor_to_doc[int(cursor)] = sid
        # ``drain`` returns as soon as the projection queue is empty; the large
        # budget only caps a pathological stall. The dense arm must be FULLY
        # embedded before retrieval, else fused recall is measured over a
        # partially-projected corpus (a background scheduler keeps embedding
        # after a too-short drain returns).
        if register_doc_vector_kind:
            # Resumable drain-to-complete + completeness GATE: a single drain() with a
            # huge timeout is wrong for a multi-hour embed (it would raise after the
            # timeout while embedding continues), and drain-returns-idle != embedded
            # (docs can be `projection_terminal` WITHOUT a vector). Poll in chunks,
            # use the verifier's coverage as the completion oracle, watch for stalls,
            # and only serve fused recall at coverage==1.0. Resumable: the embed state
            # is persisted, so on a process restart reopen + this loop continues.
            rep = _drain_until_embedded(db_path, engine, expected_docs=len(documents))
            if not rep.ok:
                failed = "; ".join(f"{c.name}[{c.detail}]" for c in rep.checks if not c.ok)
                return None, {
                    "id": "fused-embed-incomplete",
                    "description": (
                        f"dense embed incomplete: coverage={rep.coverage:.4f} "
                        f"({rep.n_docs_embedded}/{rep.n_docs} docs); {failed}"
                    ),
                    "resolution": "re-embed (resumable: reopen + drain) until coverage==1.0",
                }
        else:
            engine.drain(timeout_s=600)  # FTS-only: no vector work, returns promptly

        adapter = FathomDBAdapter(
            engine,
            doc_id_of=lambda sh: cursor_to_doc.get(int(sh.id), str(sh.id)),
            use_graph_arm=False,
        )
        return adapter, None
    except Exception as exc:  # noqa: BLE001
        return None, {
            "id": "fathomdb-ingest-failed",
            "description": f"FathomDB open/ingest failed (use_embedder={use_embedder}): {exc}",
            "resolution": "UNRESOLVED — variant skipped",
        }


def build_variants(
    documents: dict[str, str],
    db_dir: Path,
    *,
    include_fused: bool = True,
) -> tuple[dict[str, RetrievalAdapter], list[dict[str, str]]]:
    """Build the three P0-A retrieval variants; degrade gracefully per arm."""
    systems: dict[str, RetrievalAdapter] = {}
    blockers: list[dict[str, str]] = []

    systems["naive_bm25"] = NaiveRAGAdapter(documents)

    fts, blk = _build_fathomdb_variant(
        documents,
        db_dir / "p0a_fts_only.sqlite",
        use_embedder=False,
        register_doc_vector_kind=False,
    )
    if fts is not None:
        systems["fathomdb_fts_only"] = fts
    if blk is not None:
        blk["id"] = "fts_only:" + blk["id"]
        blockers.append(blk)

    if include_fused:
        fused, blk2 = _build_fathomdb_variant(
            documents,
            db_dir / "p0a_fused.sqlite",
            use_embedder=True,
            register_doc_vector_kind=True,
        )
        if fused is not None:
            systems["fathomdb_fused"] = fused
        if blk2 is not None:
            blk2["id"] = "fused:" + blk2["id"]
            blockers.append(blk2)

    return systems, blockers


# --------------------------------------------------------------------------- #
# Airlock answerer (end-to-end loop)
# --------------------------------------------------------------------------- #


class AirlockAnswerer(BaseAnswerer):
    """Reader over the airlock OpenAI-compatible proxy (reuses the ELPS path).

    Uses the shared :class:`BaseAnswerer` prompt template, so every variant is
    read through the identical prompt (the identical-answerer invariant). The
    reader model is a constructor arg; base-url / key come from the ELPS env.
    """

    def __init__(self, model_id: str, *, timeout_s: float = 120.0) -> None:
        self.model_id = model_id
        self._timeout = timeout_s
        self.base_url = _AIRLOCK_BASE_URL
        self.api_key = _AIRLOCK_API_KEY

    @property
    def available(self) -> bool:
        return bool(self.base_url) and self.model_id not in ("", "<unset>")

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        import urllib.request

        payload = json.dumps(
            {
                "model": self.model_id,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0,
                "seed": 0,
            }
        ).encode("utf-8")
        req = urllib.request.Request(
            self.base_url.rstrip("/") + "/chat/completions",
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
        )
        with urllib.request.urlopen(req, timeout=self._timeout) as resp:  # noqa: S310
            body = json.loads(resp.read().decode("utf-8"))
        return normalize_answer(body["choices"][0]["message"]["content"])


# --------------------------------------------------------------------------- #
# The two loops
# --------------------------------------------------------------------------- #


def run_retrieval_loop(
    smoke: SmokeSet,
    systems: dict[str, RetrievalAdapter],
    *,
    ks: tuple[int, ...] = (5, 10, 20),
    graded_k: int = 10,
) -> dict[str, Any]:
    """LLM-free loop: per-variant per-class Recall@K + graded metrics.

    Retrieves once at ``max(ks)`` per (variant, question) and scores all K.
    """
    kmax = max(ks)
    out: dict[str, Any] = {}
    for name, adapter in systems.items():
        records: list[RetrievalRecord] = []
        for q in smoke.questions:
            hits = adapter.retrieve(q.question, kmax)
            ranked = tuple(h.doc_id for h in hits)
            records.append(
                RetrievalRecord(q.reporting_class, q.gold_sessions, ranked)
            )
        out[name] = aggregate(records, ks=ks, graded_k=graded_k)
    return out


@dataclass(frozen=True)
class AnswerRecord:
    """One (variant, question) outcome, normalized for scoring.

    ``answer`` is already passed through :func:`normalize_answer` — ``None`` means
    the reader abstained (or produced no usable content). ``cid`` keys the optional
    LLM-judge ``verdicts`` map (``"{variant}||{qid}"``)."""

    cid: str
    variant: str
    reporting_class: str
    is_abstention: bool
    gold_answer: str
    answer: Optional[str]


def score_answer(rec: AnswerRecord, *, verdict: Optional[bool] = None) -> float:
    """Score one record. Abstention query: correct iff the reader abstained.
    Positive query: correct iff a non-abstaining answer matches gold — by the LLM
    ``verdict`` when supplied, else the strict :func:`_match` substring check (a
    correct-but-rephrased answer omitting the gold string scores 0)."""
    if rec.is_abstention:
        return 1.0 if rec.answer is None else 0.0
    if rec.answer is None:
        return 0.0  # abstention miss on a positive query — counted, never skipped
    if verdict is not None:
        return 1.0 if verdict else 0.0
    return 1.0 if _match([rec.gold_answer], rec.answer) else 0.0


def score_answers(
    records: Sequence[AnswerRecord],
    *,
    verdicts: Optional[dict[str, bool]] = None,
) -> dict[str, Any]:
    """Per-variant per-class accuracy + overall + ``n_answered`` — the single readout
    both the batch (:func:`p0a_batch_e2e.score_e2e`) and sync (:func:`run_e2e_loop`)
    e2e paths share, so they cannot drift. ``verdicts`` (cid -> bool) plugs in the
    LLM judge; a cid missing from it falls back to :func:`_match`."""
    per_variant: dict[str, dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    answered: dict[str, int] = defaultdict(int)
    for r in records:
        if r.answer is not None:
            answered[r.variant] += 1
        v = None if verdicts is None else verdicts.get(r.cid)
        per_variant[r.variant][r.reporting_class].append(score_answer(r, verdict=v))
    out: dict[str, Any] = {}
    for name, pc in per_variant.items():
        out[name] = {
            "per_class_accuracy": {c: _mean(v) for c, v in pc.items()},
            "overall_accuracy": _mean([s for vv in pc.values() for s in vv]),
            "n_answered": answered[name],
        }
    return out


def run_e2e_loop(
    smoke: SmokeSet,
    systems: dict[str, RetrievalAdapter],
    readers: tuple[str, ...],
    *,
    context_k: int = 10,
) -> tuple[dict[str, Any], list[dict[str, str]]]:
    """Airlock loop: feed each variant's retrieved bodies to each reader; score
    answer accuracy (normalized substring match; abstention => correct iff the
    reader abstains)."""
    e2e: dict[str, Any] = {}
    blockers: list[dict[str, str]] = []

    for reader_id in readers:
        answerer = AirlockAnswerer(reader_id)
        if not answerer.available:
            blockers.append(
                {
                    "id": f"airlock-unavailable:{reader_id}",
                    "description": f"airlock reader {reader_id} unavailable at {_AIRLOCK_BASE_URL}",
                    "resolution": "UNRESOLVED — set ELPS_LLM_BASE_URL / ELPS_LLM_API_KEY",
                }
            )
            continue
        reader_block: dict[str, Any] = {}
        for name, adapter in systems.items():
            records: list[AnswerRecord] = []
            n_calls = 0
            errors = 0
            for q in smoke.questions:
                hits = adapter.retrieve(q.question, context_k)
                ctx = [h.body for h in hits if h.body]
                try:
                    ans = answerer.answer(q.question, ctx)
                    n_calls += 1
                except Exception as exc:  # noqa: BLE001
                    errors += 1
                    if errors == 1:
                        blockers.append(
                            {
                                "id": f"airlock-call-failed:{reader_id}",
                                "description": f"first reader call failed: {exc}",
                                "resolution": "partial — see e2e error counts",
                            }
                        )
                    continue
                records.append(
                    AnswerRecord(
                        cid=f"{name}||{q.qid}",
                        variant=name,
                        reporting_class=q.reporting_class,
                        is_abstention=q.is_abstention,
                        gold_answer=q.answer,
                        answer=ans,
                    )
                )
            scored = score_answers(records).get(
                name, {"per_class_accuracy": {}, "overall_accuracy": None, "n_answered": 0}
            )
            # n_calls/n_errors are sync-only network bookkeeping (batch has no
            # per-call exception); the scoring block is shared, so it cannot drift.
            reader_block[name] = {**scored, "n_calls": n_calls, "n_errors": errors}
        e2e[reader_id] = reader_block
    return e2e, blockers


# --------------------------------------------------------------------------- #
# G1 haystack measurement (plan §3.2 — validate the ~5,760 estimate)
# --------------------------------------------------------------------------- #


def measure_haystack(smoke: SmokeSet) -> dict[str, Any]:
    hs = list(smoke.per_question_h.values())
    n_q = len(hs)
    union = len(smoke.documents)
    avg_h = round(sum(hs) / n_q, 2) if n_q else None
    # Project the §3.2 estimate to 150 questions, holding the measured mean H.
    projected_150 = round(avg_h * 150) if avg_h else None
    return {
        "n_questions": n_q,
        "per_question_h_min": min(hs) if hs else None,
        "per_question_h_max": max(hs) if hs else None,
        "per_question_h_mean": avg_h,
        "smoke_union_size": union,
        "union_over_count": round(union / n_q, 2) if n_q else None,
        "projected_union_at_150q_if_disjoint": projected_150,
        "plan_estimate_at_150q": 5760,
        "note": (
            "If union_over_count << per_question_h_mean the haystacks overlap "
            "(distractor pools shared); if ~equal they are near-disjoint and the "
            "~5,760 estimate (38.4*150) is an undercount at the measured mean H."
        ),
    }


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #


def run_smoke(
    *,
    dataset_name: str = DEFAULT_DATASET,
    split: str = DEFAULT_SPLIT,
    per_class: int = 4,
    seed: int = DEFAULT_SEED,
    readers: tuple[str, ...] = DEFAULT_READERS,
    db_dir: Path = Path("/tmp"),
    run_e2e: bool = True,
    include_fused: bool = True,
) -> dict[str, Any]:
    t0 = time.time()
    blockers: list[dict[str, str]] = []

    smoke = load_lme_smoke(
        dataset_name, split, per_class=per_class, seed=seed, classes=SMOKE_CLASSES
    )
    g1 = measure_haystack(smoke)

    systems, build_blk = build_variants(smoke.documents, db_dir, include_fused=include_fused)
    blockers.extend(build_blk)

    retrieval = run_retrieval_loop(smoke, systems)

    e2e: dict[str, Any] = {}
    if run_e2e:
        e2e, e2e_blk = run_e2e_loop(smoke, systems, readers)
        blockers.extend(e2e_blk)

    class_counts: dict[str, int] = defaultdict(int)
    for q in smoke.questions:
        class_counts[q.reporting_class] += 1

    return {
        "slice": "0.8.1/p0-a",
        "mode": "smoke",
        "full_500_run": "NOT RUN — awaiting HITL approval",
        "dataset": dataset_name,
        "split": split,
        "seed": seed,
        "per_class_requested": per_class,
        "n_questions": len(smoke.questions),
        "class_counts": dict(class_counts),
        "question_ids": [q.qid for q in smoke.questions],
        "variants": sorted(systems.keys()),
        "readers_requested": list(readers),
        "g1_haystack_measurement": g1,
        "retrieval_loop": retrieval,
        "e2e_loop": e2e,
        "blockers_encountered": blockers,
        "elapsed_s": round(time.time() - t0, 1),
        "reserved_followups": [
            {
                "id": "dense-only-ablation",
                "description": "Dense-only arm (FTS off) requires an engine knob not "
                "exposed in Python; needs a Rust/engine slice.",
            },
            {
                "id": "rrf-weight-ablation-FIX-5",
                "description": "RRF weight tuning / FIX-5 3x text-weight cascade requires "
                "an engine knob not exposed in Python; needs a Rust/engine slice.",
            },
            {
                "id": "vector-kind-binding-gap",
                "description": "No production Python surface registers a node vector kind; "
                "fathomdb_fused used the test-hooks _configure_vector_kind_for_test seam. "
                "A binding to configure vector kinds is needed for a production fused path.",
            },
        ],
    }


def main(argv: Optional[list[str]] = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(description="P0-A base-retrieval ablation (LME)")
    parser.add_argument("--dataset", default=DEFAULT_DATASET)
    parser.add_argument("--split", default=DEFAULT_SPLIT)
    parser.add_argument("--per-class", type=int, default=4)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--db-dir", default="/tmp")
    parser.add_argument("--readers", default=",".join(DEFAULT_READERS))
    parser.add_argument("--no-e2e", action="store_true", help="skip the airlock end-to-end loop")
    parser.add_argument("--no-fused", action="store_true", help="skip the dense+FTS fused variant")
    parser.add_argument("--output", required=True, help="smoke result JSON path")
    parser.add_argument(
        "--full",
        action="store_true",
        help="HITL-GATED: run the full 500-question / 19,195-session benchmark. "
        "Default (absent) runs the small smoke. Do NOT pass without HITL approval.",
    )
    args = parser.parse_args(argv)

    if args.full:
        raise SystemExit(
            "--full is HITL-gated and not implemented in this slice: the full 500-Q / "
            "19,195-session run awaits HITL approval (plan §6 / mandate HARD limits)."
        )

    readers = tuple(r.strip() for r in args.readers.split(",") if r.strip())
    out = run_smoke(
        dataset_name=args.dataset,
        split=args.split,
        per_class=args.per_class,
        seed=args.seed,
        readers=readers,
        db_dir=Path(args.db_dir),
        run_e2e=not args.no_e2e,
        include_fused=not args.no_fused,
    )
    Path(args.output).write_text(json.dumps(out, indent=2), encoding="utf-8")
    print(f"[p0a] wrote {args.output}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
