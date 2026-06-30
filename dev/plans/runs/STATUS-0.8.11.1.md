# STATUS — 0.8.11.1 (Library Sweep #1, contained bumps) · LBS ledger

> Live state for `plan-0.8.11.1.md`. Label-only pico (F-13): **NO manifest bump, NO `v*` tag, NO
> publish.** Owned by the Library Bump Steward (LBS). Updated per slice.

## Baseline
- Tip: `origin/main` @ `7929d1a7` (== `main`, clean tree) — verified 2026-06-30.
- 0.8.11 complete + merged (PR #122). Sweep baselines off current `origin/main`.

## Slice ladder
| Slice | Title | State |
|------:|-------|-------|
| 0 | LBS setup + re-triage + raise HITL §11 | **DONE — HITL cleared 2026-06-30** |
| 5 | LBO: `sha2` 0.10→0.11 (#77) | **DONE → PR #138 GREEN (17 pass/1 skip), MERGEABLE.** Blast=**contained**; 1 breaking change (digest 0.11 dropped `LowerHex` on `Array` output → fixed 7 `{:x}` sites byte-identically); MERGE-recommended |
| 10 | LBO: `typescript` 5→6 (#67) + `@types/node` 25→26 (#92) | **LBO DONE → PR #137 GREEN, MERGEABLE. HELD for HITL review (user choice 2026-06-30).** Blast=**contained**; TS6 dropped auto-`@types` discovery → 96 errs root-caused to TS6 alone (`@types/node` 26 innocent); fix = 1 line `"types": ["node"]` in tsconfig, **no source edits, byte-identical `.js`+`.d.ts` emit**; 126/126 tests; one unrelated rust-macos flake re-ran green |
| 15 | LBO: `actions/checkout` 6→7 (#97) + `action-gh-release` 2→3 (#98) | **DONE.** #97→**PR #136 MERGED** (`fede9fd4`), 26 pins. #98→**DEFERRED to 0.8.20** (dry-run vacuous; comment posted on #98) |
| 20 | `dependabot.yml` reconciliation | **DONE — PR #135 MERGED** (`1c73d3f6`) |
| 40 | Sweep verification + closure | **DONE** — all 4 bumps landed + #98 deferred; DoD R-SW-1..6 met (below) |

## Landed state (2026-06-30, origin/main `3c695971`)
| PR | Bump | State | Merge commit |
|----|------|-------|--------------|
| #138 | sha2 0.10→0.11 | **MERGED** | `ff397708` |
| #136 | actions/checkout v6→v7 | **MERGED** | `fede9fd4` |
| #135 | dependabot.yml reconcile | **MERGED** | `1c73d3f6` |
| #137 | typescript 6 + @types/node 26 | **MERGED** (HITL-reviewed) | `3c695971` |
| #98 | action-gh-release v2→v3 | **DEFERRED → 0.8.20** (open, commented; added to `plan-0.8.20.md` §1) | — |

## Slice 40 — DoD readback (R-SW-1..6)
- **R-SW-1** — every in-scope bump dispositioned: #138/#136/#135/#137 **merged-green**; #98 **escalated-to-0.8.20** (recorded in `plan-0.8.20.md` §1 + comment on PR #98). No dangling PR. ✅
- **R-SW-2** — each merged bump green on its full CI matrix before merge (17 pass / 1 correct skip each; verified from `gh pr checks` on the merge commit). ✅
- **R-SW-3** — `sha2` blast evidenced: LBO posted call-site grep + `digest` 0.11 CHANGELOG; rated **contained** (only breaking change = `LowerHex` drop on `Array`, fixed byte-identically). Not assumed. ✅
- **R-SW-4** — `action-gh-release` v3 NOT merged unproven: the dry-run path is vacuous (skips the gated job), so it was **deferred, not forced**. The HITL "accept iff green dry-run" rule held. ✅
- **R-SW-5** — `dependabot.yml` coverage matches tracked manifests; no orphan ecosystems; deliberate exclusions documented (#135). ✅
- **R-SW-6** — label-only: no `version=`/`"version":` change in any shipped manifest (verified `7929d1a7..3c695971`), no `v*` tag created. ✅

**All 6 met. Sweep #1 COMPLETE.** Label-only pico: manifests unmoved, no tag, no publish.

## Worktree / branch / PR namespace (LBS-owned)
| Slice | Worktree | Branch | PR | State |
|------:|----------|--------|----|-------|
| 5 | (removed, merged) | (deleted) | #138 | MERGED |
| 10 | `fathomdb-worktrees/0.8.11.1-ts-tooling` | `lbo/ts-tooling-20260630` | #137 | HELD |
| 15 | (removed, merged) | (deleted) | #136 | MERGED |
| 20 | (removed, merged) | (deleted) | #135 | MERGED |
| LBS | `fathomdb-worktrees/0.8.11.1-lbs` | `lbs/ledger-0.8.11.1` | (ledger) | live |

All worktrees cut from verified `origin/main` tip `7929d1a7`. Merges stay HITL/Steward-gated (no self-merge).

> **Note — shared-checkout hazard (observed, not mine):** the concurrent 0.8.11.2 session committed to local `main` in the **primary checkout** (`34af4bbd`), diverging it from `origin/main`. This sweep was unaffected — all LBO/LBS work ran in isolated worktrees and landed via origin. Left their checkout untouched.

## HITL §11 answers (2026-06-30)
1. **#98 action-gh-release** — accept **iff release dry-run green** (else defer 0.8.20).
2. **#67 TypeScript 6** — **attempt; escalate to 0.8.20 if type churn is wide.**
3. **#77 sha2** — merge here **iff blast trivial/contained**, else escalate to 0.8.20.
4. **Label-only** — confirmed: NO manifest bump, NO `v*` tag, NO publish.

## Re-triage (verified 2026-06-30 from manifests on `main` + `gh pr list`)
All in-scope PRs still open; none merged/closed/superseded since 2026-06-29. Split holds.

### IN SCOPE — contained
| PR | Bump | Current on `main` | Target | Disposition |
|----|------|-------------------|--------|-------------|
| #77 | `sha2` | `0.10` (engine ×2, embedder ×3) | `0.11.0` | DO — Slice 5; LBO rates blast; `wide` ⇒ escalate to 0.8.20 |
| #67 | `typescript` (dev, /src/ts) | `^5.8.3` (PR base says 5.9.3 — drift) | `6.0.3` | DO — Slice 10 (grouped) |
| #92 | `@types/node` (dev, /src/ts) | `^25.6.0` | `26.0.1` | DO — Slice 10 (grouped) |
| #97 | `actions/checkout` | `v6.0.2` pinned-by-SHA (ci/release/perf-canonical/corpus-freeze) | `v7.0.0` | DO — Slice 15 |
| #98 | `action-gh-release` | `v2.6.1` (release.yml:545) | `v3.0.1` | DO — Slice 15, HITL-gated, release dry-run only |
| — | `dependabot.yml` | — | — | reconcile — Slice 20 |

### OUT OF SCOPE — deferred to 0.8.20 (migration-class)
| PR | Bump | Current on `main` | Reason |
|----|------|-------------------|--------|
| #102 | `napi-derive` 2→3 | `napi-derive = "2"` (napi crate) | binding migration → 0.8.20 (`plan-0.8.20.md`) |
| #90 | `@napi-rs/cli` 2→3 | `^2.18.4` (/src/ts) | couples with #102 → 0.8.20 |
| #103 | `rusqlite` 0.31→0.40 | `0.31` (schema + engine ×2) | engine/migration, couples w/ sqlite-vec → 0.8.20 |
| #99 | `sqlite-vec` =0.1.7→0.1.9 | `=0.1.7` (schema + engine) | bundled-SQLite version-coupled w/ rusqlite → 0.8.20 |

### Noise — do not chase
- 2 Dependabot security alerts (`idna` / `torch`) live in gitignored eval env (`python/uv.lock`), not
  shipped code. Per plan §1 + decision log.

## Decisions / events
- 2026-06-30 — Slice 0 opened; re-triage confirms the §1 split unchanged; §11 questions raised to HITL; PAUSED.

## HITL §11 questions (raised 2026-06-30, awaiting answers)
1. `action-gh-release` v3 (#98): accept iff green release dry-run, or defer to 0.8.20?
2. TypeScript 6 major (#67): attempt + fix type churn in-scope, or defer if wide?
3. `sha2` escalation rule: merge here iff blast trivial/contained, else → 0.8.20 — confirm?
4. Label-only confirmation: no publish/tag for this sweep — confirm?
