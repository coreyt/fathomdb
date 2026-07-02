#!/usr/bin/env python3
"""EXP-COV-1 — resilient priced relation-extraction runner (the ONLY priced call).

Decouples the priced LLM extraction from the (`$0`, deterministic) engine ingest so
ALL §4 resilience preconditions live in one place, provable at `$0` before any spend:

* **incremental ATOMIC checkpoint** — every unit is written through
  :class:`~eval.exp_cov1_common.ExtractionCache` (temp-file + ``os.replace``); at most
  the in-flight unit is lost on a crash.
* **verified ``--resume``** — a re-invocation skips any unit already ``ok`` in the cache,
  keyed by ``doc_id + model + prompt_version``.
* **429/5xx exponential backoff + cap** — :func:`_call_llm`.
* **per-doc window-fit** — the body is truncated to a char budget so a request never
  overflows the context (LOCOMO sessions are small, but the guard is unconditional).
* **completeness guard** — a failed extraction is RECORDED ``status=failed`` (never a
  silent skip); the sweep refuses to score until every unit is present-or-failed.
* **running $ LEDGER with HARD auto-stop** — :class:`~eval.exp_cov1_common.DollarLedger`
  multiplies ACTUAL response-usage tokens by a pinned price map and STOPS at the
  ceiling BEFORE issuing a call that would exceed it.

Usage::

    # $0 dry run of resilience on a free/local model or the stub (no spend)
    python -m eval.exp_cov1_extract --corpus locomo --limit 8 --model gemma-4 \
        --cache /tmp/cov1/relation.ndjson --ledger /tmp/cov1/ledger.ndjson

    # priced pilot then (if extrapolation <= ceiling) the full pass
    python -m eval.exp_cov1_extract --corpus locomo --pilot 8 --model gpt-5-mini ...
    python -m eval.exp_cov1_extract --corpus locomo --model gpt-5-mini ...   # resumes
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from typing import Any, Optional

from eval.exp_cov1_common import (
    HARD_DOLLAR_CEILING,
    PROMPT_VERSION,
    RELATION_SYSTEM_PROMPT,
    RELATION_USER_TEMPLATE,
    DollarLedger,
    ExtractionCache,
    cache_key,
)

_BASE_URL = os.environ.get("ELPS_LLM_BASE_URL", "http://localhost:4000/v1")
_API_KEY = os.environ.get("ELPS_LLM_API_KEY", "sk-airlock-mk")

#: Window-fit char budget (~ 4 chars/token; a generous cap well under any model window).
_MAX_BODY_CHARS = 24_000
#: Backoff config.
_MAX_RETRIES = 6
_BACKOFF_BASE_S = 2.0
_BACKOFF_CAP_S = 60.0
#: A cost-estimate guard: if the per-doc mean cost so far * remaining would blow the
#: ceiling, stop. Also a fixed conservative per-call projection for the FIRST call.
_FIRST_CALL_PROJECTION_USD = 0.05


def _strip_fences(text: str) -> str:
    text = text.strip()
    if text.startswith("```"):
        lines = text.splitlines()
        inner = lines[1:-1] if lines and lines[-1].strip() == "```" else lines[1:]
        text = "\n".join(inner).strip()
    return text


def _call_llm(
    doc_id: str, body: str, created_at: str, model: str, *, timeout_s: float = 90.0
) -> tuple[dict[str, Any], int, int]:
    """One extraction call with 429/5xx exponential backoff. Returns
    ``(parsed_json, prompt_tokens, completion_tokens)``. Raises on final failure."""
    user_msg = RELATION_USER_TEMPLATE.format(
        doc_id=doc_id, created_at=created_at, body=body[:_MAX_BODY_CHARS]
    )
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": RELATION_SYSTEM_PROMPT},
            {"role": "user", "content": user_msg},
        ],
        "temperature": 0,
        "seed": 0,
        "max_tokens": 8192,
        "response_format": {"type": "json_object"},
    }
    data = json.dumps(payload).encode("utf-8")
    last_exc: Optional[Exception] = None
    for attempt in range(_MAX_RETRIES):
        try:
            req = urllib.request.Request(
                _BASE_URL.rstrip("/") + "/chat/completions",
                data=data,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {_API_KEY}",
                },
            )
            with urllib.request.urlopen(req, timeout=timeout_s) as resp:  # noqa: S310
                body_json = json.loads(resp.read().decode("utf-8"))
            text = body_json["choices"][0]["message"]["content"]
            parsed = json.loads(_strip_fences(text))
            usage = body_json.get("usage", {}) or {}
            pt = int(usage.get("prompt_tokens", 0))
            ct = int(usage.get("completion_tokens", 0))
            return parsed, pt, ct
        except urllib.error.HTTPError as exc:  # noqa: PERF203
            last_exc = exc
            if exc.code in (429, 500, 502, 503, 504):
                sleep = min(_BACKOFF_BASE_S * (2 ** attempt), _BACKOFF_CAP_S)
                print(
                    f"[cov1-extract] {doc_id} HTTP {exc.code} attempt {attempt + 1}"
                    f"/{_MAX_RETRIES}; backoff {sleep:.1f}s",
                    file=sys.stderr,
                    flush=True,
                )
                time.sleep(sleep)
                continue
            raise
        except json.JSONDecodeError as exc:
            # Deterministic (temperature=0, seed=0): retrying the identical request
            # yields the identical bad output, so do NOT burn the exponential backoff.
            # One quick retry (covers a rare transient truncation), then fail fast so
            # the completeness guard records FAILED and the pass moves on.
            last_exc = exc
            if attempt >= 1:
                break
            time.sleep(1.0)
        except (urllib.error.URLError, TimeoutError) as exc:
            last_exc = exc
            sleep = min(_BACKOFF_BASE_S * (2 ** attempt), _BACKOFF_CAP_S)
            print(
                f"[cov1-extract] {doc_id} {type(exc).__name__} attempt {attempt + 1}"
                f"/{_MAX_RETRIES}; backoff {sleep:.1f}s",
                file=sys.stderr,
                flush=True,
            )
            time.sleep(sleep)
    raise RuntimeError(f"extraction failed after {_MAX_RETRIES} attempts: {last_exc}")


def _sanitize(parsed: dict[str, Any], doc_id: str) -> dict[str, Any]:
    """Mirror the ELPS harness's Rust-safety filtering (empty names, ':' in kind,
    confidence clamp) so the replay-ingest never trips EngineError."""
    entities = [
        e for e in parsed.get("entities", [])
        if isinstance(e, dict)
        and str(e.get("name", "")).strip()
        and ":" not in str(e.get("type", ""))
    ]
    edges = []
    for e in parsed.get("edges", []):
        if not isinstance(e, dict):
            continue
        if not str(e.get("from_entity", "")).strip() or not str(e.get("to_entity", "")).strip():
            continue
        if not e.get("source_doc_id"):
            e["source_doc_id"] = doc_id
        c = e.get("confidence")
        if isinstance(c, (int, float)) and c == c:
            e["confidence"] = max(0.0, min(1.0, float(c)))
        elif c is not None:
            e["confidence"] = 0.5
        edges.append(e)
    return {"entities": entities, "edges": edges, "warnings": parsed.get("warnings", [])}


def load_docs(corpus: str, path: Optional[str]) -> list[tuple[str, str, str]]:
    """Return ``[(doc_id, body, created_at)]`` for the corpus."""
    if corpus == "locomo":
        from eval.locomo_loader import load_locomo

        docs, _gold = load_locomo(
            path or "data/corpus-data/raw/locomo10.json"
        )
        return [(did, body, "2023-01-01T00:00:00Z") for did, body in sorted(docs.items())]
    raise ValueError(f"unknown corpus {corpus!r}")


def run_extraction(
    docs: list[tuple[str, str, str]],
    *,
    model: str,
    cache_path: str,
    ledger_path: str,
    ceiling: float = HARD_DOLLAR_CEILING,
    limit: Optional[int] = None,
    stub: bool = False,
) -> dict[str, Any]:
    """Resilient extraction pass. Resumes from ``cache_path``; auto-stops at ``ceiling``.

    ``stub`` (a `$0` resilience dry-run) records a canned extraction with zero usage,
    exercising the checkpoint/resume/completeness path without any network/spend."""
    cache = ExtractionCache.load(cache_path)
    ledger = DollarLedger.load(ledger_path, ceiling=ceiling)
    if limit is not None:
        docs = docs[:limit]

    expected_keys = [cache_key(d, model, PROMPT_VERSION) for d, _, _ in docs]
    n_done = n_new = n_failed = 0
    stopped_reason: Optional[str] = None

    for doc_id, body, created_at in docs:
        key = cache_key(doc_id, model, PROMPT_VERSION)
        if cache.has_ok(key):
            n_done += 1
            continue

        # $ auto-stop BEFORE the call. Project next-call cost from the running mean
        # (or a fixed conservative floor for the first call).
        priced_units = [e for e in ledger.entries if e.get("cost_usd", 0) > 0]
        mean_cost = (
            sum(e["cost_usd"] for e in priced_units) / len(priced_units)
            if priced_units else _FIRST_CALL_PROJECTION_USD
        )
        if not stub and ledger.would_exceed(mean_cost):
            stopped_reason = (
                f"AUTO-STOP: ledger ${ledger.total:.4f} + projected ${mean_cost:.4f} "
                f"> ceiling ${ceiling:.2f}"
            )
            print(f"[cov1-extract] {stopped_reason}", file=sys.stderr, flush=True)
            break

        try:
            if stub:
                parsed = {
                    "entities": [{"name": "StubEntity", "type": "concept", "aliases": []}],
                    "edges": [{
                        "from_entity": "StubEntity", "to_entity": "StubEntity",
                        "relation": "self_ref", "body": "stub", "t_valid": created_at,
                        "t_invalid": None, "confidence": 0.5,
                        "source_doc_id": doc_id, "source_span": None,
                    }],
                    "warnings": [],
                }
                pt = ct = 0
            else:
                parsed, pt, ct = _call_llm(doc_id, body, created_at, model)
            clean = _sanitize(parsed, doc_id)
            rec = {
                "key": key, "doc_id": doc_id, "model": model,
                "prompt_version": PROMPT_VERSION, "status": "ok",
                "entities": clean["entities"], "edges": clean["edges"],
                "warnings": clean["warnings"],
                "usage": {"prompt_tokens": pt, "completion_tokens": ct},
            }
            cache.put(rec)  # ATOMIC checkpoint
            if not stub:
                ledger.add(doc_id=doc_id, model=model, prompt_tokens=pt, completion_tokens=ct)
            n_new += 1
            print(
                f"[cov1-extract] ok {doc_id} ent={len(clean['entities'])} "
                f"edge={len(clean['edges'])} tok={pt}+{ct} "
                f"cum=${ledger.total:.4f}",
                flush=True,
            )
        except Exception as exc:  # noqa: BLE001 — record FAILED, never silent-skip
            cache.put({
                "key": key, "doc_id": doc_id, "model": model,
                "prompt_version": PROMPT_VERSION, "status": "failed",
                "entities": [], "edges": [], "warnings": [],
                "usage": {"prompt_tokens": 0, "completion_tokens": 0},
                "error": str(exc),
            })
            n_failed += 1
            print(f"[cov1-extract] FAILED {doc_id}: {exc}", file=sys.stderr, flush=True)

    comp = cache.completeness(expected_keys)
    return {
        "model": model,
        "prompt_version": PROMPT_VERSION,
        "n_docs": len(docs),
        "n_already_done": n_done,
        "n_new": n_new,
        "n_failed": n_failed,
        "dollars_spent": round(ledger.total, 6),
        "ceiling": ceiling,
        "stopped_reason": stopped_reason,
        "completeness": comp,
        "cache_path": cache_path,
        "ledger_path": ledger_path,
    }


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="EXP-COV-1 resilient priced extraction runner")
    ap.add_argument("--corpus", default="locomo")
    ap.add_argument("--corpus-path", default=None)
    ap.add_argument("--model", required=True)
    ap.add_argument("--cache", required=True)
    ap.add_argument("--ledger", required=True)
    ap.add_argument("--ceiling", type=float, default=HARD_DOLLAR_CEILING)
    ap.add_argument("--limit", type=int, default=None, help="cap docs (dry-run / pilot)")
    ap.add_argument("--pilot", type=int, default=None, help="alias for --limit (pilot size)")
    ap.add_argument("--stub", action="store_true", help="$0 resilience dry-run (no network)")
    args = ap.parse_args(argv)

    docs = load_docs(args.corpus, args.corpus_path)
    limit = args.pilot if args.pilot is not None else args.limit
    summary = run_extraction(
        docs, model=args.model, cache_path=args.cache, ledger_path=args.ledger,
        ceiling=args.ceiling, limit=limit, stub=args.stub,
    )
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":  # pragma: no cover
    raise SystemExit(main())
