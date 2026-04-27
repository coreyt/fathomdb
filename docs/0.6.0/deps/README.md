---
title: 0.6.0 Dependency Audit — Index
date: 2026-04-25
target_release: 0.6.0
desc: Per-dep audit verdicts (keep|drop|replace) + alternatives
blast_radius: workspace + python + typescript SDK
status: living
---

# Dependency Audit

One file per direct third-party dep at `deps/<dep-name>.md`. Living folder during
0.6.0 (individual files are lockable). Transitives are out of scope unless
flagged by `cargo audit` / `cargo deny`.

## Cross-platform requirement (HITL 2026-04-25)

**Every direct dep must build clean on all four target platforms:** Linux x86_64,
Linux aarch64, macOS, Windows. HITL-approved exceptions only. Per-dep file
records each platform's build status; any "asserted" or "unverified" entry must
be promoted to CI evidence before the dep is locked. Aarch64-Linux is
load-bearing per memory `feedback_cross_platform_rust` (c_char i8/u8 split).

## Tooling signal availability (2026-04-27)

| Tool | Status |
|------|--------|
| `cargo tree -e normal --workspace --depth 1` | available, run |
| `cargo audit` | available, run — see transitive findings below |
| `cargo deny check` | **installed + run; clean** (config: `deny.toml`). Advisories ok / bans ok / licenses ok / sources ok. No flips. |
| `cargo udeps --workspace` | install failed (libssl-dev missing on aarch64); HITL pending — sudo apt install libssl-dev pkg-config OR rustls-feature variant |
| `cargo outdated -R` | install failed (same as udeps) |

### `cargo audit` — transitive findings
- RUSTSEC-2026-0097 — `rand 0.9.2` unsound with custom logger. Reaches us via `ulid`, `tokenizers`, `candle-*`, `hf-hub`/`reqwest`/`quinn`, `rand_distr`, `criterion`. Not exploitable in our paths (we never install a custom rand logger). Track upstream rand bumps.
- RUSTSEC-2024-0436 — `paste 1.0.15` unmaintained (not vulnerable). Reaches us via `tokenizers`, `pulp`. Macro-only crate, low risk. Watch for replacement upstream.

## HITL decisions (2026-04-25)

Critic-B ran against this audit and surfaced 12 findings. HITL resolutions:

- **F3** `sqlite-vec` sole-maintainer risk — accepted; **no fallback plan, no vendored fork.** Decision recorded in `sqlite-vec.md`. No further work.
- **F4** `rusqlite` async-surface ADR — promoted to **Phase 1 ADR queue** (frames every public 0.6.0 API).
- **F5** `tokenizers` + `paste` transitive — covered by global all-platforms requirement above; `paste` watch in transitive findings.
- **F6** `candle-core` keep — settled per HITL: candle is the chosen embedder stack for 0.6.0. Architecture per NOTE 1 (candle + tokenizers + sqlite-vec, manual mean-pool + L2-normalize, zerocopy BLOB to vec0). ADR records the decision.
- **F7** `safetensors` direct dep — flipped to **drop**; use candle re-export.
- **F8** `sentence-transformers` (python) — flipped to **drop**; redundant with candle Rust path.
- **F9** `windows-sys` 0.59 → 0.60 bump approved (debloat Windows build dup-versions).
- **F10** `hf-hub` cache-compat fixture — declined; use what is there + works.
- **F11** `toml` — flipped to **drop**; **JSON-only operator config in 0.6.0+**. Removes extension-branch in `load_vector_regeneration_config`.

## Verdict summary

Sorted: drops first, then replace, then keep.

| Dep | Ecosystem | Verdict | Replacement | Notes |
|-----|-----------|---------|-------------|-------|
| [toml](toml.md) | rust (engine) | drop | serde_json | HITL: JSON-only operator config in 0.6.0+. Removes dual-format `load_vector_regeneration_config` branch. |
| [safetensors](safetensors.md) | rust (default-embedder) | drop | candle re-export | HITL F7: candle-* re-exports safetensors; no need for direct dep. |
| [sentence-transformers](sentence-transformers.md) | python (optional) | drop | candle Rust path | HITL F8: redundant with candle stack chosen for 0.6.0. |
| [hf-hub](hf-hub.md) | rust (engine, default-embedder) | replace | thin `ureq` GET + cache | Drags in reqwest+quinn+tokio for "GET 4 files once". ~+120/-40 LoC. Use existing cache layout that works (HITL F10 — no fixture). |
| [rusqlite](rusqlite.md) | rust (engine, schema) | keep | — | Sole viable embedded sync engine + extension loader. **Phase 1 ADR queue**: async-surface decision (HITL F4). |
| [sqlite-vec](sqlite-vec.md) | rust (engine, schema) | keep | — | Only ANN as SQLite VT. Sole-maintainer risk **accepted, no fallback** per HITL F3. |
| [serde](serde.md) | rust (all) | keep | — | De-facto standard. |
| [serde_json](serde_json.md) | rust (all) | keep | — | Pairs with serde. Operator config format in 0.6.0+. |
| [sha2](sha2.md) | rust (engine, schema) | keep | — | Locked in by persisted identity hashes. |
| [thiserror](thiserror.md) | rust (all) | keep | — | Standard, minimal. |
| [tracing](tracing.md) | rust (all, optional) | keep | — | Feature-gated observability. |
| [tracing-subscriber](tracing-subscriber.md) | rust (engine, optional) | keep | — | Pairs with tracing. |
| [ulid](ulid.md) | rust (engine) | keep | — | Stable; transitive rand advisory not triggered. |
| [napi](napi.md) | rust (TS SDK) | keep | — | Only viable Node binding for our core. |
| [napi-derive](napi-derive.md) | rust (TS SDK) | keep | — | Pairs with napi. |
| [napi-build](napi-build.md) | rust (TS SDK build) | keep | — | Build helper. |
| [pyo3](pyo3.md) | rust (Python SDK) | keep | — | Only viable Python ext-module gen. abi3 a 0.7 followup. |
| [pyo3-log](pyo3-log.md) | rust (Python SDK) | keep | — | log → Python logging bridge. |
| [candle-core](candle-core.md) | rust (default-embedder) | keep | — | HITL F6: chosen embedder stack for 0.6.0; ADR records architecture (NOTE 1). |
| [candle-nn](candle-nn.md) | rust (default-embedder) | keep | — | Pairs with candle-core. |
| [candle-transformers](candle-transformers.md) | rust (default-embedder) | keep | — | BGE model. |
| [tokenizers](tokenizers.md) | rust (default-embedder) | keep | — | WordPiece needed for BGE; all-platform CI required (HITL). |
| [windows-sys](windows-sys.md) | rust (engine, win32) | keep (bump 0.59→0.60) | — | HITL F9: bump approved to debloat Windows dup-versions. |
| [criterion](criterion.md) | rust (dev) | keep | — | Standard bench. |
| [insta](insta.md) | rust (dev) | keep | — | Snapshot tests. |
| [rstest](rstest.md) | rust (dev) | keep | — | Parameterized tests. |
| [tempfile](tempfile.md) | rust (dev) | keep | — | Temp dirs in tests. |
| [maturin](maturin.md) | python (build) | keep | — | Standard PyO3 backend. |
| [httpx](httpx.md) | python (optional) | keep | — | OpenAI/Jina embedder clients. |
| [click](click.md) | python (optional `cli`) | keep | — | Console script. |
| [typescript](typescript.md) | ts (devDep) | keep | — | Required. |
| [tsup](tsup.md) | ts (devDep) | keep | — | Esbuild wrapper. |
| [vitest](vitest.md) | ts (devDep) | keep | — | Test runner. |
| [@types/node](types-node.md) | ts (devDep) | keep | — | Node typings. |

## Followups for Phase 2

- Install `cargo deny`, `cargo udeps`, `cargo outdated` on CI; rerun audit. udeps in particular may surface additional drops not detected in this pass. **HITL: any flips trigger HITL review before action.**
- ADR (Phase 1, promoted): `rusqlite` async-surface decision — sync-only vs sqlx-based async layer. Frames every public 0.6.0 API.
- ADR (Phase 1, decision-recording): default-embedder architecture per NOTE 1 — candle + tokenizers + sqlite-vec, manual mean-pool + L2-normalize, zerocopy BLOB.
- ADR (Phase 1, decision-recording): `sqlite-vec` accept-no-fallback (sole-maintainer risk acknowledged; no vendored fork).
- ADR (Phase 1, decision-recording): operator config = JSON-only.
- Followup: replace `hf-hub` with thin `ureq` downloader (~+120/-40 LoC). Preserve current cache layout — no fixture.
- Followup: bump `windows-sys` to 0.60 (separate implementer change touching `Cargo.toml`).
- Followup: PyO3 abi3 migration (compile once for all CPython 3.10+).
- Followup: aarch64-Linux + Windows + macOS CI evidence per dep before lock — replaces "asserted" with "verified" in each dep file.
- Followup: `paste` (unmaintained) replacement watch — reaches us via tokenizers + pulp.
