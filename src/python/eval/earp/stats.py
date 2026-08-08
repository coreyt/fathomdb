"""S8 — pinned comparison statistics: one RNG, one method.

Pure: no SDK, no database, no network, and deliberately NO numpy — the
numerics here are byte-faithful to the Rust harness's `bootstrap_ci`
(`src/rust/crates/fathomdb-engine/tests/eu8_ir_validation.rs:283-304`), whose
percentiles are truncated-index order statistics over sorted resample means,
NOT interpolated. `np.percentile`'s interpolation (and PCG64, as used by the
repo's `paired_bootstrap_delta` forks) would be a different method wearing the
same name; EARP adopts the Rust reference, not the forks.

The RNG is SplitMix64 (`eu8_ir_validation.rs:104-121`), pinned two ways: the
published Vigna test vectors (seed 0 -> 0xE220A8397B1DCDAF, ...) and an
executed expectations file generated once from a verbatim standalone copy of
the Rust functions (`tests/earp/fixtures/bootstrap_parity.json`; provenance in
`tests/earp/test_stats.py`). Index mapping is the reference's `next_in`:
`next_u64() % n` (`:118-120`), not rejection sampling — the modulo bias is
part of the pinned method.

Design of record: `dev/design/earp-slice-8-design.md`.
"""

from __future__ import annotations

from typing import Iterator, Sequence

_MASK64 = (1 << 64) - 1
_GAMMA = 0x9E3779B97F4A7C15
_MUL1 = 0xBF58476D1CE4E5B9
_MUL2 = 0x94D049BB133111EB


def splitmix64(seed: int) -> Iterator[int]:
    """The reference's SplitMix64 as an infinite u64 stream.

    Python ints are unbounded, so every wrapping add/mul is masked to 64 bits;
    without the masks the mixer would silently compute different values from
    the Rust `wrapping_*` ops for any state past 2^64.
    """
    state = seed & _MASK64
    while True:
        state = (state + _GAMMA) & _MASK64
        z = state
        z = ((z ^ (z >> 30)) * _MUL1) & _MASK64
        z = ((z ^ (z >> 27)) * _MUL2) & _MASK64
        yield (z ^ (z >> 31)) & _MASK64


def paired_bootstrap_ci(
    deltas: Sequence[float], *, seed: int, resamples: int, alpha: float = 0.05
) -> tuple[float, float]:
    """Percentile bootstrap over paired per-query deltas, byte-faithful to the
    Rust `bootstrap_ci`.

    Every arithmetic choice below is the reference's, deliberately:

    * resample n indices per round via `next_u64() % n`;
    * plain sequential f64 accumulation of each resample mean (no pairwise or
      Kahan summation — a "better" sum would break bit parity);
    * sort the means, then take TRUNCATED order statistics:
      `lo = means[int(resamples * alpha/2)]`,
      `hi = means[min(int(resamples * (1 - alpha/2)), resamples - 1)]` —
      no interpolation.

    The empty case returns (0.0, 0.0) as the reference does; the caller
    reports n == 0 as a withheld claim, never as a real interval.
    """
    if not deltas:
        return (0.0, 0.0)
    n = len(deltas)
    rng = splitmix64(seed)
    means: list[float] = []
    for _ in range(resamples):
        acc = 0.0
        for _ in range(n):
            acc += deltas[next(rng) % n]
        means.append(acc / n)
    means.sort()
    low = means[int(resamples * (alpha / 2))]
    high = means[min(int(resamples * (1 - alpha / 2)), resamples - 1)]
    return (low, high)


__all__ = ["paired_bootstrap_ci", "splitmix64"]
