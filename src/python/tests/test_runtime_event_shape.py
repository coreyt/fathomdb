"""EU-6 FIX-2 — runtime shape consistency for ``embedder_events`` (AC-FIX2-6).

Codifies the contract that the Rust emitter ships exactly the
variant-appropriate keys per ``kind`` — same shape the TypedDict union
declared in FIX-2 GREEN promises statically. This is a contract test
that should ALREADY pass on current ``main`` because the EU-6 GREEN
emitter already produces the correct shape; FIX-2's GREEN does not
change runtime emission. The point is to lock the invariant in CI so
any future drift between the Rust emitter and the typed union is caught
by a failing test rather than slipping through static analysis.

Network-gated via ``FATHOMDB_SKIP_NETWORK_TESTS`` (mirrors
``test_use_default_embedder.py``).
"""

from __future__ import annotations

import os

import pytest

from fathomdb import Engine

# Expected key set per `kind`. Sourced from
# `dev/design/0.7.1-EU-6-FIX-2-design.md` §2.1 (verified against the
# Rust emitter at `fathomdb-py/src/lib.rs:417-444`).
_VARIANT_KEYS: dict[str, set[str]] = {
    "DefaultEmbedderDownload": {
        "kind",
        "file",
        "url",
        "bytes",
        "sha256",
        "cache_path",
        "duration_ms",
    },
    "DefaultEmbedderCacheHit": {
        "kind",
        "file",
        "sha256",
        "cache_path",
    },
    "MeanVecPinned": {
        "kind",
        "dim",
        "doc_count",
    },
}

# Expected value type per (kind, field). int/str only (no Optional —
# emitter always populates required fields for the variant).
_VARIANT_TYPES: dict[str, dict[str, type]] = {
    "DefaultEmbedderDownload": {
        "kind": str,
        "file": str,
        "url": str,
        "bytes": int,
        "sha256": str,
        "cache_path": str,
        "duration_ms": int,
    },
    "DefaultEmbedderCacheHit": {
        "kind": str,
        "file": str,
        "sha256": str,
        "cache_path": str,
    },
    "MeanVecPinned": {
        "kind": str,
        "dim": int,
        "doc_count": int,
    },
}


def _skip_if_no_network() -> None:
    if os.environ.get("FATHOMDB_SKIP_NETWORK_TESTS"):
        pytest.skip("FATHOMDB_SKIP_NETWORK_TESTS set; skipping network-hitting test")


def test_runtime_embedder_events_match_typed_union_shape(db_path: str) -> None:
    """For each event emitted by a live ``open_report()`` against the
    default embedder, the dict's key set + value types match what the
    typed union promises."""

    _skip_if_no_network()
    engine = Engine.open(db_path, use_default_embedder=True)
    try:
        report = engine.open_report()
    finally:
        engine.close()

    assert report.embedder_events, (
        "expected at least one embedder event on a fresh default-embedder open"
    )

    for event in report.embedder_events:
        assert isinstance(event, dict), f"event is not a dict: {event!r}"
        assert "kind" in event, f"event missing `kind`: {event!r}"
        kind = event["kind"]
        assert kind in _VARIANT_KEYS, (
            f"unknown `kind` {kind!r}; expected one of "
            f"{sorted(_VARIANT_KEYS)}"
        )

        expected_keys = _VARIANT_KEYS[kind]
        actual_keys = set(event.keys())
        missing = expected_keys - actual_keys
        extra = actual_keys - expected_keys
        assert not missing, (
            f"{kind}: missing keys {sorted(missing)}; event={event!r}"
        )
        assert not extra, (
            f"{kind}: unexpected extra keys {sorted(extra)}; event={event!r}"
        )

        for field, expected_type in _VARIANT_TYPES[kind].items():
            value = event[field]
            # bool is a subclass of int — exclude it for int fields.
            if expected_type is int:
                assert isinstance(value, int) and not isinstance(value, bool), (
                    f"{kind}.{field}: expected int, got {type(value).__name__} "
                    f"({value!r})"
                )
            else:
                assert isinstance(value, expected_type), (
                    f"{kind}.{field}: expected {expected_type.__name__}, "
                    f"got {type(value).__name__} ({value!r})"
                )
