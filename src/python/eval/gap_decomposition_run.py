"""0.8.3 gap-decomposition runner — retrieval / distilled-form / Mem0-residual.

Extends the R2 identical-answerer harness (:mod:`eval.r2_parity_eval`) with the
three NEW arms of the bounded 3-way decomposition (design
``dev/design/0.8.3-gap-decomposition-probe.md`` §2):

* ``oracle_raw``         — the query's gold doc(s), raw, same 32k fitter → reader.
* ``oracle_distilled``   — query-/answer-BLIND one-line distillation of each gold doc.
* ``fathomdb_distilled`` — the SAME blind distiller over FathomDB's retrieved bodies.

The ``fathomdb`` + ``mem0_oss`` cells are **reused** from D0b's per-question
checkpoint (already paid); the runner HARD-STOPS rather than fall back to D0b's
aggregate class-means (paired CIs against the oracle require per-question cells).

Budget safety (codex BLOCK Q5 — implemented LITERALLY):

* ``gpt-5.4`` pricing is **pinned**; an un-pinned priced model **fails closed**
  (:class:`UnpinnedPricing`) — never a silent default.
* the **D0b $10.7479** opening balance is carried; a **pre-call projected** cost
  check keeps ``ledger + projected <= $30`` for the reader AND the distiller,
  raising :class:`BudgetExceeded` BEFORE the call that would exceed (NOT the
  after-call check d0b uses).

The distiller is **corpus-level + query-/answer-BLIND** (design §4): each candidate
doc is distilled ONCE from a generic, body-only prompt; the cache is keyed by
``doc_id`` and selected at eval time. Backend = local Qwen ($0) or a
``--max-usd``-capped cheap model, **fail-closed**.

Pure helpers (``price_for`` / :class:`BudgetLedger` / ``load_d0b_cells`` /
``oracle_context`` / component-delta builders / ``answer_retention``) are
import-light + backend-free so the unit tests run with fakes (no DB, no LLM, no
``mem0``, no ``fathomdb`` extension build).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import time
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, Optional, Protocol

from eval.d0b_parity_run import CHEAP_READER_DEFAULT, class_delta, fit_context
from eval.gap_decomposition_rule import (
    COMPONENTS,
    FIT_COVERAGE_MIN,
    GapDecision,
    decide_gap_decomposition,
)
from eval.m1_verdict_run import _atomic_write_json
from eval.r2_parity_eval import (
    BaseAnswerer,
    GoldQuery,
    PerClassScorer,
    load_repin_gold,
)

# --------------------------------------------------------------------------- #
# Frozen wiring (design §2/§3/§4).
# --------------------------------------------------------------------------- #

#: The four agentic-memory classes scored (same as decision_rule_083).
GAP_CLASSES: tuple[str, ...] = ("factoid", "knowledge_update", "multi_session", "temporal")

#: The three NEW priced arms this runner produces (reader calls).
NEW_ARMS: tuple[str, ...] = ("oracle_raw", "oracle_distilled", "fathomdb_distilled")
#: The two arms reused from D0b's per-question checkpoint (already paid).
REUSED_ARMS: tuple[str, ...] = ("fathomdb", "mem0_oss")

#: component → (minuend_arm, subtrahend_arm). All three reference an oracle arm, so
#: the decomposition is computed on the SAME oracle-fit-complete subset (keeps the
#: algebraic identity R + F + Resid == acc_mem0 − acc_fathomdb on that subset).
_COMPONENT_ARMS: dict[str, tuple[str, str]] = {
    "RETRIEVAL": ("oracle_raw", "fathomdb"),
    "DISTILLED_FORM": ("oracle_distilled", "oracle_raw"),
    "MEM0_RESIDUAL": ("mem0_oss", "oracle_distilled"),
}

#: The SAME 32 000-char fitter budget as D0b (so a form difference is not a fitter
#: artifact; design §2).
CONTEXT_CHAR_BUDGET = 32000

#: Priced reader (design §3). The airlock proxy serves this id.
STRONG_READER_DEFAULT = "gpt-5.4"

#: PINNED pricing ($ / 1M tokens) — the exact rates the D0b n606 run recorded.
#: NO default fallback: an un-pinned priced model fails closed (codex BLOCK Q5).
#: The cheap distiller ids (``gemini-flash-lite`` / ``gemini-3.1-flash-lite``,
#: the rates m1/d0b cheap-validate use) are pinned too so the ledger cap on the
#: distiller stays projectable; only a truly-unpinned model fails closed.
PRICE_PER_1M: dict[str, tuple[float, float]] = {
    "gpt-5.4": (1.25, 5.00),
    "gemini-flash-lite": (0.05, 0.20),
    "gemini-3.1-flash-lite": (0.05, 0.20),
}

#: The D0b spend carried as this ledger's opening balance (design §4).
D0B_OPENING_BALANCE_USD = 10.7479
#: The HITL-approved hard cap (design §4).
HARD_CAP_USD = 30.0
#: The conservative per-call max output tokens used in the PRE-call projection.
DEFAULT_MAX_OUTPUT_TOKENS = 512
#: Default paired-bootstrap resample count (deterministic given seed).
DEFAULT_N_BOOT = 2000

#: The published verdict token for a NON-CITABLE run (capped / incomplete). A run
#: in this state must NEVER emit a RETRIEVAL/DISTILLED_FORM/MEM0_RESIDUAL DOMINANT
#: result — an incomplete priced run is non-citable until completeness is satisfied
#: (codex §9 P1: the resilience contract).
ABORTED_VERDICT = "ABORTED_INCOMPLETE"
#: Answer-completeness floor: the fraction of the input's answerable questions whose
#: THREE new arms were all processed. Below this (or on a budget abort) the run is
#: non-citable. 1.0 = every answerable question's new arms were produced/resumed.
ANSWER_COMPLETENESS_MIN = 1.0


# --------------------------------------------------------------------------- #
# Errors (loud, never silent).
# --------------------------------------------------------------------------- #


class UnpinnedPricing(RuntimeError):
    """A priced model has no pinned price metadata — fail closed (codex BLOCK Q5)."""


class BudgetExceeded(RuntimeError):
    """A pre-call projection would push the ledger over the hard cap — halt BEFORE
    the call (codex BLOCK Q5: the pre-call check, not d0b's after-call check)."""


class CheckpointMissingRecords(RuntimeError):
    """The D0b checkpoint lacks per-question ``records`` — HARD-STOP (design §4: no
    aggregate fallback; paired CIs against the oracle require per-question cells)."""


# --------------------------------------------------------------------------- #
# Pricing + budget (pre-call projection; codex BLOCK Q5).
# --------------------------------------------------------------------------- #


def price_for(model: str) -> tuple[float, float]:
    """Return ``(price_in_per_1m, price_out_per_1m)`` for a priced ``model``.

    **Fail closed**: an un-pinned priced model raises :class:`UnpinnedPricing` —
    never a silent default (a $-cap is unenforceable without pinned pricing)."""
    price = PRICE_PER_1M.get(model)
    if price is None:
        raise UnpinnedPricing(
            f"no pinned pricing for priced model {model!r}; refusing to project a "
            f"$-cap on a default (codex BLOCK Q5). Pinned models: {sorted(PRICE_PER_1M)}"
        )
    return price


def resolve_reader(mode: str, reader: Optional[str]) -> str:
    """Default the reader by ``mode`` (codex §9 P2), mirroring the D0b runner:
    ``cheap`` → :data:`CHEAP_READER_DEFAULT`, ``full`` → :data:`STRONG_READER_DEFAULT`.

    An explicit ``--reader`` always wins. The bug this fixes: ``--mode cheap`` with no
    ``--reader`` still picked the priced ``gpt-5.4`` → a cheap-validate pass spent
    priced budget."""
    if reader:
        return reader
    return CHEAP_READER_DEFAULT if mode == "cheap" else STRONG_READER_DEFAULT


def resolve_distiller_model(distiller: Optional[str], reader: str) -> str:
    """Resolve the corpus distiller model (design §4): a CHEAP / local model,
    **never** the priced strong reader.

    Defaults to :data:`CHEAP_READER_DEFAULT` (the gemini-flash-lite id m1/d0b
    cheap-validate use). **Fail-closed**: if the resolved distiller equals the priced
    strong reader (:data:`STRONG_READER_DEFAULT`, e.g. ``gpt-5.4``), raise
    :class:`SystemExit` — **independent of the reader / mode** (codex §9 P2). The old
    guard only fired when ``distiller == reader`` AND the reader was strong, so a
    cheap-resolved reader (e.g. ``--mode cheap``) let ``--distiller gpt-5.4`` slip a
    priced distiller through; the corpus distiller must never be the strong/priced
    model in ANY mode (the flagged placeholder ``distiller_model = reader`` is the bug
    this guards)."""
    model = distiller or CHEAP_READER_DEFAULT
    if model == STRONG_READER_DEFAULT:
        raise SystemExit(
            f"[GAPDECOMP][STOP] distiller {model!r} is the priced strong reader "
            f"{STRONG_READER_DEFAULT!r}; the corpus distiller must be a cheap/local "
            f"model, NEVER the strong/priced model — in ANY mode (codex §9 P2; reader "
            f"resolved to {reader!r}). Pass --distiller with a cheap id "
            f"(default {CHEAP_READER_DEFAULT!r})."
        )
    return model


def estimate_tokens(text: str) -> int:
    """Conservative token estimate (~4 chars/token, rounded up) for the PRE-call
    projection. The projection is intentionally an over-estimate (it also assumes
    the full ``max_output_tokens``), so the pre-call guard errs toward halting."""
    return math.ceil(len(text) / 4) if text else 0


class BudgetLedger:
    """A $ ledger with a **pre-call** projected-cost guard (codex BLOCK Q5).

    Carries an ``opening_balance_usd`` (the D0b spend) and a ``hard_cap_usd``. Before
    each priced call the caller invokes :meth:`guard` with the (estimated) prompt
    tokens; the projection adds the FULL ``max_output_tokens`` and raises
    :class:`BudgetExceeded` if ``spent + projected > hard_cap`` — *before* the call.
    :meth:`record` then books the call's ACTUAL token cost (fail-closed pricing)."""

    def __init__(
        self,
        *,
        opening_balance_usd: float = D0B_OPENING_BALANCE_USD,
        hard_cap_usd: float = HARD_CAP_USD,
        max_output_tokens: int = DEFAULT_MAX_OUTPUT_TOKENS,
    ) -> None:
        self.opening_balance_usd = float(opening_balance_usd)
        self.hard_cap_usd = float(hard_cap_usd)
        self.max_output_tokens = int(max_output_tokens)
        self._spent = float(opening_balance_usd)

    @property
    def spent(self) -> float:
        return round(self._spent, 6)

    @property
    def remaining(self) -> float:
        return round(self.hard_cap_usd - self._spent, 6)

    def restore_spent(self, spent_usd: float) -> None:
        """Set the running total to a persisted cumulative spend (codex §9 P1#1).

        On a checkpoint-resume the ledger MUST carry forward the spend already paid
        in prior processes (new-arm reader + distiller calls), so the ``$30`` cap is
        per-EXPERIMENT, not per-PROCESS. The persisted value is the cumulative
        :attr:`spent` (already inclusive of the D0b opening balance), so this is a
        plain assignment, never an addition (no double-count)."""
        self._spent = float(spent_usd)

    def project(self, model: str, prompt_tokens: int) -> float:
        """Projected cost of one call: ``prompt_tokens`` + the full
        ``max_output_tokens`` at the model's pinned price (fail-closed)."""
        pin, pout = price_for(model)
        return prompt_tokens / 1e6 * pin + self.max_output_tokens / 1e6 * pout

    def guard(self, model: str, prompt_tokens: int) -> float:
        """Raise :class:`BudgetExceeded` if this call would push the ledger over the
        cap — BEFORE the call. Returns the projection when it is within budget."""
        proj = self.project(model, prompt_tokens)
        if self._spent + proj > self.hard_cap_usd:
            raise BudgetExceeded(
                f"pre-call projection ${proj:.4f} on top of spent ${self._spent:.4f} "
                f"would exceed the ${self.hard_cap_usd:.2f} cap (model {model!r}, "
                f"~{prompt_tokens} prompt tok) — halting BEFORE the call"
            )
        return proj

    def record(self, model: str, prompt_tokens: int, completion_tokens: int) -> float:
        """Book one call's ACTUAL cost (fail-closed pricing). Returns the new total."""
        pin, pout = price_for(model)
        self._spent += prompt_tokens / 1e6 * pin + completion_tokens / 1e6 * pout
        return self.spent


# --------------------------------------------------------------------------- #
# D0b per-question cell reuse — checkpoint or HARD-STOP (design §4).
# --------------------------------------------------------------------------- #


def load_d0b_cells(
    checkpoint_path: str | Path,
    *,
    arms: Sequence[str] = REUSED_ARMS,
) -> dict[tuple[str, str], dict[str, Any]]:
    """Load D0b per-question ``fathomdb`` + ``mem0_oss`` cells → ``{(qid,arm): {acc,answer}}``.

    **HARD-STOP**: if the checkpoint lacks a non-empty per-question ``records`` list
    (e.g. it is a D0b *aggregate* artifact like ``0.8.3-d0b-parity-n606.json`` that
    carries only ``accuracy_deltas`` class-means), raise :class:`CheckpointMissingRecords`
    — never fall back to the aggregate (paired CIs against the oracle require
    per-question cells; design §4)."""
    data = json.loads(Path(checkpoint_path).read_text(encoding="utf-8"))
    records = data.get("records")
    if not records or not isinstance(records, list):
        raise CheckpointMissingRecords(
            f"D0b checkpoint {str(checkpoint_path)!r} has no per-question 'records' — "
            "cannot reuse the paid fathomdb+mem0 cells. HARD-STOP (no aggregate "
            "fallback; design §4). Re-run D0b with per-question checkpointing, or "
            "supply 0.8.3-d0b-parity-v2.checkpoint.json with per-question records."
        )
    cells: dict[tuple[str, str], dict[str, Any]] = {}
    for r in records:
        qid = r.get("qid")
        if qid is None:
            continue
        acc = r.get("acc") or {}
        ans = r.get("answers") or {}
        for arm in arms:
            if arm in acc or arm in ans:
                cells[(str(qid), arm)] = {"acc": acc.get(arm), "answer": ans.get(arm)}
    return cells


# --------------------------------------------------------------------------- #
# Oracle context + oracle_fit_complete (codex v2 BLOCK Q1/Q6).
# --------------------------------------------------------------------------- #


def oracle_context(
    gold_doc_ids: Sequence[str],
    documents: Mapping[str, str],
    *,
    budget: Optional[int] = CONTEXT_CHAR_BUDGET,
) -> tuple[list[str], bool]:
    """Build the raw-gold oracle context + the ``oracle_fit_complete`` flag.

    Returns ``(fitted_context, complete)`` where ``complete`` is True iff **every**
    required gold doc id is present in ``documents`` AND all gold bodies are included
    **untruncated** by the ``budget`` fitter. A missing gold doc, an empty gold set,
    or any truncated gold body ⇒ ``complete = False`` (the question is excluded from
    the decomposition + reported separately; an unfit oracle is a packaging limit,
    NOT an answer-formation failure)."""
    if not gold_doc_ids:
        return [], False
    all_present = all(g in documents for g in gold_doc_ids)
    bodies = [documents[g] for g in gold_doc_ids if g in documents]
    fitted = fit_context(bodies, budget)
    untruncated = len(fitted) == len(bodies) and all(
        fitted[i] == bodies[i] for i in range(len(bodies))
    )
    return fitted, bool(all_present and bodies and untruncated)


def distilled_context(
    doc_ids: Sequence[str],
    distill_cache: Mapping[str, Mapping[str, Any]],
    *,
    budget: Optional[int] = CONTEXT_CHAR_BUDGET,
) -> list[str]:
    """Build a distilled context from the corpus-level distill cache (selected by
    id at eval time — never label-selected distillation; design §4)."""
    bodies = [
        str(distill_cache[d]["distilled"])
        for d in doc_ids
        if d in distill_cache and distill_cache[d].get("distilled") is not None
    ]
    return fit_context(bodies, budget)


# --------------------------------------------------------------------------- #
# Corpus-level, query-/answer-BLIND distiller (design §4).
# --------------------------------------------------------------------------- #


class DistillerClient(Protocol):
    """A backend that turns a generic prompt into a one-line distillation. Real:
    local Qwen ($0) or a ``--max-usd``-capped cheap model. Tests inject a fake."""

    model_id: str

    def complete(self, prompt: str) -> str: ...


class BlindDistiller:
    """Corpus-level, **query-/answer-blind** one-line distiller.

    :meth:`distill` takes ONLY a document body — it is *structurally* blind to the
    query and the gold answers (they are not in scope at the corpus-distillation
    layer). The generic prompt carries no query, no answers, no labels (design §4;
    the ``Mem0-FORM`` label this produces stays PROVISIONAL — a generic distiller is
    NOT Mem0's memory units, codex Q4)."""

    PROMPT_TEMPLATE = (
        "Summarize the following document into a single concise line that captures "
        "its key facts. Do not add information that is not present in the document.\n\n"
        "Document:\n{body}\n\nOne-line summary:"
    )

    def __init__(self, client: DistillerClient) -> None:
        self._client = client

    @property
    def model_id(self) -> str:
        return self._client.model_id

    def build_prompt(self, body: str) -> str:
        return self.PROMPT_TEMPLATE.format(body=body)

    def distill(self, body: str) -> str:
        """Distill ONE document body (blind to any query/answer). Returns one line."""
        out = self._client.complete(self.build_prompt(body))
        return " ".join(str(out).split())


class RawCompletionDistillerClient:
    """A :class:`DistillerClient` that sends the distill prompt as a **RAW**
    chat/completions user message — NO QA answer template, NO empty-context "I don't
    know" abstention instruction (codex §9 P1#2).

    The flagged seam wrapped the distill prompt in :class:`BaseAnswerer`'s QA
    template (``answer ONLY from the context / reply exactly: I don't know``) by
    calling ``answerer.answer(prompt, [])``; a real cheap model then returned
    QA-shaped abstentions, corrupting BOTH distilled arms. This client instead calls
    the answerer's underlying chat/completions path directly with the distill prompt
    as the sole user message, reusing the cost-tracking + 429/5xx backoff seam. It
    stays query-/answer-blind and ledger-capped (the cap is enforced by
    :func:`distill_corpus`'s pre-call guard, unchanged)."""

    def __init__(self, answerer: BaseAnswerer) -> None:
        self._answerer = answerer
        self.model_id = str(getattr(answerer, "model_id", "<unset>"))

    def complete(self, prompt: str) -> str:
        # `_complete` posts the prompt verbatim as the user message (no template);
        # question/context are ignored by the airlock answerers (only StubAnswerer
        # subclasses read them), so the distill prompt is sent RAW.
        out = self._answerer._complete(prompt, "", [])
        return out or ""


def _body_hash(body: str) -> str:
    return hashlib.sha256(body.encode("utf-8")).hexdigest()[:16]


def distill_corpus(
    documents: Mapping[str, str],
    distiller: BlindDistiller,
    *,
    cache_path: Optional[Path] = None,
    ledger: Optional[BudgetLedger] = None,
    priced_model: Optional[str] = None,
) -> dict[str, dict[str, Any]]:
    """Distill EVERY candidate doc ONCE (query-/answer-blind) → a frozen, resumable
    ``{doc_id: {distilled, prompt, model, hash}}`` cache (design §4).

    When ``ledger`` + ``priced_model`` are given, the distiller is **$-capped**: the
    PRE-call projection guards each distill (raises :class:`BudgetExceeded` before a
    call that would exceed). A local-Qwen / $0 distiller passes ``ledger=None``."""
    cache: dict[str, dict[str, Any]] = {}
    if cache_path is not None and Path(cache_path).exists():
        cache = json.loads(Path(cache_path).read_text(encoding="utf-8"))
    for doc_id, body in documents.items():
        if doc_id in cache:
            continue
        prompt = distiller.build_prompt(body)
        if ledger is not None and priced_model is not None:
            ledger.guard(priced_model, estimate_tokens(prompt))
        distilled = distiller.distill(body)
        if ledger is not None and priced_model is not None:
            ledger.record(priced_model, estimate_tokens(prompt), estimate_tokens(distilled))
        cache[doc_id] = {
            "distilled": distilled,
            "prompt": prompt,
            "model": distiller.model_id,
            "hash": _body_hash(body),
        }
        if cache_path is not None:
            _atomic_write_json(Path(cache_path), cache)
    return cache


# --------------------------------------------------------------------------- #
# Per-call reader with the PRE-call budget guard.
# --------------------------------------------------------------------------- #


def answer_with_budget(
    answerer: BaseAnswerer,
    *,
    reader: str,
    question: str,
    context: list[str],
    ledger: BudgetLedger,
) -> Optional[str]:
    """Run ONE reader call behind the pre-call budget guard.

    Estimates the prompt tokens from the built prompt, calls :meth:`BudgetLedger.guard`
    (which raises :class:`BudgetExceeded` BEFORE the call when it would exceed the
    cap), then calls the answerer and books the actual token cost. The answerer's
    measured tokens are used when it exposes ``prompt_tokens`` / ``completion_tokens``
    (e.g. :class:`eval.m1_baseline_run.CostTrackingAnswerer`); else the estimate."""
    prompt = answerer.build_prompt(question, context)
    est_prompt = estimate_tokens(prompt)
    ledger.guard(reader, est_prompt)  # raises BEFORE the call if it would exceed
    before_p = int(getattr(answerer, "prompt_tokens", 0) or 0)
    before_c = int(getattr(answerer, "completion_tokens", 0) or 0)
    ans = answerer.answer(question, context)
    after_p = int(getattr(answerer, "prompt_tokens", 0) or 0)
    after_c = int(getattr(answerer, "completion_tokens", 0) or 0)
    used_p = (after_p - before_p) if after_p > before_p else est_prompt
    used_c = (after_c - before_c) if after_c >= before_c and after_c != before_c else estimate_tokens(ans or "")
    ledger.record(reader, used_p, used_c)
    return ans


# --------------------------------------------------------------------------- #
# Component deltas + fit coverage + retention diagnostic.
# --------------------------------------------------------------------------- #


def component_paired_deltas(
    records: Sequence[Mapping[str, Any]],
    *,
    component: str,
    cls: str,
    fit_required: bool = True,
) -> list[float]:
    """Per-question paired ``minuend − subtrahend`` accuracy deltas for ``component``
    in ``cls``. A question contributes ONLY when both arms carry a non-``None`` acc
    AND (when ``fit_required``) its oracle context fit untruncated (design §2: unfit
    oracle questions are excluded from the decomposition)."""
    minuend, subtrahend = _COMPONENT_ARMS[component]
    out: list[float] = []
    for r in records:
        if r.get("reporting_class") != cls:
            continue
        if fit_required and not r.get("oracle_fit_complete"):
            continue
        acc = r.get("acc") or {}
        tv = acc.get(minuend)
        cv = acc.get(subtrahend)
        if tv is not None and cv is not None:
            out.append(float(tv) - float(cv))
    return out


def per_component_table(
    records: Sequence[Mapping[str, Any]],
    *,
    classes: Sequence[str] = GAP_CLASSES,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
    fit_required: bool = True,
) -> dict[str, dict[str, dict[str, Any]]]:
    """``{class: {component: class_delta}}`` over every class + component, plus a
    ``"pooled"`` row over all classes."""
    out: dict[str, dict[str, dict[str, Any]]] = {}
    for cls in list(classes) + ["pooled"]:
        out[cls] = {}
        for comp in COMPONENTS:
            if cls == "pooled":
                deltas: list[float] = []
                for c in classes:
                    deltas += component_paired_deltas(
                        records, component=comp, cls=c, fit_required=fit_required
                    )
            else:
                deltas = component_paired_deltas(
                    records, component=comp, cls=cls, fit_required=fit_required
                )
            out[cls][comp] = class_delta(deltas, n_boot=n_boot, seed=seed)
    return out


def class_fit_coverage(records: Sequence[Mapping[str, Any]], cls: str) -> float:
    """Fraction of ``cls``'s answerable questions whose oracle context fit
    untruncated (``oracle_fit_complete``). 0.0 when the class has no questions."""
    rs = [r for r in records if r.get("reporting_class") == cls and r.get("has_answers")]
    if not rs:
        return 0.0
    return round(sum(1 for r in rs if r.get("oracle_fit_complete")) / len(rs), 4)


def answer_retention(records: Sequence[Mapping[str, Any]], *, arm: str) -> dict[str, Any]:
    """Per-arm answer-retention diagnostic: fraction of answerable questions whose
    ``arm`` context still contains a gold answer string (design §6 — reported
    SEPARATELY; a low-retention distilled arm is a LOSSY-distill artifact, NOT Mem0
    superiority, codex Q6)."""
    flagged = [
        r for r in records
        if r.get("has_answers") and arm in (r.get("context_has_gold") or {})
    ]
    if not flagged:
        return {"arm": arm, "n": 0, "retention": None}
    hits = sum(1 for r in flagged if (r.get("context_has_gold") or {}).get(arm))
    return {"arm": arm, "n": len(flagged), "retention": round(hits / len(flagged), 4)}


def _context_contains_answer(context: Sequence[str], answers: Sequence[str]) -> bool:
    """True iff any non-empty gold answer string appears (case-insensitively) in the
    joined context — the over-/under-salience signal for the retention diagnostic."""
    joined = "\n".join(context).lower()
    for a in answers:
        a = str(a).strip().lower()
        if a and a in joined:
            return True
    return False


# --------------------------------------------------------------------------- #
# Verdict assembly.
# --------------------------------------------------------------------------- #


def answer_completeness(
    records: Sequence[Mapping[str, Any]],
    queries: Sequence[GoldQuery],
) -> float:
    """Fraction of the input's **answerable** questions whose THREE new arms were all
    processed (codex §9 P1). A question counts as complete only when its record is
    present AND every arm in :data:`NEW_ARMS` was produced (key present in
    ``answers`` — an abstention ``None`` still counts as *processed*; a budget abort
    leaves the un-reached arms' keys ABSENT). A skipped (never-appended) question and
    a partially-answered question both count against completeness, so a capped prefix
    scores ``< 1.0``. Returns ``1.0`` when the input has no answerable question."""
    expected = sum(1 for q in queries if q.answers)
    if expected == 0:
        return 1.0
    complete = 0
    for r in records:
        if not r.get("has_answers"):
            continue
        answers = r.get("answers") or {}
        if all(arm in answers for arm in NEW_ARMS):
            complete += 1
    return round(complete / expected, 4)


def decide_all_classes(
    component_table: Mapping[str, Mapping[str, Mapping[str, Any]]],
    records: Sequence[Mapping[str, Any]],
    *,
    classes: Sequence[str] = GAP_CLASSES,
) -> dict[str, GapDecision]:
    """Apply the frozen :func:`decide_gap_decomposition` per class + pooled. The
    per-class ``fit_coverage`` gates the verdict (a class below
    :data:`FIT_COVERAGE_MIN` is forced INCONCLUSIVE); ``pooled`` uses the
    answerable-weighted mean coverage."""
    out: dict[str, GapDecision] = {}
    coverages: dict[str, float] = {c: class_fit_coverage(records, c) for c in classes}
    for cls in classes:
        out[cls] = decide_gap_decomposition(component_table[cls], coverages[cls])
    n_ans = [r for r in records if r.get("has_answers")]
    pooled_cov = round(sum(1 for r in n_ans if r.get("oracle_fit_complete")) / len(n_ans), 4) if n_ans else 0.0
    out["pooled"] = decide_gap_decomposition(component_table["pooled"], pooled_cov)
    return out


# --------------------------------------------------------------------------- #
# Orchestrator.
# --------------------------------------------------------------------------- #


def run_gap_decomposition(
    *,
    queries: Sequence[GoldQuery],
    documents: Mapping[str, str],
    d0b_cells: Mapping[tuple[str, str], Mapping[str, Any]],
    distill_cache: Mapping[str, Mapping[str, Any]],
    answerer: BaseAnswerer,
    ledger: BudgetLedger,
    reader: str,
    output: Path,
    fathomdb_adapter: Optional[Any] = None,
    k: int = 10,
    budget: Optional[int] = CONTEXT_CHAR_BUDGET,
    n_boot: int = DEFAULT_N_BOOT,
    seed: int = 0,
    classes: Sequence[str] = GAP_CLASSES,
    checkpoint_path: Optional[Path] = None,
    checkpoint_every: int = 10,
    mode: str = "run",
) -> dict[str, Any]:
    """Run the gap-decomposition matrix and emit per-class + pooled R/F/Resid with
    paired CIs, the frozen verdicts, the fit-coverage report, the $ ledger, and the
    answer-retention diagnostic.

    The reader answers the THREE new arms; ``fathomdb`` + ``mem0_oss`` acc come from
    the reused D0b cells. Every reader call passes the pre-call budget guard; a
    :class:`BudgetExceeded` checkpoints and HALTS (``aborted_for_cap``)."""
    t0 = time.time()
    scorer = PerClassScorer()
    answerer_available = bool(getattr(answerer, "available", False))
    ckpt_path = checkpoint_path or output.with_suffix(".checkpoint.json")

    # Resume the NEW-arm answers from a prior checkpoint (membership = reuse signal).
    rmap: dict[tuple[str, str], Optional[str]] = {}
    if ckpt_path.exists():
        prior = json.loads(ckpt_path.read_text(encoding="utf-8"))
        # Carry forward the prior cumulative $ spend so the cap is per-EXPERIMENT,
        # not per-PROCESS (codex §9 P1#1). The persisted value already includes the
        # D0b opening balance + all prior gap-decomp spend (new-arm + distiller).
        prior_spent = prior.get("ledger_spent_usd")
        if prior_spent is not None:
            ledger.restore_spent(float(prior_spent))
        for r in prior.get("records") or []:
            qid = r.get("qid")
            if qid is None:
                continue
            for arm, ans in (r.get("answers") or {}).items():
                if arm in NEW_ARMS:
                    rmap[(str(qid), str(arm))] = ans

    records: list[dict[str, Any]] = []
    aborted_for_cap = False

    def _checkpoint() -> None:
        # Persist the cumulative ledger spend ATOMICALLY with the records so a
        # resume restores the true per-experiment spend (codex §9 P1#1).
        _atomic_write_json(
            ckpt_path,
            {"records": records, "mode": mode, "reader": reader, "ledger_spent_usd": ledger.spent},
        )

    for i, q in enumerate(queries, start=1):
        gold = list(q.gold_doc_ids)
        oracle_raw_ctx, fit_complete = oracle_context(gold, documents, budget=budget)
        oracle_distilled_ctx = distilled_context(gold, distill_cache, budget=budget)
        if fathomdb_adapter is not None:
            fdb_hits = fathomdb_adapter.retrieve(q.question, k)
            fdb_doc_ids = [h.doc_id for h in fdb_hits]
        else:
            fdb_doc_ids = []
        fathomdb_distilled_ctx = distilled_context(fdb_doc_ids, distill_cache, budget=budget)

        arm_ctx: dict[str, list[str]] = {
            "oracle_raw": oracle_raw_ctx,
            "oracle_distilled": oracle_distilled_ctx,
            "fathomdb_distilled": fathomdb_distilled_ctx,
        }

        rec: dict[str, Any] = {
            "qid": q.query_id,
            "reporting_class": q.reporting_class,
            "gold": gold,
            "has_answers": bool(q.answers),
            "oracle_fit_complete": fit_complete,
            "answers": {},
            "acc": {},
            "context_has_gold": {},
        }
        # Reuse the paid D0b fathomdb + mem0 acc cells.
        for arm in REUSED_ARMS:
            cell = d0b_cells.get((q.query_id, arm))
            if cell is not None and cell.get("acc") is not None:
                rec["acc"][arm] = float(cell["acc"])

        if answerer_available and q.answers:
            for arm in NEW_ARMS:
                ctx = [b for b in arm_ctx[arm] if b]
                rec["context_has_gold"][arm] = _context_contains_answer(ctx, q.answers)
                key = (q.query_id, arm)
                if key in rmap:
                    ans = rmap[key]
                else:
                    try:
                        ans = answer_with_budget(
                            answerer, reader=reader, question=q.question, context=ctx, ledger=ledger
                        )
                    except BudgetExceeded:
                        aborted_for_cap = True
                        _checkpoint()
                        break
                rec["answers"][arm] = ans
                rec["acc"][arm] = scorer.score_answer(list(q.answers), ans)
            else:
                records.append(rec)
                if i % checkpoint_every == 0 or i == len(queries):
                    _checkpoint()
                continue
            # BudgetExceeded broke the arm loop → record partial + stop.
            records.append(rec)
            _checkpoint()
            break
        records.append(rec)
        if i % checkpoint_every == 0 or i == len(queries):
            _checkpoint()

    table = per_component_table(records, classes=classes, n_boot=n_boot, seed=seed)
    raw_verdicts = decide_all_classes(table, records, classes=classes)
    retention = {arm: answer_retention(records, arm=arm) for arm in NEW_ARMS}
    coverages = {c: class_fit_coverage(records, c) for c in classes}

    # --- citability gate (codex §9 P1) --------------------------------------- #
    # An incomplete / cap-aborted priced run is NON-CITABLE: a low-variance prefix
    # must NOT be allowed to emit a powered DOMINANT verdict for an INCOMPLETE
    # experiment. When aborted OR answer-completeness is below the floor, suppress
    # every DOMINANT result and publish ABORTED_INCOMPLETE; the component tables are
    # still recorded but the whole artifact is clearly marked non-citable.
    completeness = answer_completeness(records, queries)
    incomplete = aborted_for_cap or completeness < ANSWER_COMPLETENESS_MIN
    citable = not incomplete
    if aborted_for_cap:
        non_citable_reason: Optional[str] = "aborted_for_cap"
    elif incomplete:
        non_citable_reason = f"answer_completeness:{completeness:.4f}<{ANSWER_COMPLETENESS_MIN}"
    else:
        non_citable_reason = None

    if citable:
        verdicts: dict[str, Any] = dict(raw_verdicts)
        top_verdict = str(raw_verdicts["pooled"]["verdict"])
    else:
        # Override EVERY per-class + pooled verdict to the non-citable token so no
        # RETRIEVAL/DISTILLED_FORM/MEM0_RESIDUAL DOMINANT result can be published.
        verdicts = {}
        for cls_name, dec in raw_verdicts.items():
            d = dict(dec)
            d["verdict"] = ABORTED_VERDICT
            d["reason"] = non_citable_reason
            verdicts[cls_name] = d
        top_verdict = ABORTED_VERDICT

    art: dict[str, Any] = {
        "schema": "0.8.3-gap-decomposition-v1",
        "mode": mode,
        "reader_model": reader,
        "k": k,
        "context_char_budget": budget,
        "n_boot": n_boot,
        "seed": seed,
        "new_arms": list(NEW_ARMS),
        "reused_arms": list(REUSED_ARMS),
        "n_questions": len(records),
        "n_per_class": {c: sum(1 for r in records if r["reporting_class"] == c) for c in classes},
        "fit_coverage_per_class": coverages,
        "fit_coverage_min": FIT_COVERAGE_MIN,
        "component_deltas": table,
        "verdict": top_verdict,
        "verdicts": verdicts,
        "citable": citable,
        "run_valid": citable,
        "answer_completeness": completeness,
        "answer_completeness_min": ANSWER_COMPLETENESS_MIN,
        "non_citable_reason": non_citable_reason,
        "answer_retention": retention,
        "over_salience_note": (
            "answer-retention is a LOSSY-DISTILL diagnostic, reported SEPARATELY from "
            "the verdict: a low-retention distilled arm is a distillation artifact, NOT "
            "Mem0 superiority (codex Q6). A high oracle_raw retention with low "
            "oracle_distilled retention flags over-salience loss in the distiller."
        ),
        "confounds": [
            "R (RETRIEVAL) is an upper bound — oracle also strips distractors PRF/D2 "
            "cannot fully reproduce; treat as a ceiling, not the PRF lift.",
            "Resid (MEM0_RESIDUAL) bundles Mem0 retrieval+indexing+consolidation; a "
            "dominant Resid triggers a disambiguation follow-up, NOT a direct build.",
            "the 'Mem0-FORM' label is PROVISIONAL: a generic blind distiller is not "
            "Mem0's memory units (codex Q4).",
        ],
        "ledger": {
            "opening_balance_usd": ledger.opening_balance_usd,
            "hard_cap_usd": ledger.hard_cap_usd,
            "spent_usd": ledger.spent,
            "remaining_usd": ledger.remaining,
        },
        "aborted_for_cap": aborted_for_cap,
        "elapsed_s": round(time.time() - t0, 1),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(art, indent=2, default=str), encoding="utf-8")
    print(
        f"[GAPDECOMP][{mode.upper()}] wrote {output} | {len(records)} Q | "
        f"spent ${ledger.spent:.4f}/{ledger.hard_cap_usd:.0f} | "
        f"citable={citable} compl={completeness} | verdict={top_verdict}",
        flush=True,
    )
    return art


# --------------------------------------------------------------------------- #
# CLI (live backends — not exercised by the unit tests).
# --------------------------------------------------------------------------- #


def main(argv: Optional[list[str]] = None) -> int:  # pragma: no cover - CLI
    ap = argparse.ArgumentParser(description="0.8.3 gap-decomposition runner")
    ap.add_argument("--mode", choices=["cheap", "full"], required=True)
    ap.add_argument("--reader", default=None)
    ap.add_argument(
        "--distiller", default=CHEAP_READER_DEFAULT,
        help=("corpus distiller model — a CHEAP/local id, NEVER the priced reader "
              f"(default {CHEAP_READER_DEFAULT!r}; fail-closed if == the strong reader)"),
    )
    ap.add_argument("--gold", default="dev/plans/runs/0.8.3-d0a-memory-gold.json")
    ap.add_argument("--d0b-checkpoint", required=True,
                    help="0.8.3-d0b-parity-v2.checkpoint.json (HARD-STOP if no per-question records)")
    ap.add_argument("--distill-cache", default=None)
    ap.add_argument("--output", required=True)
    ap.add_argument("--k", type=int, default=10)
    ap.add_argument("--context-char-budget", type=int, default=CONTEXT_CHAR_BUDGET)
    ap.add_argument("--per-class", type=int, default=None)
    ap.add_argument("--max-output-tokens", type=int, default=DEFAULT_MAX_OUTPUT_TOKENS)
    args = ap.parse_args(argv)

    from eval.d0b_parity_run import _select_subset, build_documents_from_lme, build_live_adapters
    from eval.m1_baseline_run import CostTrackingAnswerer

    reader = resolve_reader(args.mode, args.reader)  # cheap-mode → cheap reader (P2)
    price_for(reader)  # fail closed BEFORE any backend stand-up

    _ch, _qv, queries = load_repin_gold(Path(args.gold))
    if args.per_class:
        queries = _select_subset(queries, per_class=args.per_class, classes=GAP_CLASSES)

    d0b_cells = load_d0b_cells(args.d0b_checkpoint)  # HARD-STOP here if no records

    documents = build_documents_from_lme(queries)
    ledger = BudgetLedger(max_output_tokens=args.max_output_tokens)

    # Distiller: a $-capped CHEAP model via the airlock (fail-closed pricing),
    # NEVER the priced reader. A local-Qwen ($0) distiller would pass ledger=None —
    # wire it here when the GPU is free. resolve_distiller_model fail-closes if the
    # distiller would be the priced strong reader.
    distiller_model = resolve_distiller_model(args.distiller, reader)
    price_for(distiller_model)  # fail closed: the distiller cap needs pinned pricing
    distill_client = CostTrackingAnswerer(distiller_model, timeout_s=120.0)
    # RAW completion path — the distill prompt is sent verbatim, NOT through the QA
    # answer template (codex §9 P1#2: the template made a real cheap model abstain,
    # corrupting both distilled arms).
    distiller = BlindDistiller(RawCompletionDistillerClient(distill_client))
    distill_cache = distill_corpus(
        documents, distiller,
        cache_path=Path(args.distill_cache) if args.distill_cache else None,
        ledger=ledger, priced_model=distiller_model,
    )

    adapters, _blk = build_live_adapters(documents, want_mem0=False, want_graphiti=False)
    fathomdb_adapter = adapters.get("fathomdb")

    answerer = CostTrackingAnswerer(reader, timeout_s=240.0)
    if not answerer.available:
        raise SystemExit(f"[GAPDECOMP][STOP] reader {reader!r} unavailable — do not fake answers")

    run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=answerer, ledger=ledger,
        reader=reader, output=Path(args.output), fathomdb_adapter=fathomdb_adapter,
        k=args.k, budget=args.context_char_budget, mode=args.mode,
    )
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
