"""S8 stats tests — written RED, before `eval.earp.stats` exists.

Pure: no SDK, no database, no network, no numpy.

Rust-parity provenance: `tests/earp/fixtures/bootstrap_parity.json` was
generated ONCE from a verbatim standalone copy of the Rust harness's two
functions — `SplitMix64` (`src/rust/crates/fathomdb-engine/tests/`
`eu8_ir_validation.rs:104-121`, index mapping `next_in` = `next_u64() % n` at
`:118-120`) and `bootstrap_ci` (`:283-304`), with the hardcoded
`BOOTSTRAP_SEED = 0x0E88B007574A9001` (`:71`) — compiled and run with bare
`rustc -O gen_bootstrap_expectations.rs -o gen_bootstrap_expectations`
(rustc 1.95.0), NOT via `cargo test` (which pays a full candle compile; D-1
forbids committing engine-crate tests, not running the reference once). The
scratch .rs and binary were deleted after generation; only the JSON
expectations are committed. Delta inputs and CI bounds are carried as f64 BIT
PATTERNS, so parity is asserted byte-for-byte with no decimal round-trip.
"""

from __future__ import annotations

import json
import struct
from itertools import islice
from pathlib import Path
from typing import Any

from eval.earp.stats import paired_bootstrap_ci, splitmix64

FIXTURE = Path(__file__).parent / "fixtures" / "bootstrap_parity.json"
BOOTSTRAP_SEED = 0x0E88B007574A9001

#: Published SplitMix64 test vectors (Vigna), seed 0. The first output is the
#: canonical pin; the full sequence is cross-checked against the executed Rust
#: expectations below, so a wrong constant cannot hide behind a wrong fixture.
VIGNA_SEED0_FIRST = 0xE220A8397B1DCDAF


def _expectations() -> dict[str, Any]:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


def _f64(bits_hex: str) -> float:
    return struct.unpack("<d", struct.pack("<Q", int(bits_hex, 16)))[0]


def _bits(value: float) -> str:
    return f"0x{struct.unpack('<Q', struct.pack('<d', value))[0]:016x}"


# --- AC-4: the RNG is the reference's RNG -----------------------------------


def test_splitmix64_matches_the_published_seed0_vector() -> None:
    assert next(splitmix64(0)) == VIGNA_SEED0_FIRST


def test_splitmix64_seed0_sequence_matches_the_rust_copy() -> None:
    expected = [int(x, 16) for x in _expectations()["splitmix64_seed0"]]
    assert list(islice(splitmix64(0), len(expected))) == expected


def test_splitmix64_bootstrap_seed_sequence_matches_the_rust_copy() -> None:
    expected = [int(x, 16) for x in _expectations()["splitmix64_bootstrap_seed"]]
    assert list(islice(splitmix64(BOOTSTRAP_SEED), len(expected))) == expected


def test_splitmix64_outputs_stay_in_u64() -> None:
    """Python ints are unbounded; every wrapping_mul/add must be masked."""
    for value in islice(splitmix64(0xFFFFFFFFFFFFFFFF), 100):
        assert 0 <= value < 1 << 64


# --- AC-4: byte-parity with the executed Rust bootstrap_ci -------------------


def test_bootstrap_ci_byte_matches_the_rust_expectations() -> None:
    """Same seed, same resamples, same deltas -> bit-identical CI bounds.

    Bit equality, not approx: the whole point of rule 4 (one RNG, one method,
    pinned) is that the Python port IS the Rust reference's arithmetic —
    `next_u64() % n` index mapping, sequential f64 accumulation, truncated
    order-statistic percentiles with NO interpolation.
    """
    data = _expectations()
    assert data["bootstrap_seed"] == "0x0E88B007574A9001"
    assert data["cases"], "expectations file carries no cases"
    for case in data["cases"]:
        deltas = [_f64(b) for b in case["deltas_bits"]]
        low, high = paired_bootstrap_ci(
            deltas, seed=BOOTSTRAP_SEED, resamples=case["resamples"]
        )
        assert _bits(low) == case["lo_bits"], case["name"]
        assert _bits(high) == case["hi_bits"], case["name"]


def test_bootstrap_ci_is_deterministic() -> None:
    deltas = [0.25, -0.5, 0.0, 1.0, -0.125, 0.375]
    first = paired_bootstrap_ci(deltas, seed=42, resamples=500)
    second = paired_bootstrap_ci(deltas, seed=42, resamples=500)
    assert first == second


def test_bootstrap_ci_depends_on_the_seed() -> None:
    deltas = [0.25, -0.5, 0.0, 1.0, -0.125, 0.375]
    assert paired_bootstrap_ci(deltas, seed=1, resamples=500) != paired_bootstrap_ci(
        deltas, seed=2, resamples=500
    )


def test_constant_deltas_collapse_the_interval() -> None:
    """Every resample mean of a constant series is that constant, so the
    truncated order statistics return it exactly at both bounds."""
    low, high = paired_bootstrap_ci([0.125] * 10, seed=7, resamples=100)
    assert (low, high) == (0.125, 0.125)


def test_empty_deltas_return_the_rust_empty_case() -> None:
    """The reference returns (0.0, 0.0, sigma=0.0) on empty input; the port
    keeps the bounds. n == 0 is reported as withheld upstream, never as a
    fabricated interval."""
    assert paired_bootstrap_ci([], seed=1, resamples=100) == (0.0, 0.0)
