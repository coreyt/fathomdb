"""M1 answerer 429 backoff/retry — TDD (RED first), Slice 20 clean re-run.

Root cause of the INVALID first priced pass: ``workers=10`` tripped the airlock
quota; every later call HTTP-429'd and degraded to an abstention (None ⇒ F1=0),
deflating all arms inside the primary ≥3-hop cell. Fix: the priced answerer
(:class:`eval.m1_baseline_run.CostTrackingAnswerer`) must RETRY a transient HTTP
429 (or a 5xx / connection-level error) with exponential backoff so a rate-limited
cell recovers instead of being lost. A cell counts as failed only after the
retries are exhausted.

These tests inject a fake opener (no network / no env) so they pin the retry
contract deterministically.
"""

from __future__ import annotations

import json
import urllib.error

from eval.m1_baseline_run import CostTrackingAnswerer

_BODY = {
    "choices": [{"message": {"content": "Paris"}}],
    "usage": {"prompt_tokens": 10, "completion_tokens": 2},
}


class _FakeResp:
    def __init__(self, body: dict) -> None:
        self._b = json.dumps(body).encode("utf-8")

    def read(self) -> bytes:
        return self._b

    def __enter__(self) -> "_FakeResp":
        return self

    def __exit__(self, *_a: object) -> bool:
        return False


def _http429(req: object) -> urllib.error.HTTPError:
    url = getattr(req, "full_url", "http://x")
    return urllib.error.HTTPError(url, 429, "Too Many Requests", {}, None)  # type: ignore[arg-type]


def _http503(req: object) -> urllib.error.HTTPError:
    url = getattr(req, "full_url", "http://x")
    return urllib.error.HTTPError(url, 503, "Service Unavailable", {}, None)  # type: ignore[arg-type]


def _answerer() -> CostTrackingAnswerer:
    # backoff_base=0 + a no-op sleep ⇒ the test never actually waits.
    return CostTrackingAnswerer(
        "gemini-3.1-pro", max_retries=4, backoff_base=0.0, sleep=lambda _s: None
    )


def test_429_then_200_yields_answer_not_none() -> None:
    ans = _answerer()
    calls = {"n": 0}

    def fake_open(req: object) -> _FakeResp:
        calls["n"] += 1
        if calls["n"] <= 2:  # two 429s, then a 200
            raise _http429(req)
        return _FakeResp(_BODY)

    ans._open = fake_open  # type: ignore[assignment]
    out = ans.answer("what is the capital of France?", ["ctx"])

    assert out is not None  # recovered after backoff, NOT degraded to abstention
    assert out == "Paris"
    assert calls["n"] == 3  # 2 failed attempts + 1 success
    assert ans.n_retries == 2
    assert ans.n_calls == 1  # exactly one SUCCESSFUL completion counted
    assert ans.n_errors == 0  # a recovered call is not a failure


def test_transient_5xx_is_retried() -> None:
    ans = _answerer()
    calls = {"n": 0}

    def fake_open(req: object) -> _FakeResp:
        calls["n"] += 1
        if calls["n"] == 1:
            raise _http503(req)
        return _FakeResp(_BODY)

    ans._open = fake_open  # type: ignore[assignment]
    assert ans.answer("q?", ["ctx"]) == "Paris"
    assert ans.n_retries == 1


def test_persistent_429_exhausts_then_raises() -> None:
    # After max_retries the call gives up and RAISES — run_baseline's _do catches it,
    # counts n_errors, and degrades that single cell to None (the completeness guard
    # then catches a wholesale outage). A cell fails ONLY after retries are exhausted.
    ans = _answerer()
    calls = {"n": 0}

    def fake_open(req: object) -> _FakeResp:
        calls["n"] += 1
        raise _http429(req)

    ans._open = fake_open  # type: ignore[assignment]
    raised = False
    try:
        ans._complete("prompt", "q?", ["ctx"])
    except urllib.error.HTTPError:
        raised = True
    assert raised
    assert calls["n"] == 1 + 4  # the initial try + max_retries=4 retries
    assert ans.n_retries == 4
    assert ans.n_calls == 0  # no successful completion


def test_non_retryable_4xx_raises_immediately() -> None:
    # a 400/401 is a hard error, not a rate-limit — do not waste retries on it.
    ans = _answerer()
    calls = {"n": 0}

    def fake_open(req: object) -> _FakeResp:
        calls["n"] += 1
        url = getattr(req, "full_url", "http://x")
        raise urllib.error.HTTPError(url, 400, "Bad Request", {}, None)  # type: ignore[arg-type]

    ans._open = fake_open  # type: ignore[assignment]
    raised = False
    try:
        ans._complete("prompt", "q?", ["ctx"])
    except urllib.error.HTTPError:
        raised = True
    assert raised
    assert calls["n"] == 1  # no retries on a non-retryable status
    assert ans.n_retries == 0
