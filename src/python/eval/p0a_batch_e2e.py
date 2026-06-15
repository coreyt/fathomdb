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
import sys
import time
from collections import defaultdict
from pathlib import Path
from typing import Any, Callable, Optional, Protocol

from eval.p0a_base_retrieval import (
    DEFAULT_DATASET,
    DEFAULT_SEED,
    DEFAULT_SPLIT,
    SMOKE_CLASSES,
    _AIRLOCK_API_KEY,
    _AIRLOCK_BASE_URL,
    _mean,
    build_variants,
    load_lme_smoke,
    measure_haystack,
    run_retrieval_loop,
)
from eval.r2_parity_eval import BaseAnswerer, _match, _normalize

# Abstention strings treated as "no answer". Computed THROUGH `_normalize` so the
# comparison is self-consistent: `_normalize` turns "I don't know" into
# "i don t know" (apostrophe -> space), so a raw set like {"i dont know"} silently
# misses the exact phrase the prompt instructs the model to emit. (The legacy
# AirlockAnswerer._complete set has that gap; this driver fixes it.)
_ABSTAIN = {_normalize(s) for s in ("I don't know", "I dont know", "idk")} | {""}


# --------------------------------------------------------------------------- #
# Pure logic (network-free, unit-tested)
# --------------------------------------------------------------------------- #
def _norm_answer(text: Optional[str]) -> Optional[str]:
    """Normalize a raw reader answer to ``None`` when it is empty/an abstention."""
    t = (text or "").strip()
    if not t or _normalize(t) in _ABSTAIN:
        return None
    return t


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
    normalized via :func:`_norm_answer`.
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
        answers[rec.get("custom_id")] = _norm_answer(content)
    return answers, parse_errors


def score_e2e(
    sidecar: dict[str, dict[str, Any]],
    answers: dict[str, Optional[str]],
) -> dict[str, Any]:
    """Score answers per variant/class.

    Positive query: correct iff a non-abstaining answer matches gold via
    :func:`_match` (a normalized-substring check — strict; a correct-but-rephrased
    answer that omits the gold string scores 0, the documented scorer-strictness
    caveat). Abstention query: correct iff the reader abstains (``None``).

    NB — this intentionally diverges from the synchronous
    ``p0a_base_retrieval.run_e2e_loop`` in two ways: (1) it reports ``n_answered``
    (a non-None count) rather than that loop's ``n_calls``/``n_errors`` (batch has
    no per-call exception), and (2) abstention is detected via the apostrophe-safe
    :data:`_ABSTAIN` here, fixing a gap the legacy ``AirlockAnswerer._complete``
    still has. Unifying the two scorers (and back-porting the abstention fix) is a
    tracked follow-up, not in this runner's scope.
    """
    per_variant: dict[str, dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    answered: dict[str, int] = defaultdict(int)
    for cid, meta in sidecar.items():
        ans = answers.get(cid)
        if ans is not None:
            answered[meta["variant"]] += 1
        if meta["is_abstention"]:
            score = 1.0 if ans is None else 0.0
        else:
            score = 1.0 if (ans is not None and _match([meta["answer"]], ans)) else 0.0
        per_variant[meta["variant"]][meta["class"]].append(score)

    out: dict[str, Any] = {}
    for name, pc in per_variant.items():
        out[name] = {
            "per_class_accuracy": {c: _mean(v) for c, v in pc.items()},
            "overall_accuracy": _mean([s for v in pc.values() for s in v]),
            "n_answered": answered[name],
        }
    return out


# --------------------------------------------------------------------------- #
# Batch transport (a duck-typed client so run_batch is mockable)
# --------------------------------------------------------------------------- #
class BatchClient(Protocol):
    def upload(self, jsonl: str, provider: str) -> str: ...
    def create(self, file_id: str, provider: str) -> str: ...
    def status(self, batch_id: str, provider: str) -> dict[str, Any]: ...
    def download(self, file_id: str) -> str: ...


class AirlockBatchClient:
    """OpenAI-compatible Batch client over the airlock proxy (lazy-imports httpx)."""

    def __init__(self, base_url: str, api_key: str, *, timeout: float = 120.0) -> None:
        self._base = base_url.rstrip("/")
        self._hdr = {"Authorization": f"Bearer {api_key}"}
        self._timeout = timeout

    def _client(self):  # pragma: no cover - thin httpx wrapper
        import httpx

        return httpx.Client(timeout=self._timeout)

    def upload(self, jsonl: str, provider: str) -> str:  # pragma: no cover - network
        with self._client() as c:
            r = c.post(
                f"{self._base}/files", headers=self._hdr,
                data={"purpose": "batch", "custom_llm_provider": provider},
                files={"file": ("batch_input.jsonl", jsonl, "application/jsonl")},
            )
            r.raise_for_status()
            return r.json()["id"]

    def create(self, file_id: str, provider: str) -> str:  # pragma: no cover - network
        with self._client() as c:
            r = c.post(
                f"{self._base}/batches",
                headers={**self._hdr, "Content-Type": "application/json"},
                json={
                    "input_file_id": file_id, "endpoint": "/v1/chat/completions",
                    "completion_window": "24h", "custom_llm_provider": provider,
                },
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

    def download(self, file_id: str) -> str:  # pragma: no cover - network
        with self._client() as c:
            r = c.get(f"{self._base}/files/{file_id}/content", headers=self._hdr)
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
    sleep: Callable[[float], None] = time.sleep,
    log: Callable[[str], None] = lambda _m: None,
) -> tuple[Optional[str], str, Optional[str]]:
    """Upload → create → poll → download. Returns ``(batch_id, status, out_text)``.

    Terminal failure or poll exhaustion returns ``out_text=None``. ``sleep``/``log``
    are injectable so the loop is unit-testable with no real waiting or network.
    """
    file_id = client.upload(jsonl, provider)
    batch_id = client.create(file_id, provider)
    status = "unknown"
    for i in range(max_polls):
        st = client.status(batch_id, provider)
        status = str(st.get("status"))
        log(f"[poll {i + 1}] status={status} counts={st.get('request_counts')}")
        if status == "completed":
            ofid = st.get("output_file_id")
            return batch_id, status, (client.download(ofid) if ofid else None)
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
    ap.add_argument("--max-tokens", type=int, default=256)
    ap.add_argument("--db-dir", default="/tmp")
    ap.add_argument("--output", required=True)
    ap.add_argument("--poll-secs", type=float, default=15.0)
    ap.add_argument("--max-polls", type=int, default=80)
    args = ap.parse_args(argv)

    t0 = time.time()
    smoke = load_lme_smoke(
        DEFAULT_DATASET, DEFAULT_SPLIT, per_class=args.per_class, seed=args.seed,
        classes=SMOKE_CLASSES,
    )
    g1 = measure_haystack(smoke)
    systems, build_blk = build_variants(smoke.documents, Path(args.db_dir), include_fused=False)
    retrieval = run_retrieval_loop(smoke, systems)

    jsonl, sidecar = build_batch_jsonl(
        smoke, systems, context_k=args.context_k,
        reader_model=args.reader_model, max_tokens=args.max_tokens,
    )
    print(f"[batch-e2e] {len(sidecar)} requests ({len(systems)} variants x {len(smoke.questions)} Q)")

    client = AirlockBatchClient(_AIRLOCK_BASE_URL, _AIRLOCK_API_KEY)
    batch_id, status, out_text = run_batch(
        client, jsonl, args.provider,
        poll_secs=args.poll_secs, max_polls=args.max_polls, log=print,
    )

    answers, parse_errors = parse_batch_output(out_text or "")
    reader_block = score_e2e(sidecar, answers)

    result = {
        "slice": "0.8.1/p0-a-batch-e2e",
        "mode": "smoke-batch",
        "provider": args.provider,
        "reader_model": args.reader_model,
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
