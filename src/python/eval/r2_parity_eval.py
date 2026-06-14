"""R2 parity eval harness — identical-answerer protocol, per-class scoring.

Binding spec: ``dev/adr/ADR-0.8.1-ir-measure-eval-design.md §3``. Design memo:
``dev/design/slice-25-r2-design.md``.

The load-bearing invariant (ADR §3.2): the **same answerer** (same prompt
template, same context budget) answers questions retrieved by **all three**
systems. Adapters expose only ``retrieve(question, k)`` — they cannot build a
prompt — so a per-system prompt divergence is structurally impossible. The
harness owns the single :class:`BaseAnswerer` and routes every system's context
through it.

Footprint (ADR §3.6): the answerer LLM and Mem0-OSS are **test-infra** (BYO/local,
gated by ``R2_RUN``); FathomDB itself makes zero network calls (local SQLite).
"""

from __future__ import annotations

import json
import math
import os
import re
from collections import defaultdict
from collections.abc import Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any, Optional, Protocol, runtime_checkable

if TYPE_CHECKING:  # pragma: no cover - typing only
    from fathomdb.engine import Engine

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #

#: COR-2: the frozen corpus snapshot the parity numbers are pinned to.
CORPUS_HASH_PREFIX = "fe973fcd"

#: The five R2 query classes (ADR §3.4). Slice 30's go/no-go reads the middle
#: three (``temporal``/``multi_hop``/``knowledge_update``).
R2_CLASSES: tuple[str, ...] = (
    "factoid",
    "temporal",
    "multi_hop",
    "knowledge_update",
    "multi_session",
)

#: Gold-set ``query_class`` → reporting class. The frozen FathomDB gold carries
#: only ``exact_fact``/``exploratory``/``negative``; ``exact_fact`` maps onto the
#: R2 ``factoid`` class, the other two are carried as report-only extras. The four
#: memory classes have no gold in the frozen corpus (see the design memo §1).
GOLD_CLASS_MAP: dict[str, str] = {
    "exact_fact": "factoid",
    "factoid": "factoid",
    "exploratory": "exploratory",
    "negative": "negative",
    "temporal": "temporal",
    "multi_hop": "multi_hop",
    "knowledge_update": "knowledge_update",
    "multi_session": "multi_session",
}

_R2_RUN_ENV = "R2_RUN"


# --------------------------------------------------------------------------- #
# Data model
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class Hit:
    """A single retrieved candidate. ``doc_id`` is the *corpus* doc id (the join
    key against the gold set), not an engine-internal row cursor."""

    doc_id: str
    body: str
    score: float


@dataclass(frozen=True)
class GoldQuery:
    query_id: str
    question: str
    reporting_class: str
    answers: tuple[str, ...]
    gold_doc_ids: tuple[str, ...]

    @property
    def has_answer(self) -> bool:
        """True when an answer exists (positive query); False for the negative
        class (no answer exists — answering it is a false positive)."""
        return bool(self.answers) or bool(self.gold_doc_ids)


# --------------------------------------------------------------------------- #
# Answerer (identical across all systems)
# --------------------------------------------------------------------------- #


class BaseAnswerer:
    """Shared answerer. The prompt template lives here, never on an adapter, so
    every system answers through the identical template (ADR §3.2)."""

    PROMPT_TEMPLATE = (
        "You are a precise question-answering assistant. Answer the question using "
        "ONLY the provided context. If the context does not contain the answer, "
        "reply with exactly: I don't know.\n\n"
        "Context:\n{context}\n\nQuestion: {question}\nAnswer:"
    )
    model_id: str = "<unset>"

    @property
    def available(self) -> bool:
        """Whether this answerer can actually produce answers in this environment.
        When False, the harness emits retrieval-only metrics (accuracy = null)."""
        return True

    def build_prompt(self, question: str, context: list[str]) -> str:
        joined = "\n---\n".join(context) if context else "(no documents retrieved)"
        return self.PROMPT_TEMPLATE.format(context=joined, question=question)

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        prompt = self.build_prompt(question, context)
        return self._complete(prompt, question, context)

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        raise NotImplementedError


class StubAnswerer(BaseAnswerer):
    """Deterministic, LLM-free answerer for unit tests and stub runs: it echoes
    the top retrieved passage, or abstains (``None``) when nothing was retrieved.
    No network, no ``R2_RUN`` gate."""

    model_id = "stub-deterministic-v1"

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        if not context:
            return None
        return context[0]


@dataclass
class _Record:
    question: str
    template: str
    context: tuple[str, ...]


class RecordingAnswerer(StubAnswerer):
    """A :class:`StubAnswerer` that records every (question, template, context) it
    is asked — used by the identical-answerer constraint test (RED-1)."""

    model_id = "recording-stub-v1"

    def __init__(self) -> None:
        self.records: list[_Record] = []

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        self.records.append(
            _Record(question=question, template=self.PROMPT_TEMPLATE, context=tuple(context))
        )
        return super().answer(question, context)


class NullAnswerer(BaseAnswerer):
    """Retrieval-only sentinel: ``available`` is False, so the harness produces
    Evidence Recall@K but leaves ``answerer_accuracy``/``abstention_rate`` null
    (the §9 null-vs-zero distinction). Used when no answerer LLM is available."""

    model_id = "unavailable — see blockers_encountered"

    @property
    def available(self) -> bool:
        return False

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        raise RuntimeError("NullAnswerer cannot answer (no LLM available)")


class LLMAnswerer(BaseAnswerer):
    """BYO/local answerer over an OpenAI-compatible ``/chat/completions`` endpoint.

    Gated by ``R2_RUN=1`` plus ``R2_ANSWERER_MODEL`` and ``R2_ANSWERER_BASE_URL``
    (e.g. a local llama.cpp/ollama OpenAI shim). Temperature 0 + fixed seed for
    replay-determinism (design memo §4). FathomDB never calls this — it is the
    answerer for *all three* systems equally.
    """

    def __init__(self) -> None:
        self.base_url = os.environ.get("R2_ANSWERER_BASE_URL", "")
        self.api_key = os.environ.get("R2_ANSWERER_API_KEY", "")
        self.model_id = os.environ.get("R2_ANSWERER_MODEL", "<unset>")

    @property
    def available(self) -> bool:
        return (
            os.environ.get(_R2_RUN_ENV) == "1"
            and bool(self.base_url)
            and self.model_id != "<unset>"
        )

    def _complete(self, prompt: str, question: str, context: list[str]) -> Optional[str]:
        if os.environ.get(_R2_RUN_ENV) != "1":
            raise RuntimeError(f"{_R2_RUN_ENV} not set; set to 1 to run the full eval")
        if not self.available:
            raise RuntimeError(
                "LLMAnswerer not configured: set R2_ANSWERER_BASE_URL + R2_ANSWERER_MODEL"
            )
        import urllib.request  # local import: stdlib only, keeps the import graph clean

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
        with urllib.request.urlopen(req, timeout=120) as resp:  # noqa: S310 (BYO local endpoint)
            body = json.loads(resp.read().decode("utf-8"))
        text = body["choices"][0]["message"]["content"].strip()
        if not text or _normalize(text) in {"i dont know", "idk", ""}:
            return None
        return text


# --------------------------------------------------------------------------- #
# Retrieval adapters — retrieve(question, k) ONLY (no answer/prompt method)
# --------------------------------------------------------------------------- #


@runtime_checkable
class RetrievalAdapter(Protocol):
    name: str

    def retrieve(self, question: str, k: int) -> list[Hit]: ...


@dataclass
class StubAdapter:
    """Returns a fixed hit list per question — for unit tests / stub runs."""

    name: str
    hits_by_query: dict[str, list[Hit]] = field(default_factory=dict)

    def retrieve(self, question: str, k: int) -> list[Hit]:
        return list(self.hits_by_query.get(question, []))[:k]


class FathomDBAdapter:
    """Wraps the FathomDB Python SDK. Retrieval only — the engine is local SQLite
    and makes **zero** network calls (ADR §3.6). ``rerank_depth`` exercises the
    R1 cross-encoder seam; ``use_graph_arm`` the R3 arm (default off).

    ``doc_id_of`` maps an engine ``SearchHit`` back to its *corpus* doc id; the
    runner supplies it from the write-order → doc-id index built at ingest time.
    """

    name = "fathomdb"

    def __init__(
        self,
        engine: "Engine",
        *,
        doc_id_of: Optional[Any] = None,
        rerank_depth: int = 0,
        use_graph_arm: bool = False,
    ) -> None:
        self._engine = engine
        self._doc_id_of = doc_id_of
        self._rerank_depth = rerank_depth
        self._use_graph_arm = use_graph_arm

    def retrieve(self, question: str, k: int) -> list[Hit]:
        result = self._engine.search(
            question, rerank_depth=self._rerank_depth, use_graph_arm=self._use_graph_arm
        )
        hits: list[Hit] = []
        for sh in result.results[:k]:
            doc_id = str(self._doc_id_of(sh)) if self._doc_id_of is not None else str(sh.id)
            hits.append(Hit(doc_id=doc_id, body=sh.body, score=float(sh.score)))
        return hits


class NaiveRAGAdapter:
    """Floor baseline: flat BM25 over the same documents, no memory graph / update
    logic (ADR §3.3). Pure-Python (no extra dependency); deterministic given the
    corpus."""

    name = "naive_rag"

    def __init__(self, documents: dict[str, str], *, k1: float = 1.5, b: float = 0.75) -> None:
        self._k1 = k1
        self._b = b
        n_docs = max(len(documents), 1)
        self._doc_len: dict[str, int] = {}
        self._bodies: dict[str, str] = {}
        # Inverted index: term -> [(doc_id, term_freq)]. Scoring touches only docs
        # that contain a query term (standard BM25 structure) — identical scores
        # to a dense per-doc scan, but O(matching postings) not O(corpus) per query.
        self._postings: dict[str, list[tuple[str, int]]] = defaultdict(list)
        df: dict[str, int] = defaultdict(int)
        for doc_id, body in documents.items():
            self._bodies[doc_id] = body
            toks = _tokenize(body)
            self._doc_len[doc_id] = len(toks)
            counts: dict[str, int] = defaultdict(int)
            for t in toks:
                counts[t] += 1
            for t, f in counts.items():
                self._postings[t].append((doc_id, f))
                df[t] += 1
        self._avgdl = max(sum(self._doc_len.values()) / n_docs, 1e-9)
        self._idf = {t: math.log(1 + (n_docs - n + 0.5) / (n + 0.5)) for t, n in df.items()}

    def retrieve(self, question: str, k: int) -> list[Hit]:
        q_terms = set(_tokenize(question))
        scores: dict[str, float] = defaultdict(float)
        for t in q_terms:
            idf = self._idf.get(t)
            if not idf:
                continue
            coeff = self._k1 + 1
            for doc_id, f in self._postings.get(t, ()):
                dl = self._doc_len[doc_id]
                denom = f + self._k1 * (1 - self._b + self._b * dl / self._avgdl)
                scores[doc_id] += idf * (f * coeff) / denom
        ranked = sorted(scores.items(), key=lambda kv: kv[1], reverse=True)[:k]
        return [Hit(doc_id=doc_id, body=self._bodies.get(doc_id, ""), score=score) for doc_id, score in ranked]


class Mem0OSSAdapter:
    """Local Mem0-OSS baseline (``pip install mem0ai`` — NOT the cloud API; cloud
    is a footprint violation, ADR §3.6). ``mem0`` is imported lazily so the harness
    and its unit tests do not require it; ``available`` is False until both the
    library and a configured backend (LLM + embedder) exist.

    Mem0's ``add()`` extraction and ``search()`` embedding both require an LLM /
    embedding backend; with no local model available the adapter is inert and the
    live comparison runs FathomDB vs naive-RAG only (recorded as a blocker)."""

    name = "mem0_oss"

    def __init__(self, memory: Optional[Any] = None, *, user_id: str = "r2-eval") -> None:
        self._memory = memory
        self._user_id = user_id

    @property
    def available(self) -> bool:
        return self._memory is not None

    @staticmethod
    def try_build() -> Optional["Mem0OSSAdapter"]:
        """Best-effort local construction. Returns None (not raising) when mem0ai
        or its backend is unavailable, so the runner can record a clean blocker."""
        try:
            from mem0 import Memory  # type: ignore[import-not-found]
        except Exception:
            return None
        try:
            memory = Memory()  # default local config; needs a configured backend
        except Exception:
            return None
        return Mem0OSSAdapter(memory=memory)

    def ingest(self, documents: dict[str, str]) -> None:
        if self._memory is None:
            raise RuntimeError("Mem0OSSAdapter not available (mem0ai/backend missing)")
        for doc_id, body in documents.items():
            self._memory.add(body, user_id=self._user_id, metadata={"doc_id": doc_id})

    def retrieve(self, question: str, k: int) -> list[Hit]:
        if self._memory is None:
            raise RuntimeError("Mem0OSSAdapter not available (mem0ai/backend missing)")
        res = self._memory.search(query=question, user_id=self._user_id, limit=k)
        items = res.get("results", res) if isinstance(res, dict) else res
        hits: list[Hit] = []
        for item in items:
            doc_id = str(item.get("metadata", {}).get("doc_id", item.get("id", "")))
            hits.append(Hit(doc_id=doc_id, body=str(item.get("memory", "")), score=float(item.get("score", 0.0))))
        return hits


# --------------------------------------------------------------------------- #
# Scoring
# --------------------------------------------------------------------------- #


def _normalize(text: str) -> str:
    return re.sub(r"[^a-z0-9 ]+", " ", text.lower()).strip()


def _match(ground_truth: list[str], system_answer: str) -> bool:
    sa = _normalize(system_answer)
    if not sa:
        return False
    for gt in ground_truth:
        g = _normalize(gt)
        if not g:
            continue
        if g in sa or (len(g) <= 40 and sa in g):
            return True
    return False


def _tokenize(text: str) -> list[str]:
    return [t for t in re.findall(r"[a-z0-9]+", text.lower()) if len(t) >= 2]


class PerClassScorer:
    """Scores ``(ground_truth, system_answer)`` with abstention, stratified by
    class. Abstention against a positive query is a miss (0.0); answering a
    negative query is a false positive (0.0). Correct abstention on a negative
    query scores 1.0."""

    def __init__(self, extra_classes: tuple[str, ...] = ("exploratory", "negative")) -> None:
        self.classes: set[str] = set(R2_CLASSES) | set(extra_classes)

    def score_answer(
        self, ground_truth: list[str] | tuple[str, ...] | None, system_answer: Optional[str]
    ) -> float:
        gt = list(ground_truth or [])
        if gt:  # an answer exists
            if system_answer is None:
                return 0.0  # abstention miss — counted, never skipped
            return 1.0 if _match(gt, system_answer) else 0.0
        # negative class: no answer exists
        return 0.0 if system_answer is not None else 1.0


# --------------------------------------------------------------------------- #
# Harness
# --------------------------------------------------------------------------- #


def _parse_gold(raw: dict[str, Any]) -> list[GoldQuery]:
    out: list[GoldQuery] = []
    for q in raw.get("queries", []):
        gold_class = str(q.get("query_class", "")).strip()
        reporting = GOLD_CLASS_MAP.get(gold_class, gold_class or "unknown")
        answers = tuple(str(a) for a in (q.get("answers") or []))
        doc_ids: list[str] = []
        for ev in q.get("required_evidence") or []:
            did = ev.get("doc_id")
            if did:
                doc_ids.append(str(did))
        if not doc_ids:
            doc_ids = [str(d) for d in (q.get("expected_top_k_doc_ids") or [])]
        out.append(
            GoldQuery(
                query_id=str(q.get("query_id", "")),
                question=str(q.get("query", "")),
                reporting_class=reporting,
                answers=answers,
                gold_doc_ids=tuple(doc_ids),
            )
        )
    return out


def _mean(values: list[float]) -> Optional[float]:
    return round(sum(values) / len(values), 4) if values else None


def _delta(a: Optional[float], b: Optional[float]) -> Optional[float]:
    if a is None or b is None:
        return None
    return round(a - b, 4)


class R2Harness:
    """Runs the identical-answerer eval over a set of retrieval adapters.

    The COR-2 corpus-hash pin is asserted in ``__init__`` — *before* any number is
    produced — so a wrong/unpinned corpus cannot yield a citable result."""

    def __init__(self, gold_path: str | Path, answerer: BaseAnswerer) -> None:
        path = Path(gold_path)
        raw = json.loads(path.read_text(encoding="utf-8"))
        corpus_hash = str(raw.get("corpus_hash", ""))
        if not corpus_hash.startswith(CORPUS_HASH_PREFIX):
            raise ValueError(
                f"corpus_hash {corpus_hash!r} does not start with {CORPUS_HASH_PREFIX!r} "
                "(COR-2): refusing to produce eval numbers on an unpinned corpus"
            )
        self.corpus_hash = corpus_hash
        self.qrels_version = str(raw.get("qrels_version", ""))
        self.queries = _parse_gold(raw)
        self.answerer = answerer

    def run(
        self,
        systems: Mapping[str, RetrievalAdapter],
        *,
        k: int = 10,
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        queries = self.queries[:limit] if limit is not None else self.queries
        answerer_available = self.answerer.available
        scorer = PerClassScorer()

        report_classes = sorted(scorer.classes | {q.reporting_class for q in queries})

        # accumulators[system][class] -> {"recall": [...], "acc": [...], "abst": [...]}
        acc: dict[str, dict[str, dict[str, list[float]]]] = {
            sys_name: {cls: {"recall": [], "acc": [], "abst": []} for cls in report_classes}
            for sys_name in systems
        }
        n_per_class: dict[str, int] = {cls: 0 for cls in report_classes}

        for q in queries:
            n_per_class[q.reporting_class] += 1
            for sys_name, adapter in systems.items():
                hits = adapter.retrieve(q.question, k)
                bucket = acc[sys_name][q.reporting_class]
                if q.gold_doc_ids:
                    retrieved = {h.doc_id for h in hits}
                    bucket["recall"].append(
                        1.0 if all(g in retrieved for g in q.gold_doc_ids) else 0.0
                    )
                if answerer_available:
                    answer = self.answerer.answer(q.question, [h.body for h in hits if h.body])
                    bucket["abst"].append(1.0 if answer is None else 0.0)
                    if q.answers:  # only score accuracy when answer strings exist
                        bucket["acc"].append(scorer.score_answer(list(q.answers), answer))

        results: dict[str, dict[str, dict[str, Optional[float]]]] = {}
        for sys_name in systems:
            results[sys_name] = {}
            for cls in report_classes:
                b = acc[sys_name][cls]
                results[sys_name][cls] = {
                    "recall_at_k": _mean(b["recall"]),
                    "answerer_accuracy": _mean(b["acc"]) if answerer_available else None,
                    "abstention_rate": _mean(b["abst"]) if answerer_available else None,
                }

        deltas = self._delta_table(results, report_classes)

        return {
            "corpus_hash": self.corpus_hash,
            "qrels_version": self.qrels_version,
            "answerer_model": self.answerer.model_id,
            "answerer_available": answerer_available,
            "k": k,
            "n_queries_per_class": {cls: n_per_class.get(cls, 0) for cls in report_classes},
            "r2_results": results,
            "r2_per_class_deltas": deltas,
        }

    @staticmethod
    def _delta_table(
        results: dict[str, dict[str, dict[str, Optional[float]]]],
        report_classes: list[str],
    ) -> dict[str, dict[str, Optional[float]]]:
        fdb = results.get("fathomdb")
        mem0 = results.get("mem0_oss")
        naive = results.get("naive_rag")
        table: dict[str, dict[str, Optional[float]]] = {}
        for cls in report_classes:
            fa = fdb[cls]["answerer_accuracy"] if fdb else None
            fr = fdb[cls]["recall_at_k"] if fdb else None
            ma = mem0[cls]["answerer_accuracy"] if mem0 else None
            mr = mem0[cls]["recall_at_k"] if mem0 else None
            na = naive[cls]["answerer_accuracy"] if naive else None
            nr = naive[cls]["recall_at_k"] if naive else None
            table[cls] = {
                # primary delta = end-to-end answerer accuracy (null when no answerer)
                "fathomdb_minus_mem0": _delta(fa, ma),
                "fathomdb_minus_naive_rag": _delta(fa, na),
                # retrieval-only deltas (always available when both systems ran)
                "fathomdb_minus_mem0_recall_at_k": _delta(fr, mr),
                "fathomdb_minus_naive_rag_recall_at_k": _delta(fr, nr),
            }
        return table


def run_r2_eval(
    query_set: str | Path,
    systems: Mapping[str, RetrievalAdapter],
    answerer: BaseAnswerer,
    corpus_hash: str,
    *,
    k: int = 10,
    limit: Optional[int] = None,
) -> dict[str, Any]:
    """Convenience entry point. Validates the supplied ``corpus_hash`` (COR-2)
    before constructing the harness, then runs the eval."""
    if not corpus_hash.startswith(CORPUS_HASH_PREFIX):
        raise ValueError(
            f"corpus_hash {corpus_hash!r} does not start with {CORPUS_HASH_PREFIX!r} (COR-2)"
        )
    harness = R2Harness(gold_path=query_set, answerer=answerer)
    return harness.run(systems, k=k, limit=limit)


# --------------------------------------------------------------------------- #
# Live-run CLI:  python -m eval.r2_parity_eval --corpus-hash fe973fcd --output ...
# --------------------------------------------------------------------------- #


def _default_corpus_dir() -> Path:
    env = os.environ.get("FATHOMDB_CORPUS_DIR")
    if env:
        return Path(env)
    # eval/ -> src/python -> src -> repo
    return Path(__file__).resolve().parents[3] / "data" / "corpus-data"


def _load_documents(raw_dir: Path) -> dict[str, str]:
    docs: dict[str, str] = {}
    for jsonl in sorted(raw_dir.glob("*.jsonl")):
        with jsonl.open(encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if not line:
                    continue
                rec = json.loads(line)
                doc_id = rec.get("doc_id") or rec.get("id")
                if doc_id is None:
                    continue
                docs[str(doc_id)] = str(rec.get("body", ""))
    return docs


def _build_fathomdb(
    documents: dict[str, str],
    db_path: Path,
    *,
    batch: int = 500,
    elps_harness_cmd: Optional[list[str]] = None,
    elps_limit: int = 100,
    use_graph_arm: bool = False,
) -> tuple[Optional["FathomDBAdapter"], Optional[dict[str, str]]]:
    """Best-effort: ingest the corpus and return a live adapter.

    When ``elps_harness_cmd`` is supplied the first ``elps_limit`` documents
    are ingested via ``ingest_with_extractor`` (ELPS path; builds the graph
    arm).  Remaining documents fall back to plain ``write()``.  When
    ``elps_harness_cmd`` is None the original FTS-only path is used.

    Returns ``(adapter, None)`` on success or ``(None, blocker_dict)`` on
    any failure so the runner can degrade gracefully.
    """
    try:
        from fathomdb.engine import Engine
    except Exception as exc:  # noqa: BLE001 - report, don't crash the eval
        return None, {
            "id": "fathomdb-sdk-unavailable",
            "description": f"could not import fathomdb SDK ({exc}); run `pip install -e src/python`",
            "resolution": "UNRESOLVED — FathomDB arm skipped; naive-RAG retrieval reported",
        }
    if db_path.exists():
        db_path.unlink()
    try:
        # ELPS path uses the embedder so projection scheduler can drain the
        # edge_fact queue. Plain-write path stays FTS-only (no embedder needed).
        use_embedder = elps_harness_cmd is not None
        engine = Engine.open(str(db_path), use_default_embedder=use_embedder)
        cursor_to_doc: dict[int, str] = {}

        if elps_harness_cmd is not None:
            # ELPS path -------------------------------------------------------
            # Two-pass ingest for the ELPS sample:
            #   Pass 1 — write() for doc nodes: gives FTS searchability + cursor→doc_id
            #   Pass 2 — ingest_with_extractor: adds entity nodes + edges for graph arm
            # ingest_with_extractor creates entity/edge nodes but NOT source doc nodes,
            # so pass 1 is required for correct doc_id resolution in R2 eval.
            items = list(documents.items())
            elps_sample = items[:elps_limit]
            remainder = items[elps_limit:]

            # Pass 1: write doc nodes for all ELPS-sample docs
            for start in range(0, len(elps_sample), batch):
                chunk = elps_sample[start : start + batch]
                receipt = engine.write([{"kind": "doc", "body": body} for _, body in chunk])
                for (doc_id, _body), cursor in zip(chunk, receipt.row_cursors):
                    cursor_to_doc[int(cursor)] = doc_id

            # Pass 2: ELPS extraction adds entity nodes + edges (graph arm population)
            elps_docs = [
                {"source_doc_id": doc_id, "body": body} for doc_id, body in elps_sample
            ]
            engine.ingest_with_extractor(elps_harness_cmd, elps_docs)

            # Remainder docs: plain write only (no ELPS extraction)
            for start in range(0, len(remainder), batch):
                chunk = remainder[start : start + batch]
                receipt = engine.write([{"kind": "doc", "body": body} for _, body in chunk])
                for (doc_id, _body), cursor in zip(chunk, receipt.row_cursors):
                    cursor_to_doc[int(cursor)] = doc_id

            # Longer drain: embedder processes entity nodes from ELPS extraction
            engine.drain(timeout_s=600)

            adapter = FathomDBAdapter(
                engine,
                doc_id_of=lambda sh: cursor_to_doc.get(int(sh.id), str(sh.id)),
                use_graph_arm=use_graph_arm,
            )
        else:
            # Original FTS-only path ------------------------------------------
            items = list(documents.items())
            for start in range(0, len(items), batch):
                chunk = items[start : start + batch]
                receipt = engine.write([{"kind": "doc", "body": body} for _, body in chunk])
                for (doc_id, _body), cursor in zip(chunk, receipt.row_cursors):
                    cursor_to_doc[int(cursor)] = doc_id
            engine.drain(timeout_s=120)
            adapter = FathomDBAdapter(
                engine,
                doc_id_of=lambda sh: cursor_to_doc.get(int(sh.id), str(sh.id)),
                use_graph_arm=use_graph_arm,
            )

        return adapter, None
    except Exception as exc:  # noqa: BLE001
        return None, {
            "id": "fathomdb-ingest-failed",
            "description": f"FathomDB open/ingest failed: {exc}",
            "resolution": "UNRESOLVED — FathomDB arm skipped",
        }


def main(argv: Optional[list[str]] = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(description="R2 end-to-end parity eval runner")
    parser.add_argument("--corpus-hash", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--corpus-dir", default=None)
    parser.add_argument("--gold", default=None)
    parser.add_argument("--db-path", default="/tmp/r2-fathomdb-eval.sqlite")
    parser.add_argument("--k", type=int, default=10)
    parser.add_argument("--limit", type=int, default=None, help="cap queries (debug)")
    parser.add_argument("--no-fathomdb", action="store_true")
    # Option 2 arguments
    parser.add_argument(
        "--elps-harness",
        default=None,
        help="Path to elps_live_harness.py; if set, ingest sample via ingest_with_extractor",
    )
    parser.add_argument(
        "--elps-limit",
        type=int,
        default=100,
        help="Max docs to ingest via ELPS (Option 2 sample size)",
    )
    parser.add_argument(
        "--use-graph-arm",
        action="store_true",
        default=False,
        help="Set use_graph_arm=True in FathomDBAdapter",
    )
    parser.add_argument(
        "--extra-gold",
        default=None,
        help="Path to an additional gold file to merge with --gold (for memory-class QA)",
    )
    args = parser.parse_args(argv)

    if not args.corpus_hash.startswith(CORPUS_HASH_PREFIX):
        raise SystemExit(f"--corpus-hash must start with {CORPUS_HASH_PREFIX} (COR-2)")

    corpus_dir = Path(args.corpus_dir) if args.corpus_dir else _default_corpus_dir()
    gold_path = Path(args.gold) if args.gold else corpus_dir / "eval" / "ir_gold" / "all.gold.json"
    raw_dir = corpus_dir / "raw"

    blockers: list[dict[str, str]] = []
    print(f"[runner] corpus_dir={corpus_dir} gold={gold_path}")
    documents = _load_documents(raw_dir)
    print(f"[runner] loaded {len(documents)} documents")

    # Change 3: restrict to ELPS sample when --elps-harness is active (fair comparison)
    if args.elps_harness:
        items = list(documents.items())[: args.elps_limit]
        documents = dict(items)
        print(f"[runner] ELPS mode: restricted to {len(documents)} documents (--elps-limit)")

    systems: dict[str, RetrievalAdapter] = {}

    # naive-RAG (always; pure-Python BM25, no deps, no LLM)
    systems["naive_rag"] = NaiveRAGAdapter(documents)
    print("[runner] naive_rag adapter ready")

    # FathomDB (best-effort; ELPS path or FTS-only path)
    if not args.no_fathomdb:
        # Change 4: wire new args through to _build_fathomdb
        elps_cmd = ["python", args.elps_harness] if args.elps_harness else None
        fdb, blk = _build_fathomdb(
            documents,
            Path(args.db_path),
            elps_harness_cmd=elps_cmd,
            elps_limit=args.elps_limit,
            use_graph_arm=args.use_graph_arm,
        )
        if fdb is not None:
            systems["fathomdb"] = fdb
            print("[runner] fathomdb adapter ready")
        if blk is not None:
            blockers.append(blk)
            print(f"[runner] fathomdb BLOCKED: {blk['description']}")

    # Mem0-OSS (best-effort local; needs mem0ai + a configured backend)
    mem0 = Mem0OSSAdapter.try_build()
    if mem0 is not None and mem0.available:
        try:
            mem0.ingest(documents)
            systems["mem0_oss"] = mem0
            print("[runner] mem0_oss adapter ready")
        except Exception as exc:  # noqa: BLE001
            blockers.append(
                {
                    "id": "mem0-oss-unavailable",
                    "description": f"mem0.ingest() failed: {exc}",
                    "resolution": "UNRESOLVED — mem0 arm skipped; check backend config",
                }
            )
            print(f"[runner] mem0_oss BLOCKED (ingest failed): {exc}")
    else:
        blockers.append(
            {
                "id": "mem0-oss-unavailable",
                "description": (
                    "mem0ai (or its LLM+embedding backend) is not available locally; "
                    "Memory.add() extraction requires a model. Mem0 arm skipped."
                ),
                "resolution": "UNRESOLVED — install mem0ai + configure a local backend",
            }
        )
        print("[runner] mem0_oss BLOCKED (no local mem0 backend)")

    # answerer
    answerer: BaseAnswerer = LLMAnswerer()
    if not answerer.available:
        answerer = NullAnswerer()
        blockers.append(
            {
                "id": "answerer-llm-unavailable",
                "description": (
                    "no answerer LLM available (R2_RUN/R2_ANSWERER_* unset, no local "
                    "model). Retrieval-only metrics (Evidence Recall@K) reported; "
                    "answerer_accuracy is null."
                ),
                "resolution": "UNRESOLVED — set R2_ANSWERER_BASE_URL + R2_ANSWERER_MODEL + R2_RUN=1",
            }
        )
        print("[runner] answerer BLOCKED — retrieval-only run")

    harness = R2Harness(gold_path=gold_path, answerer=answerer)

    # Change 1: merge extra gold if provided
    if args.extra_gold:
        extra_gold_path = Path(args.extra_gold)
        extra_raw = json.loads(extra_gold_path.read_text(encoding="utf-8"))
        extra_queries = _parse_gold(extra_raw)
        harness.queries = list(harness.queries) + extra_queries  # type: ignore[assignment]
        print(f"[runner] merged {len(extra_queries)} extra queries from {extra_gold_path}")

    out = harness.run(systems, k=args.k, limit=args.limit)
    out["blockers_encountered"] = blockers
    out["systems_run"] = sorted(systems.keys())

    Path(args.output).write_text(json.dumps(out, indent=2), encoding="utf-8")
    print(f"[runner] wrote {args.output}")
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
