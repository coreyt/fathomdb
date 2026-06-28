# Steward review + verdict — 0.8.9 Slice 20 (F-9): pyo3 `extension-module` mac/win link fix

**From:** Program Steward · **Date:** 2026-06-28 · **Re:** PR #104 (`0.8.9-slice20-pyo3-link`), master finding F-9.
**Verdict: the fix is SAFE — no at-risk build path.** Basis: two steward-commissioned investigations (CI bisect + root-cause) + steward git-verification.

## The fix
Drop the always-on `extension-module` from the `src/rust/crates/fathomdb-py/Cargo.toml` pyo3 dep-line (keep `abi3-py310`). Repairs the `rust-macos`/`rust-windows` `cargo test --workspace` link failure.

## Investigation 1 — regression-or-pre-existing (CI bisect)
`rust-macos`/`rust-windows` have failed `cargo test --workspace` since **2026-05-21** (`80ffa6bd`, first run under the current job names) — **~5 weeks before** the pyo3 0.24→0.29 bump (`8c938bb7`, 2026-06-27); byte-identical link errors (`_PyDict_GetItemWithError`, `_PyExc_*`) before and after. **Verdict: pre-existing, NOT a 0.8.8 / pyo3-0.29 regression.** Every release since 0.8.6 merged over this red.

## Investigation 2 — why `extension-module` was introduced (root-cause)
- **Intro commit `07672725`** (2026-05-16, "feat(11a): PyO3 binding — fathomdb-py crate"). Hand-authored from the slice prompt `dev/plans/prompts/11a-pyo3-binding.md:96-97`, which copied maturin's documented scaffolding pattern verbatim — a **maturin-template default**, not a reasoned standalone decision.
- **Recorded rationale for the dep-line always-on:** none found (no ADR/design/comment).
- **Redundant from day one:** the same intro commit's `pyproject.toml:24` already carried `[tool.maturin] features = ["pyo3/extension-module"]`, so the Cargo dep-line was a redundant second carrier (steward-verified: `git show 07672725:src/python/pyproject.toml`).

## Build-path inventory (every path compiling `fathomdb-py`)
| Path (file:line) | Importable ext? | Passes `extension-module`? |
|---|---|---|
| release.yml:103 (shipped PyPI wheel) | Yes | Y explicit |
| ci.yml:277 (wheel-feature size gate) | Yes | Y explicit |
| ci.yml:286 (wheel-nofeature info) | Yes | Y explicit |
| pyproject.toml:52 `[tool.maturin].features` → `pip install -e .` / `maturin develop`/`build` / sdist→pip | Yes | Y inherited |
| conftest fixture (pyproject.toml:49) | Yes | Y explicit |
| Cargo.toml:30 dev/GPU command | Yes | Y explicit |
| ci.yml:167/184 `cargo test --workspace` (win/mac) | No | N — *the path the fix repairs* |
| release.yml:185 `cargo build --release --workspace` | No (compile check) | N — benefits from removal |

## Verdict — (b) redundant maturin-template default; SAFE
Every path producing an importable extension routes through maturin (sources the feature from `pyproject.toml:52`) and/or passes it explicitly in CI — double-covered, independent of the Cargo dep-line. The only paths lacking it produce no wheel and are exactly what the always-on mis-linked on strict macOS/Windows linkers; removing it fixes them. **No at-risk path; no bare `maturin build` lacking the feature.**

## Steward git-verification (trust git, not narration)
- `07672725:pyproject.toml:24` carried the maturin feature → dep-line redundant from day one. ✅
- current `pyproject.toml:52` carries `pyo3/extension-module` → maturin paths covered. ✅
- `ci.yml:167/184` (`cargo test --workspace`) + `release.yml:185` (`cargo build --workspace`) pass no feature → no-wheel paths the fix repairs. ✅

## PR #104 merge gates
1. **Safety / completeness — CLEARED** (this note).
2. **Functional** — CI `rust-macos`/`rust-windows` flip green (orchestrator's watch). Once green → HITL sign-off → merge → on-main re-verify closes 0.8.9.

*Reviews commissioned + verified by the Program Steward, 2026-06-28 (bisect + root-cause subagents, git-confirmed).*

## UPDATE 2026-06-28 — MERGED; final scope + honest-green verified

PR #104 **MERGED** (origin/main `1cb1c7ac`); `rust-macos` + `rust-windows` **GREEN** (first-ever Windows green). **The merged fix grew beyond the one-line drop this note was first scoped to:** dropping `extension-module` made the Windows suite finally LINK, exposing **5 cross-platform test gaps** previously masked because the suite never compiled. Slice 20 closed them: two principled `#[cfg(unix)]`/`#![cfg(unix)]` gates (`durability_soak` SIGKILL/re-exec power-cut harness; `migrations` bash-linter exec — the core accretion-guard test stays ungated), benign portability fixes (`reader_pool` bounded worker wait, `lifecycle_observability` relative-path artifact check, `qd_envelope_deserialize`/`stub_harness.py` UTF-8 stdio), and a contract-preserving macOS de-flake (one change corrects a test back toward its documented `drain` contract).

**Steward adversarial honesty-review (commissioned + git-spot-checked):** HONEST and coverage-preserving, high confidence — no core-engine product behavior gated off Windows; ci.yml has no `continue-on-error`/allow-failure (`--no-fail-fast` enumerates failures but still fails the job). **Caveat:** `compatibility.rs` relaxes only the `holder_pid` diagnostic on Windows (LockFileEx blocks PID readback); the load-bearing lock-rejection/close-release semantics stay fully asserted; unverified on a real Windows host.

**Both PR #104 gates satisfied** (functional green + safety/honest). **0.8.9 fully complete.**
