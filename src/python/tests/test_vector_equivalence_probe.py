"""0.8.18 Slice 5 (#5 vector-equivalence probe) — Python SDK surface parity (X1).

Verifies the Python binding surfaces the #5 vector-equivalence probe contract at
Py↔TS parity (mirror of `src/ts/tests/vector-equivalence-probe.test.ts`):

  - `VectorEquivalenceMismatchError` is an importable leaf of `EngineError` and
    carries a `reason` attribute (parity with `EmbedderIdentityMismatchError`).
  - `OpenReport` surfaces `dense_disabled` (+ `dense_disabled_reason`); a healthy
    (non-divergent) open reports `dense_disabled == False` (R-VEQ-6).
  - the `Engine.search_text_only` FTS-only path exists and serves.
  - the degraded-observability accessors exist and report the healthy default.

The DIVERGENCE behaviour (a divergent backend trips the refusal; sub-epsilon
float noise does not) is proven end-to-end at the engine layer in Rust
(`fathomdb-engine/tests/vector_equivalence_probe.rs`), which re-embeds through the
SAME live-backend path the binding uses — this suite pins the SDK SURFACE parity.
"""

from __future__ import annotations

from fathomdb import Engine
from fathomdb.errors import EngineError, VectorEquivalenceMismatchError

# 0.8.20 (R-20-E3): `source_id` is mandatory on every canonical write.
_SOURCE_ID = "py-test:vector-equivalence-probe"



def test_vector_equivalence_mismatch_is_engine_error_leaf() -> None:
    assert issubclass(VectorEquivalenceMismatchError, EngineError)
    # Typed-init carries the divergence reason (parity with the identity-mismatch leaf).
    err = VectorEquivalenceMismatchError("boom", reason="P1 flips=3")
    assert err.reason == "P1 flips=3"


def test_open_report_surfaces_dense_disabled_default_false(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        report = engine.open_report()
        assert hasattr(report, "dense_disabled")
        assert report.dense_disabled is False
        assert hasattr(report, "dense_disabled_reason")
        assert report.dense_disabled_reason is None
        # engine-attached observability accessors mirror the report.
        assert engine.dense_disabled() is False
        assert engine.dense_disabled_reason() is None
        assert engine.vector_equivalence_refusal_count() == 0
    finally:
        engine.close()


def test_search_text_only_serves(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        engine.write(
            [{"kind": "note", "body": "alpha bravo charlie", "source_id": _SOURCE_ID}]
        )
        engine.drain(timeout_s=30)
        result = engine.search_text_only("alpha")
        # FTS-only path returns a SearchResult (may or may not hit; must not raise).
        assert hasattr(result, "results")
    finally:
        engine.close()
