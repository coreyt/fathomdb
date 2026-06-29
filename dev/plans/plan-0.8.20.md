# FathomDB 0.8.20 — Plan (Library Sweep — owned engine slices) · **Major dependency migrations**

> **Plan-as-LBS-runbook (engine depth).** Run via `/goal complete plan-0.8.20.md` acting as the
> **Library Bump Steward (LBS)**, spawning per-migration **Library Bump Orchestrators (LBOs)** for work
> that requires real engine changes + a full test matrix. Read first:
> `dev/plans/prompts/LIBRARY-BUMP-STEWARD.md` (charter),
> `dev/plans/prompts/LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md` (LBO prompt),
> `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (finding **F-12** — disposition of record; the 0.8.20
> row), `dev/plans/runs/NOTE-2026-06-29-library-sweep-to-0.8.x-steward.md`. Method runbook:
> `dev/design/orchestration.md`. Live state: stand up `runs/STATUS-0.8.20.md`.
>
> **⚠ TIMING IS NOT CONFIRMED (F-12).** Unlike the 0.8.11.1 contained sweep, 0.8.20 ships **real engine
> changes** and **must be strongly reviewed for timing-correctness before it proceeds.** Each migration
> here is **deferred-with-trigger** — it runs only when a concrete driver exists (a feature needs the
> new API, a security advisory lands, or the toolchain deprecates the old version). **Slice 0 is a
> mandatory timing/trigger review that PAUSES for HITL before any migration LBO is spawned.**
>
> **Theme.** Land the two migration-class dependency upgrades the 0.8.11.1 sweep deliberately excluded:
> the **napi 2→3** binding migration and the **rusqlite 0.31→0.40 + sqlite-vec** engine/storage
> migration. These are coupled, behavior-and-build-affecting, and need full Rust + cross-language
> testing — not a label-only micro.
>
> **Footprint.** Engine + binding builds (not label-only). Respect the single `.venv`/`maturin` build
> mutex; GPU not required. **Publishability is a Slice-0 question** — these change shipped crates, so
> unlike a transitory sweep this may need a real (HITL-gated) publish; resolve the OOB-vs-in-band label
> at Slice 0 (0.8.20 is even-numbered but was filed "net-new OOB" in F-12 — reconcile).

---

## 0. START HERE — timing/trigger review + HITL gate (mandatory, blocks everything)

The **first LBS action is a timing-correctness review**, per F-12. Do this and PAUSE before any LBO:

1. **Confirm a trigger exists for each migration.** For napi 2→3 and rusqlite 0.31→0.40: is there a
   concrete driver (a needed API, a security advisory, a toolchain/Cargo MSRV deprecation), or is the
   old version still fine? **No trigger ⇒ recommend continued deferral**, do not migrate for novelty.
2. **Check release collision.** Confirm 0.8.20 does not run adjacent to an already-heavy even release
   in a way that violates F-10 self-completion; confirm no in-flight orchestrator shares the engine
   crates.
3. **Re-verify current-vs-target from git** (versions/lockfile may have moved since 2026-06-29: `napi`
   = `"2"`, `napi-derive` = `"2"`, `@napi-rs/cli` = `^2.18.4`; `rusqlite` = `"0.31"`,
   `sqlite-vec` = `"=0.1.7"`).
4. **Surface the §11 HITL questions and PAUSE** (publish/label decision; per-migration go/defer).

---

## 1. Goal & scope

In scope — two **migration/wide** upgrades, each its own LBO, each its own worktree + branch + PR:

- **napi 2 → 3 (group: #90 + #102).** Bump `napi` **and** `napi-derive` (matched set — they cannot
  split) in `fathomdb-napi`, and `@napi-rs/cli` 2→3 in `src/ts`. Follow the napi-rs v3 migration guide
  (build-system + macro changes); rebuild the native addon; run the **TS + Py binding** suites and
  cross-language equivalence. Blast: **migration**.
- **rusqlite 0.31 → 0.40 + sqlite-vec (group: #103 + #99).** Bump `rusqlite` (across `fathomdb-engine`
  + `fathomdb-schema`, `bundled`) and **un-pin/raise** `sqlite-vec` (`=0.1.7` → 0.1.9) **together** —
  they are coupled through the bundled SQLite version. Resolve the API jump (9 minors); confirm the
  bundled-SQLite change does not regress FTS5/vector behavior. Blast: **wide**.

**Out of scope (deferred):** anything the 0.8.11.1 sweep already handles (sha2 unless it escalated here;
ts-tooling; CI actions). Any further library that is still `contained` belongs in the next Library
Sweep micro, not 0.8.20.

---

## 2. Requirements + acceptance criteria (DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal (falsifiable, offline) |
|----|-------------|------------------------------------------|
| R-20-0 | A trigger + timing review precedes any migration | Slice-0 review recorded; HITL go/defer per migration in `runs/STATUS-0.8.20.md` |
| R-20-1 | napi 2→3 builds + passes the **full** matrix (incl. `rust-macos`/`rust-windows`) | CI all-SUCCESS; native addon loads; TS + Py binding suites green |
| R-20-2 | napi cross-language equivalence preserved | Py↔TS functional harness equivalence green (no result/error-shape drift) |
| R-20-3 | rusqlite 0.31→0.40 builds + full Rust suite green across all OSes | `cargo test --workspace` green on linux/mac/windows |
| R-20-4 | Vector/FTS behavior preserved under the new bundled SQLite | recall/ANN-fidelity + FTS5 conformance unchanged (byte/score equivalence where the contract requires it); `sqlite-vec` load verified |
| R-20-5 | Every behavior/API change is registered → changelog | §9 register filled; each row ships a changelog line |
| R-20-6 | Publish/label decision honored | Slice-0 decision (publish vs label-only) executed exactly; any `v*` tag is HITL-gated |

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | **Timing/trigger review + HITL gate** (§0/§11); stand up `runs/STATUS-0.8.20.md`; ADR if an API contract shifts | steward-review + design-adr | — |
| **5** | LBO: **rusqlite + sqlite-vec** (coupled) — API migration; FTS5/vector equivalence; full Rust matrix | implementation | 0 (+ HITL go) |
| **10** | LBO: **napi 2→3** (napi + napi-derive + @napi-rs/cli) — binding migration; cross-language suites | implementation | 0 (+ HITL go) |
| **40** | **Verification + release readiness** — R-20-* DoD, X1/X2/X3, publish-or-label per Slice 0 | verification | 5,10 |

**Keystones / hard gates.** Slice 0 (timing/trigger + HITL go) **blocks both** migration LBOs.
Each migration is independently go/defer-able — if only one trigger fires, run only that LBO.

**Tracks (parallelizable).** Slices 5 ∥ 10 are independent **after** HITL go: 5 touches
`fathomdb-engine`/`fathomdb-schema` + `Cargo.lock`; 10 touches `fathomdb-napi` + `src/ts`. They share
`Cargo.lock` → the LBS **serializes their merges** (rebase-then-merge one at a time) even though they
develop in parallel worktrees. Binding rebuilds respect the single `.venv`/`maturin` mutex.

---

## 4. Reserved-gap policy (carried)

Gaps `1–4, 6–9` absorb unplanned follow-on (e.g. a migration exposes a needed new conformance test).
Fully orchestrated, not ad-hoc. **HALT to HITL on band overflow** — never spill into the next mod-5.

---

## 5. Cross-cutting DoD (bind every slice)

- **X1 — SDK parity + harnesses.** The napi migration is a binding change: Py + TS surfaces must stay
  equivalent with live functional harnesses (not symbol presence). Any error/result-shape change lands
  in both SDKs same-slice.
- **X2 — `mkdocs build` green** for any `docs/` touched.
- **X3 — docs/changelog per slice + `dev/DOC-INDEX.md`.** Migrations ship real changelog lines.

---

## 7. Prerequisites (before any LBO opens)

1. **Slice-0 HITL go recorded** for each migration (no migration without a trigger).
2. **`main` clean + current**; worktree-per-LBO from a verified `origin/main` tip; never the shared
   checkout. One `maturin develop` at a time.
3. **Baseline captured**: pre-migration recall/ANN-fidelity + FTS5 + cross-language numbers, so R-20-2
   and R-20-4 equivalence has a reference.

---

## 8. Override / duplication register (fill from a real grep at Slice 0)

| # | Concept | Divergent sources (file:line) | Live consequence if they drift |
|--:|---------|-------------------------------|--------------------------------|
| 1 | `rusqlite` pin | `fathomdb-engine/Cargo.toml` (×2) · `fathomdb-schema/Cargo.toml` | mixed SQLite ABI across crates → link/runtime breakage |
| 2 | `sqlite-vec` pin | `fathomdb-engine/Cargo.toml` · `fathomdb-schema/Cargo.toml` | vec-extension/bundled-SQLite mismatch |
| 3 | `napi`/`napi-derive` | `fathomdb-napi/Cargo.toml` + `@napi-rs/cli` (`src/ts/package.json`) | split versions → addon won't build |

LBOs must bump **all** occurrences of their group consistently.

---

## 9. Behavior-change register (fill as migrations land)

| # | Change | Who notices | Changelog entry |
|--:|--------|-------------|-----------------|
| 1 | bundled SQLite version change (rusqlite 0.40) | operators with on-disk DBs | "engine: bundled SQLite updated to … (rusqlite 0.40); no schema change" |
| 2 | napi v3 addon ABI / build | TS consumers, packagers | "node binding: built on napi-rs v3 (…)" |

If equivalence (R-20-2/R-20-4) is **not** byte/score-preserving, that is a behavior change requiring an
explicit HITL call — do not bury it.

---

## 10. Decisions taken (recorded)

- 2026-06-29 — These two migrations are **deferred-with-trigger**, routed to net-new 0.8.20 · F-12, HITL.
- 2026-06-29 — **0.8.20 timing must be strongly re-reviewed before proceeding** (Slice 0) · F-12, HITL.

---

## 11. Open questions for the human (raise at Slice 0, before LBOs)

1. **Trigger present?** For each of napi-3 and rusqlite-0.40 — is there a concrete driver now, or do we
   keep deferring? (Recommendation: migrate only on a real trigger; otherwise defer.)
2. **Publish vs label-only?** 0.8.20 changes shipped crates. Does it cut a real (HITL-gated) release, or
   stay an unpublished engine-validation branch until folded into a publishing release? Reconcile the
   even-number/"OOB net-new" labeling from F-12.
3. **Equivalence bar.** For rusqlite's bundled-SQLite change, is byte/score-identical recall+FTS
   required, or is a characterized, bounded delta acceptable (and who signs it)?
4. **Sequencing vs the even line.** Confirm 0.8.20 does not collide with an in-flight engine release
   (F-10 self-completion).

---

## 12. Out-of-band / parallel notes

- Coordinate with the program Steward (not just LBS): these touch core engine crates that even-line
  releases also edit — confirm no concurrent engine orchestrator before spawning the rusqlite LBO.
- This is the heaviest possible "library bump"; treat each migration like a real engine slice with full
  codex/§9 review, not a mechanical bump.

---

## 13. Immediate next slice

**Slice 0 — timing/trigger review.** Stand up `runs/STATUS-0.8.20.md`; for each migration confirm a
concrete trigger + re-verify current-vs-target from git; capture the equivalence baseline; **post the
§11 questions to HITL and PAUSE.** On per-migration go, spawn the rusqlite LBO (Slice 5) and/or the
napi LBO (Slice 10), each from `LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md` in its own worktree, with merges
serialized on `Cargo.lock`.
