"""P0-A base-retrieval END-TO-END via the airlock **Batch API** (e.g. gpt-5.4-nano).

A runner that reuses the *tested* P0-A pieces — ``load_lme_smoke`` /
``build_variants`` / ``run_retrieval_loop`` and the identical-answerer
``BaseAnswerer`` prompt template + ``_match`` scorer — but answers all
``(variant, question)`` pairs in **one asynchronous batch job** (~50% cheaper)
instead of synchronous per-question ``/chat/completions`` calls.

LLM-free Recall@K is still reported (the primary base metric); the batch supplies
answer-accuracy. Batch path proven 2026-06-14 (airlock ``docs/guide/batch.md``):
``custom_llm_provider`` on /files + /batches, an **upstream** model id (not an
alias), multipart JSONL upload.

The pure logic (:func:`build_batch_jsonl`, :func:`parse_batch_output`,
:func:`score_e2e`) and the polling orchestrator (:func:`run_batch`, which takes a
duck-typed client) are network-free and unit-tested in
``tests/test_p0a_batch_e2e.py``.

Usage:
    .venv/bin/python -m eval.p0a_batch_e2e --per-class 2 \\
        --provider openai --reader-model gpt-5.4-nano --output /tmp/p0a_batch.json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from collections.abc import Mapping
from pathlib import Path
from typing import Any, Callable, Optional, Protocol

from eval.p0a_base_retrieval import (
    DEFAULT_DATASET,
    DEFAULT_SEED,
    DEFAULT_SPLIT,
    SMOKE_CLASSES,
    _AIRLOCK_API_KEY,
    _AIRLOCK_BASE_URL,
    AnswerRecord,
    build_variants,
    load_lme_smoke,
    measure_haystack,
    run_retrieval_loop,
    score_answers,
)
from eval.r2_parity_eval import BaseAnswerer, normalize_answer


# --------------------------------------------------------------------------- #
# Pure logic (network-free, unit-tested)
# --------------------------------------------------------------------------- #
def build_batch_jsonl(
    smoke: Any,
    systems: dict[str, Any],
    *,
    context_k: int,
    reader_model: str,
    max_tokens: int,
) -> tuple[str, dict[str, dict[str, Any]]]:
    """Build the batch input JSONL + a sidecar mapping ``custom_id -> meta``.

    Every (variant, question) pair becomes one ``/v1/chat/completions`` request,
    keyed ``custom_id = f"{variant}||{qid}"``, prompted through the **identical**
    :class:`BaseAnswerer` template so all variants are read the same way.
    """
    answerer = BaseAnswerer()
    lines: list[str] = []
    sidecar: dict[str, dict[str, Any]] = {}
    for name, adapter in systems.items():
        for q in smoke.questions:
            hits = adapter.retrieve(q.question, context_k)
            ctx = [h.body for h in hits if h.body]
            prompt = answerer.build_prompt(q.question, ctx)
            cid = f"{name}||{q.qid}"
            sidecar[cid] = {
                "variant": name,
                "qid": q.qid,
                "class": q.reporting_class,
                "question": q.question,  # carried so the judge batch is self-contained
                "answer": q.answer,
                "is_abstention": q.is_abstention,
            }
            lines.append(json.dumps({
                "custom_id": cid,
                "method": "POST",
                "url": "/v1/chat/completions",
                "body": {
                    "model": reader_model,
                    "messages": [{"role": "user", "content": prompt}],
                    "temperature": 0,
                    "seed": 0,
                    "max_completion_tokens": max_tokens,
                },
            }))
    return ("\n".join(lines) + "\n" if lines else ""), sidecar


def parse_batch_output(text: str) -> tuple[dict[str, Optional[str]], int]:
    """Parse an OpenAI-batch output JSONL into ``custom_id -> answer|None``.

    Returns ``(answers, parse_errors)``. A line whose response is missing/malformed
    contributes ``None`` and increments ``parse_errors``; answers are abstention-
    normalized via the shared :func:`normalize_answer`.
    """
    answers: dict[str, Optional[str]] = {}
    parse_errors = 0
    for ln in text.splitlines():
        if not ln.strip():
            continue
        try:
            rec = json.loads(ln)
        except json.JSONDecodeError:
            # A corrupt/truncated line must not discard an already-paid batch.
            parse_errors += 1
            continue
        body = (rec.get("response") or {}).get("body") or {}
        try:
            content = body["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError):
            content = None
            parse_errors += 1
        answers[rec.get("custom_id")] = normalize_answer(content)
    return answers, parse_errors


def score_e2e(
    sidecar: dict[str, dict[str, Any]],
    answers: Mapping[str, Optional[str]],
    *,
    verdicts: Optional[dict[str, bool]] = None,
) -> dict[str, Any]:
    """Score answers per variant/class via the shared
    :func:`p0a_base_retrieval.score_answers`, so the batch and sync e2e paths
    cannot drift (they build the same :class:`AnswerRecord`s and call one scorer).

    ``verdicts`` (cid -> bool from the LLM judge, :func:`parse_judge_output`)
    overrides the strict :func:`_match` substring check for positive queries; a cid
    missing from it falls back to ``_match`` (graceful judge-parse-error
    degradation). Abstention query: correct iff the reader abstains (``None``).
    """
    records = [
        AnswerRecord(
            cid=cid,
            variant=meta["variant"],
            reporting_class=meta["class"],
            is_abstention=meta["is_abstention"],
            gold_answer=meta["answer"],
            answer=answers.get(cid),
        )
        for cid, meta in sidecar.items()
    ]
    return score_answers(records, verdicts=verdicts)


# --------------------------------------------------------------------------- #
# LLM judge (replaces the crude substring _match for positive queries)
# --------------------------------------------------------------------------- #
_JUDGE_PROMPT = (
    "You are grading a candidate answer against a reference answer for one question.\n"
    "Mark CORRECT if the candidate conveys the same factual information as the\n"
    "reference, even if phrased differently, more verbose, or differently formatted.\n"
    "Mark INCORRECT if it contradicts, omits the key fact, or only hedges/refuses.\n"
    "Judge ONLY factual equivalence to the reference; ignore style and extra detail.\n\n"
    "Question: {question}\n"
    "Reference answer: {gold}\n"
    "Candidate answer: {candidate}\n\n"
    'Respond with ONLY this JSON, nothing else: {{"correct": true}} or {{"correct": false}}'
)

# A flat {...} object containing "correct" — tolerates ```json fences / surrounding prose.
_VERDICT_RE = re.compile(r'\{[^{}]*"correct"[^{}]*\}')


def build_judge_jsonl(
    sidecar: dict[str, dict[str, Any]],
    answers: dict[str, Optional[str]],
    *,
    judge_model: str,
    max_tokens: int = 16,
) -> tuple[str, dict[str, dict[str, Any]]]:
    """Build a judge batch: one request per POSITIVE question with a non-abstaining
    answer. Abstention questions are scored by rule and abstained positives are
    auto-misses, so neither needs (or wastes) a judge call. ``custom_id`` reuses the
    answerer's cid so verdicts plug straight into :func:`score_e2e`."""
    lines: list[str] = []
    judged: dict[str, dict[str, Any]] = {}
    for cid, meta in sidecar.items():
        ans = answers.get(cid)
        if meta["is_abstention"] or ans is None:
            continue
        prompt = _JUDGE_PROMPT.format(
            question=meta.get("question", ""), gold=meta["answer"], candidate=ans,
        )
        judged[cid] = {"gold": meta["answer"], "candidate": ans}
        lines.append(json.dumps({
            "custom_id": cid,
            "method": "POST",
            "url": "/v1/chat/completions",
            "body": {
                "model": judge_model,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0,
                "seed": 0,
                "max_completion_tokens": max_tokens,
            },
        }))
    return ("\n".join(lines) + "\n" if lines else ""), judged


def _extract_verdict(content: Optional[str]) -> Optional[bool]:
    """Pull a boolean out of a judge response, tolerant of fences/prose. ``None`` if
    no parseable ``{"correct": <bool>}`` is found."""
    if not content:
        return None
    text = content.strip()
    candidates = [text]
    m = _VERDICT_RE.search(text)
    if m:
        candidates.append(m.group(0))
    for cand in candidates:
        try:
            obj = json.loads(cand)
        except (json.JSONDecodeError, TypeError):
            continue
        if isinstance(obj, dict) and "correct" in obj:
            val = obj["correct"]
            if isinstance(val, bool):
                return val
            if isinstance(val, str):
                return val.strip().lower() in ("true", "yes", "1")
    return None


def parse_judge_output(text: str) -> tuple[dict[str, bool], int]:
    """Parse a judge batch output JSONL into ``custom_id -> bool``.

    Returns ``(verdicts, parse_errors)``. A missing/unparseable verdict is omitted
    (NOT defaulted) and counts as an error; :func:`score_e2e` then falls back to the
    strict :func:`_match` for that cid, so a flaky judge degrades gracefully rather
    than silently scoring everything wrong.
    """
    verdicts: dict[str, bool] = {}
    parse_errors = 0
    for ln in text.splitlines():
        if not ln.strip():
            continue
        try:
            rec = json.loads(ln)
        except json.JSONDecodeError:
            parse_errors += 1
            continue
        body = (rec.get("response") or {}).get("body") or {}
        try:
            content = body["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError):
            content = None
        verdict = _extract_verdict(content)
        if verdict is None:
            parse_errors += 1
            continue
        verdicts[rec.get("custom_id")] = verdict
    return verdicts, parse_errors


# --------------------------------------------------------------------------- #
# Batch transport (a duck-typed client so run_batch is mockable)
# --------------------------------------------------------------------------- #
class BatchClient(Protocol):
    def upload(self, jsonl: str, provider: str) -> str: ...
    def create(self, file_id: str, provider: str, model: Optional[str] = None) -> str: ...
    def status(self, batch_id: str, provider: str) -> dict[str, Any]: ...
    def download(self, file_id: str, provider: str = "openai") -> str: ...


class AirlockBatchClient:
    """OpenAI-compatible Batch client over the airlock proxy (lazy-imports httpx)."""

    def __init__(self, base_url: str, api_key: str, *, timeout: float = 120.0) -> None:
        self._base = base_url.rstrip("/")
        self._hdr = {"Authorization": f"Bearer {api_key}"}
        self._timeout = timeout

    def _client(self):  # pragma: no cover - thin httpx wrapper
        import httpx  # type: ignore[import-not-found]  # httpx not in [dev] extras (eval-only)

        return httpx.Client(timeout=self._timeout)

    def upload(self, jsonl: str, provider: str) -> str:  # pragma: no cover - network
        files = {"file": ("batch_input.jsonl", jsonl, "application/jsonl")}
        with self._client() as c:
            if provider == "aistudio":
                # The Airlock Batch Gateway intercepts on the ?custom_llm_provider
                # QUERY param (not the openai form field). docs/guide/batch.md.
                r = c.post(
                    f"{self._base}/files", headers=self._hdr,
                    params={"custom_llm_provider": provider},
                    data={"purpose": "batch"}, files=files,
                )
            else:
                r = c.post(
                    f"{self._base}/files", headers=self._hdr,
                    data={"purpose": "batch", "custom_llm_provider": provider}, files=files,
                )
            r.raise_for_status()
            return r.json()["id"]

    def create(self, file_id: str, provider: str, model: Optional[str] = None) -> str:  # pragma: no cover - network
        body: dict[str, Any] = {
            "input_file_id": file_id, "endpoint": "/v1/chat/completions",
            "completion_window": "24h",
        }
        params: Optional[dict[str, str]] = None
        if provider == "aistudio":
            # The gateway intercepts on the ?custom_llm_provider QUERY param (NOT the
            # body); the airlock alias goes in `model` (gateway -> provider_model).
            params = {"custom_llm_provider": provider}
            if model:
                body["model"] = model
        else:
            # openai path (proven): litellm accepts the provider in the body here.
            body["custom_llm_provider"] = provider
        with self._client() as c:
            r = c.post(
                f"{self._base}/batches",
                headers={**self._hdr, "Content-Type": "application/json"},
                params=params,
                json=body,
            )
            r.raise_for_status()
            return r.json()["id"]

    def status(self, batch_id: str, provider: str) -> dict[str, Any]:  # pragma: no cover
        with self._client() as c:
            r = c.get(
                f"{self._base}/batches/{batch_id}", headers=self._hdr,
                params={"custom_llm_provider": provider},
            )
            r.raise_for_status()  # a transient 5xx must not silently kill the poll loop
            return r.json()

    def download(self, file_id: str, provider: str = "openai") -> str:  # pragma: no cover - network
        # The aistudio gateway staged the output in its OWN store; it serves
        # /v1/files/{id}/content only when it intercepts on ?custom_llm_provider —
        # without it the request falls through to litellm and 404s.
        params = {"custom_llm_provider": provider} if provider == "aistudio" else None
        with self._client() as c:
            r = c.get(f"{self._base}/files/{file_id}/content", headers=self._hdr, params=params)
            r.raise_for_status()  # don't feed a 4xx error body into parse_batch_output
            return r.text


_TERMINAL = {"failed", "cancelled", "expired"}


def run_batch(
    client: BatchClient,
    jsonl: str,
    provider: str,
    *,
    poll_secs: float,
    max_polls: int,
    model: Optional[str] = None,
    sleep: Callable[[float], None] = time.sleep,
    log: Callable[[str], None] = lambda _m: None,
) -> tuple[Optional[str], str, Optional[str]]:
    """Upload → create → poll → download. Returns ``(batch_id, status, out_text)``.

    ``model`` is the airlock alias passed to ``create`` (the aistudio gateway needs it
    in the create body; the openai path ignores it). Terminal failure or poll
    exhaustion returns ``out_text=None``. ``sleep``/``log`` are injectable so the loop
    is unit-testable with no real waiting or network.
    """
    file_id = client.upload(jsonl, provider)
    batch_id = client.create(file_id, provider, model)
    status = "unknown"
    for i in range(max_polls):
        st = client.status(batch_id, provider)
        status = str(st.get("status"))
        log(f"[poll {i + 1}] status={status} counts={st.get('request_counts')}")
        if status == "completed":
            ofid = st.get("output_file_id")
            return batch_id, status, (client.download(ofid, provider) if ofid else None)
        if status in _TERMINAL:
            return batch_id, status, None
        sleep(poll_secs)
    return batch_id, status, None


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #
def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="P0-A base e2e via airlock Batch API")
    ap.add_argument("--per-class", type=int, default=2)
    ap.add_argument("--seed", type=int, default=DEFAULT_SEED)
    ap.add_argument("--context-k", type=int, default=10)
    # Provider + model are decoupled so the same driver works across providers (the
    # airlock dispatches batch by `custom_llm_provider`). Today:
    #   openai    -> upstream id e.g. gpt-5.4-nano (working)
    #   vertex_ai -> a *regional* Gemini id e.g. gemini-2.5-flash (when configured)
    #   aistudio / mistral -> pending the airlock unified batch gateway
    # The model id MUST be the upstream id the provider expects, not an alias.
    ap.add_argument("--provider", default="openai",
                    help="custom_llm_provider for /files + /batches (openai|vertex_ai|...)")
    ap.add_argument("--reader-model", default="gpt-5.4-nano",
                    help="upstream model id for the chosen --provider")
    ap.add_argument("--judge-model", default="",
                    help="if set, run an LLM-judge batch (same provider) for answer "
                         "accuracy instead of the crude substring _match")
    ap.add_argument("--judge-max-tokens", type=int, default=16)
    ap.add_argument("--max-tokens", type=int, default=256)
    ap.add_argument("--db-dir", default="/tmp")
    ap.add_argument("--output", required=True)
    ap.add_argument("--poll-secs", type=float, default=15.0)
    # A sized batch (e.g. 320 answerer reqs) can take ~30m server-side; 160x15s=40m
    # headroom so the poller doesn't give up on an in-flight batch (the n=160 op lesson).
    ap.add_argument("--max-polls", type=int, default=160)
    ap.add_argument("--no-fused", action="store_true",
                    help="skip the dense+FTS fused variant (default: include it). "
                         "The fused build re-embeds from scratch — run with "
                         "FATHOMDB_EMBED_DEVICE=cuda or it is CPU-bound (~hours).")
    ap.add_argument("--variants", default=None,
                    help="comma-separated subset of built variants to SUBMIT to the "
                         "answerer/judge batch (e.g. 'fathomdb_fused'). All variants are "
                         "still built; only the named ones cost LLM $. Default: all.")
    args = ap.parse_args(argv)

    t0 = time.time()
    smoke = load_lme_smoke(
        DEFAULT_DATASET, DEFAULT_SPLIT, per_class=args.per_class, seed=args.seed,
        classes=SMOKE_CLASSES,
    )
    g1 = measure_haystack(smoke)
    systems, build_blk = build_variants(
        smoke.documents, Path(args.db_dir), include_fused=not args.no_fused,
    )
    if args.variants:
        built = set(systems)
        keep = {v.strip() for v in args.variants.split(",") if v.strip()}
        unknown = keep - built
        if unknown:
            print(f"[batch-e2e] WARNING: --variants names not built: {sorted(unknown)}")
        systems = {k: v for k, v in systems.items() if k in keep}
        if not systems:
            raise SystemExit(
                f"--variants {sorted(keep)} selected no built variant "
                f"(built: {sorted(built)}); nothing to submit."
            )
        print(f"[batch-e2e] --variants filter -> submitting only: {sorted(systems)}")
    retrieval = run_retrieval_loop(smoke, systems)

    jsonl, sidecar = build_batch_jsonl(
        smoke, systems, context_k=args.context_k,
        reader_model=args.reader_model, max_tokens=args.max_tokens,
    )
    print(f"[batch-e2e] {len(sidecar)} requests ({len(systems)} variants x {len(smoke.questions)} Q)")

    client = AirlockBatchClient(_AIRLOCK_BASE_URL, _AIRLOCK_API_KEY)
    batch_id, status, out_text = run_batch(
        client, jsonl, args.provider, model=args.reader_model,
        poll_secs=args.poll_secs, max_polls=args.max_polls, log=print,
    )

    answers, parse_errors = parse_batch_output(out_text or "")

    # Optional second batch: an LLM judge replaces the strict substring _match for
    # positive answered questions (abstention scoring stays rule-based). Missing
    # verdicts fall back to _match per-cid (graceful degradation).
    verdicts: Optional[dict[str, bool]] = None
    judge_block: dict[str, Any] = {}
    if args.judge_model and answers:
        judge_jsonl, judged = build_judge_jsonl(
            sidecar, answers, judge_model=args.judge_model, max_tokens=args.judge_max_tokens,
        )
        if judge_jsonl:
            print(f"[batch-e2e] judging {len(judged)} positive answers via {args.judge_model}")
            j_bid, j_status, j_out = run_batch(
                client, judge_jsonl, args.provider, model=args.judge_model,
                poll_secs=args.poll_secs, max_polls=args.max_polls, log=print,
            )
            verdicts, judge_parse_errors = parse_judge_output(j_out or "")
            judge_block = {
                "judge_model": args.judge_model,
                "judge_batch_id": j_bid,
                "judge_batch_status": j_status,
                "n_judged": len(judged),
                "n_verdicts": len(verdicts),
                "judge_parse_errors": judge_parse_errors,
            }

    reader_block = score_e2e(sidecar, answers, verdicts=verdicts)

    result = {
        "slice": "0.8.1/p0-a-batch-e2e",
        "mode": "smoke-batch",
        "provider": args.provider,
        "reader_model": args.reader_model,
        "scoring": "llm_judge" if verdicts is not None else "substring_match",
        "judge": judge_block,
        "batch_id": batch_id,
        "batch_status": status,
        "n_questions": len(smoke.questions),
        "n_requests": len(sidecar),
        "variants": sorted(systems.keys()),
        "g1_haystack_measurement": g1,
        "retrieval_loop": retrieval,
        "e2e_loop": {f"{args.reader_model} (batch)": reader_block},
        "blockers_encountered": build_blk,
        "parse_errors": parse_errors,
        "elapsed_s": round(time.time() - t0, 1),
    }
    Path(args.output).write_text(json.dumps(result, indent=2))
    print(f"[batch-e2e] wrote {args.output} (status={status}, {result['elapsed_s']}s)")
    return 0 if status == "completed" else 1


if __name__ == "__main__":
    sys.exit(main())
