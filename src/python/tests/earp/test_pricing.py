"""S9 pricing tests — written RED, before `eval.earp.pricing` exists.

The money gate's arithmetic, pure and file-local: preflight projection against
the D-3 ceiling, the authoritative-ledger rule, fail-closed pinned pricing, and
the per-call guard. No SDK, no engine, no network.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from eval.earp.pricing import (
    D3_AUTHORIZED_USD,
    LEDGER_ROOT_ENV,
    PRICE_PER_1M,
    CallGuard,
    authoritative_root,
    preflight,
    price_for,
    read_cumulative_spend,
)
from eval.earp.schema.models import Blocker, BlockerCode, CostLedger

LEDGER_PATH = "/tmp/ledger/index.jsonl"


def _ledger(cumulative: float, estimated: float) -> CostLedger:
    return CostLedger(
        authorized_usd=D3_AUTHORIZED_USD,
        cumulative_spent_usd=cumulative,
        estimated_usd=estimated,
    )


# --- the constant -------------------------------------------------------------


def test_the_authorization_is_five_dollars_and_a_module_constant() -> None:
    """D-3: $5.00 cumulative. Raising it is an HITL act done as a reviewed code
    edit — no config or env input can raise it."""
    assert D3_AUTHORIZED_USD == 5.00


# --- preflight: under / at / over ----------------------------------------------


def test_preflight_under_the_ceiling_passes() -> None:
    assert preflight(_ledger(2.0, 1.0), 0.5, LEDGER_PATH) is None


def test_preflight_at_the_ceiling_passes() -> None:
    """Refusal is `projected > authorized`, strictly: spending exactly to the
    authorization is authorized."""
    assert preflight(_ledger(2.5, 2.5), 1.0, LEDGER_PATH) is None


def test_preflight_over_the_ceiling_is_budget_exceeded_with_the_arithmetic() -> None:
    blocker = preflight(_ledger(3.0, 2.5), 1.0, LEDGER_PATH)
    assert isinstance(blocker, Blocker)
    assert blocker.code is BlockerCode.BUDGET_EXCEEDED
    assert blocker.stage == "priced.preflight"
    # The refusal audits from the sidecar alone: full arithmetic + ledger path.
    assert blocker.detail["cumulative_spent_usd"] == 3.0
    assert blocker.detail["estimated_usd"] == 2.5
    assert blocker.detail["projected_usd"] == 5.5
    assert blocker.detail["authorized_usd"] == D3_AUTHORIZED_USD
    assert blocker.detail["ledger_path"] == LEDGER_PATH


def test_preflight_refuses_an_under_declared_estimate() -> None:
    """Gate 1's cross-check: the declared worst case must DOMINATE the computed
    one; an author cannot buy past the gate with an invented small number."""
    blocker = preflight(_ledger(0.0, 0.10), 0.25, LEDGER_PATH)
    assert isinstance(blocker, Blocker)
    assert blocker.code is BlockerCode.CONFIG_INVALID_VALUE
    assert blocker.stage == "priced.preflight"
    assert blocker.detail["computed_estimate_usd"] == 0.25
    assert blocker.detail["estimated_usd"] == 0.10
    assert blocker.detail["ledger_path"] == LEDGER_PATH


def test_a_dominating_declaration_passes_the_cross_check() -> None:
    assert preflight(_ledger(0.0, 0.25), 0.25, LEDGER_PATH) is None


def test_the_cross_check_precedes_the_projection() -> None:
    """When the declaration is a lie AND the projection overruns, the lie is
    the first defect: the projection was computed from an untrustworthy
    number."""
    blocker = preflight(_ledger(4.9, 0.2), 0.5, LEDGER_PATH)
    assert isinstance(blocker, Blocker)
    assert blocker.code is BlockerCode.CONFIG_INVALID_VALUE


# --- the ledger read -------------------------------------------------------------


def _write_index(root: Path, rows: list[Any]) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    index = root / "index.jsonl"
    lines = [row if isinstance(row, str) else json.dumps(row) for row in rows]
    index.write_text("".join(line + "\n" for line in lines), encoding="utf-8")
    return index


def test_read_cumulative_sums_cost_usd_tolerating_absent_fields(tmp_path: Path) -> None:
    _write_index(
        tmp_path,
        [
            {"run_id": "a", "cost_usd": 1.25},
            {"run_id": "b"},  # a $0 (recording-only) run: tolerated as 0.0
            {"run_id": "c", "cost_usd": 0.5},
        ],
    )
    assert read_cumulative_spend(tmp_path) == pytest.approx(1.75)


def test_a_missing_index_reads_as_zero(tmp_path: Path) -> None:
    """Worktrees start with an EMPTY (or absent) index; the authoritative-root
    gate — not this reader — is what stops that resetting the ceiling."""
    assert read_cumulative_spend(tmp_path / "nowhere") == 0.0


def test_a_malformed_ledger_is_a_typed_blocker_not_zero(tmp_path: Path) -> None:
    _write_index(tmp_path, [{"run_id": "a", "cost_usd": 1.0}, "{not json"])
    result = read_cumulative_spend(tmp_path)
    assert isinstance(result, Blocker)
    assert result.stage == "priced.ledger"


def test_a_non_numeric_cost_is_a_malformed_ledger(tmp_path: Path) -> None:
    _write_index(tmp_path, [{"run_id": "a", "cost_usd": "lots"}])
    result = read_cumulative_spend(tmp_path)
    assert isinstance(result, Blocker)
    assert result.stage == "priced.ledger"


# --- the authoritative root -------------------------------------------------------


def test_priced_runs_require_the_ledger_root_env(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.delenv(LEDGER_ROOT_ENV, raising=False)
    result = authoritative_root(tmp_path)
    assert isinstance(result, Blocker)
    assert result.stage == "priced.ledger"
    assert LEDGER_ROOT_ENV in result.message


def test_a_run_root_that_is_not_the_authoritative_root_is_refused(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv(LEDGER_ROOT_ENV, str(tmp_path / "authoritative"))
    result = authoritative_root(tmp_path / "elsewhere")
    assert isinstance(result, Blocker)
    assert result.stage == "priced.ledger"


def test_the_matching_root_resolves(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv(LEDGER_ROOT_ENV, str(tmp_path))
    result = authoritative_root(tmp_path)
    assert isinstance(result, Path)
    assert result == tmp_path.resolve()


# --- pinned pricing ---------------------------------------------------------------


def test_price_for_returns_the_pinned_rates() -> None:
    model = next(iter(PRICE_PER_1M))
    assert price_for(model) == PRICE_PER_1M[model]


def test_price_for_fails_closed_on_an_unpinned_model() -> None:
    """Never a default: a $-cap is unenforceable without pinned pricing."""
    result = price_for("some-model-nobody-pinned")
    assert isinstance(result, Blocker)
    assert "pinned" in result.message


# --- the per-call guard -----------------------------------------------------------


def test_call_guard_halts_before_the_overrunning_call() -> None:
    """spent_so_far + next_call_worst_case > authorized − cumulative → halt
    BEFORE the call, with the partial spend intact."""
    guard = CallGuard(authorized_usd=D3_AUTHORIZED_USD, cumulative_spent_usd=4.90)
    # Remaining authorization: 0.10.
    assert guard.guard(0.03) is None
    guard.record(0.03)
    assert guard.guard(0.03) is None
    guard.record(0.03)
    assert guard.guard(0.03) is None
    guard.record(0.03)
    blocker = guard.guard(0.03)  # 0.09 + 0.03 > 0.10
    assert isinstance(blocker, Blocker)
    assert blocker.code is BlockerCode.BUDGET_EXCEEDED
    assert blocker.stage == "priced.call_guard"
    assert guard.spent_usd == pytest.approx(0.09)


def test_call_guard_uses_the_remaining_authorization_not_the_ceiling() -> None:
    guard = CallGuard(authorized_usd=D3_AUTHORIZED_USD, cumulative_spent_usd=4.99)
    blocker = guard.guard(0.02)
    assert isinstance(blocker, Blocker)
    assert blocker.code is BlockerCode.BUDGET_EXCEEDED
