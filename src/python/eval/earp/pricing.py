"""S9 — the money gate's arithmetic: pinned pricing, the authoritative ledger,
the cumulative D-3 preflight, and the per-call guard.

Everything here RETURNS its refusal as a typed `Blocker` (the S1/S3 house
rule), and everything fails CLOSED: an unpinned model, an unreadable ledger,
or a missing authoritative root is a refusal, never a $0 or a default.

The pinned-pricing pattern is `eval.gap_decomposition_run`'s (`PRICE_PER_1M` /
`price_for`, "a $-cap is unenforceable without pinned pricing"), re-implemented
here rather than imported so the standing experiment module stays untouched.

Design of record: `dev/design/earp-slice-9-design.md`.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Mapping

from eval.earp.schema.models import Blocker, BlockerCode, CostLedger

#: D-3: $5.00 pre-authorized, CUMULATIVE across all priced EARP runs. Raising
#: it is an HITL act done as a reviewed code edit -- no config or env input can
#: raise it, and the sidecar's `authorized_usd` is written from this constant
#: ONLY.
D3_AUTHORIZED_USD = 5.00

#: The authoritative-ledger env gate. Worktrees start with a git-tracked EMPTY
#: `experiments/index.jsonl`, so a per-run root would reset the D-3 ceiling per
#: checkout; a priced run must therefore name the one root whose index is the
#: ledger, and run against exactly that root.
LEDGER_ROOT_ENV = "FDB_EARP_LEDGER_ROOT"

#: PINNED pricing ($ / 1M tokens, (input, output)) -- the ids this repo has
#: already pinned for priced work (`gap_decomposition_run.PRICE_PER_1M`, the
#: airlock/litellm rates recorded there). NO default fallback: an un-pinned
#: model fails closed via `price_for`.
PRICE_PER_1M: Mapping[str, tuple[float, float]] = {
    "gpt-5.4": (1.25, 5.00),
    "gpt-5-nano": (0.05, 0.40),
    "gemini-flash-lite": (0.05, 0.20),
    "gemini-3.1-flash-lite": (0.05, 0.20),
    "gemini-2.5-flash-lite": (0.05, 0.20),
    "claude-haiku-4-5": (1.00, 5.00),
    "claude-sonnet-4-6": (3.00, 15.00),
    "claude-opus-4-8": (5.00, 25.00),
    "claude-sonnet": (3.00, 15.00),
    "claude-haiku": (1.00, 5.00),
    "claude-opus": (5.00, 25.00),
}


def _refusal(message: str, stage: str, **detail: object) -> Blocker:
    return Blocker(
        code=BlockerCode.BUDGET_EXCEEDED,
        message=message,
        stage=stage,
        detail=dict(detail),
    )


def price_for(model: str) -> tuple[float, float] | Blocker:
    """`(in_rate, out_rate)` per 1M tokens for a PINNED model.

    Fail closed: an unpinned model is a typed blocker, never a default -- a
    $-cap is unenforceable without pinned pricing (the in-repo precedent)."""
    rates = PRICE_PER_1M.get(model)
    if rates is None:
        return Blocker(
            code=BlockerCode.CONFIG_INVALID_VALUE,
            message=(
                f"no pinned pricing for model {model!r}; refusing to project a "
                f"$-cap on a default. Pinned models: {sorted(PRICE_PER_1M)}"
            ),
            stage="priced.preflight",
            detail={"model": model, "pinned": sorted(PRICE_PER_1M)},
        )
    return rates


def read_cumulative_spend(experiments_root: Path) -> float | Blocker:
    """Sum `cost_usd` across the root's `index.jsonl`.

    Rows WITHOUT the field are tolerated as 0.0 (recording-only runs predate
    priced ones); a row that cannot be parsed, is not an object, or carries a
    non-numeric `cost_usd` makes the LEDGER malformed -- a typed blocker, never
    a $0 read, because a $0 read of a broken ledger un-enforces the ceiling."""
    index_path = Path(experiments_root) / "index.jsonl"
    if not index_path.is_file():
        return 0.0
    total = 0.0
    for number, line in enumerate(
        index_path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as exc:
            return _refusal(
                f"ledger {index_path} line {number} is not valid JSON ({exc}); a "
                f"malformed ledger cannot be summed, so the D-3 ceiling cannot be "
                f"enforced and the priced run is refused",
                "priced.ledger",
                ledger_path=str(index_path),
                line=number,
            )
        if not isinstance(row, dict):
            return _refusal(
                f"ledger {index_path} line {number} is not an object; refusing to "
                f"read a malformed ledger as $0",
                "priced.ledger",
                ledger_path=str(index_path),
                line=number,
            )
        cost = row.get("cost_usd", 0.0)
        if cost is None:
            cost = 0.0
        if isinstance(cost, bool) or not isinstance(cost, (int, float)):
            return _refusal(
                f"ledger {index_path} line {number} carries a non-numeric "
                f"cost_usd ({cost!r}); refusing to read a malformed ledger as $0",
                "priced.ledger",
                ledger_path=str(index_path),
                line=number,
            )
        total += float(cost)
    return total


def authoritative_root(experiments_root: Path) -> Path | Blocker:
    """Gate 3's root rule: `FDB_EARP_LEDGER_ROOT` must be set AND equal the
    run's `experiments_root`, else a typed refusal. Tmp roots stay legal for
    stub tests, which never pass the opt-in gate."""
    declared = os.environ.get(LEDGER_ROOT_ENV)
    if not declared:
        return _refusal(
            f"priced runs require the authoritative spend ledger: set "
            f"{LEDGER_ROOT_ENV} to the one experiments root whose index.jsonl is "
            f"the D-3 ledger (worktree indexes start empty, which would reset the "
            f"cumulative ceiling per checkout)",
            "priced.ledger",
            env=LEDGER_ROOT_ENV,
        )
    declared_path = Path(declared).resolve()
    run_path = Path(experiments_root).resolve()
    if declared_path != run_path:
        return _refusal(
            f"this run's experiments_root ({run_path}) is not the authoritative "
            f"ledger root ({LEDGER_ROOT_ENV}={declared_path}); a priced run "
            f"recorded outside the ledger would not feed the cumulative ceiling",
            "priced.ledger",
            env=LEDGER_ROOT_ENV,
            authoritative_root=str(declared_path),
            experiments_root=str(run_path),
        )
    return run_path


def preflight(
    ledger: CostLedger, computed_estimate_usd: float, ledger_path: str
) -> Blocker | None:
    """The pure D-3 preflight. Refuses BOTH directions, cross-check first:

    1. an under-declared estimate (`computed > declared`) -- the declared worst
       case must DOMINATE the computed one, or an author buys past the gate
       with an invented small number; and only then
    2. the over-budget projection (`cumulative + declared > authorized`),
       because a projection built on an untrusted declaration names the wrong
       defect.

    `detail` carries the full arithmetic AND the ledger path, so the refusal
    audits from the sidecar alone."""
    if computed_estimate_usd > ledger.estimated_usd:
        return Blocker(
            code=BlockerCode.CONFIG_INVALID_VALUE,
            message=(
                f"budget.estimated_usd (${ledger.estimated_usd:.4f}) is smaller "
                f"than the adapter's computed worst case "
                f"(${computed_estimate_usd:.4f}) over min(max_queries, n_queries); "
                f"the declared worst case must dominate the computed one"
            ),
            stage="priced.preflight",
            detail={
                "path": "budget.estimated_usd",
                "estimated_usd": ledger.estimated_usd,
                "computed_estimate_usd": computed_estimate_usd,
                "ledger_path": ledger_path,
            },
        )
    projected = ledger.cumulative_spent_usd + ledger.estimated_usd
    if projected > ledger.authorized_usd:
        return _refusal(
            f"projected cumulative spend ${ledger.cumulative_spent_usd:.4f} "
            f"(ledger) + ${ledger.estimated_usd:.4f} (declared estimate) = "
            f"${projected:.4f} exceeds the D-3 authorization of "
            f"${ledger.authorized_usd:.2f}; refused before any priced call",
            "priced.preflight",
            cumulative_spent_usd=ledger.cumulative_spent_usd,
            estimated_usd=ledger.estimated_usd,
            projected_usd=projected,
            authorized_usd=ledger.authorized_usd,
            ledger_path=ledger_path,
        )
    return None


@dataclass
class CallGuard:
    """The runtime meter: every priced call passes this BEFORE it is made.

    Halt rule (the `BudgetLedger.guard` precedent, pre-call, never after-call):
    `spent_so_far + next_call_worst_case > authorized_usd - cumulative_spent_usd`
    is a typed blocker; the caller records partials and stops."""

    authorized_usd: float
    cumulative_spent_usd: float
    spent_usd: float = 0.0

    @property
    def remaining_usd(self) -> float:
        return self.authorized_usd - self.cumulative_spent_usd - self.spent_usd

    def guard(self, next_call_worst_case_usd: float) -> Blocker | None:
        remaining_authorization = self.authorized_usd - self.cumulative_spent_usd
        if self.spent_usd + next_call_worst_case_usd > remaining_authorization:
            return _refusal(
                f"the next call's worst case (${next_call_worst_case_usd:.4f}) "
                f"on top of ${self.spent_usd:.4f} already spent this run would "
                f"exceed the remaining authorization "
                f"(${remaining_authorization:.4f} = ${self.authorized_usd:.2f} "
                f"authorized - ${self.cumulative_spent_usd:.4f} cumulative); "
                f"halting with partials recorded",
                "priced.call_guard",
                spent_usd=self.spent_usd,
                next_call_worst_case_usd=next_call_worst_case_usd,
                cumulative_spent_usd=self.cumulative_spent_usd,
                authorized_usd=self.authorized_usd,
            )
        return None

    def record(self, actual_usd: float) -> None:
        self.spent_usd += actual_usd


__all__ = [
    "D3_AUTHORIZED_USD",
    "LEDGER_ROOT_ENV",
    "PRICE_PER_1M",
    "CallGuard",
    "authoritative_root",
    "preflight",
    "price_for",
    "read_cumulative_spend",
]
