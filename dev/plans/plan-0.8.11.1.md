# FathomDB 0.8.11.1 — Plan (Library Sweep #1) · **Contained dependency bumps**

> **Plan-as-LBS-runbook.** This is a **Library Sweep**, not an engine release. Run via
> `/goal complete plan-0.8.11.1.md` acting as the **Library Bump Steward (LBS)** — an
> orchestrator-of-orchestrators that re-triages, spawns per-library **Library Bump Orchestrators
> (LBOs)**, navigates CI, and lands PRs. Read first:
> `dev/plans/prompts/LIBRARY-BUMP-STEWARD.md` (charter),
> `dev/plans/prompts/LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md` (LBO prompt),
> `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (finding **F-12** — the disposition of record),
> `dev/plans/runs/NOTE-2026-06-29-library-sweep-to-0.8.x-steward.md` (Steward hand-off). Method runbook:
> `dev/design/orchestration.md`. Live state: stand up `runs/STATUS-0.8.11.1.md`.
>
> **Label caveat (HITL 2026-06-29).** `0.8.11.1` is a **one-time pico exception, explicitly NOT a
> precedent** — pico (`x.y.z.p`) is decanonized; the standing sweep-label convention is non-pico and
> TBD (and `13` is forbidden). This sweep is **transitory + label-only**: NO manifest version bump, NO
> `v*` tag, NO publish.
>
> **Theme.** Clear the *contained, low-risk* dependency bumps from the post-0.8.11 Dependabot backlog
> under the new LBS program, so the desk is clean before the heavy even-line work resumes. Migration-
> class bumps are explicitly out of scope (→ 0.8.20).
>
> **Footprint.** Dependency bumps only; the in-library query path is unchanged (label-only, no behavior
> change intended). One bump (`action-gh-release`) touches the **release workflow** → exercise via a
> **release dry-run**, never a real publish. CI is advisory (main is not branch-protected,
> `dont-gate-trivial-changes-on-ci`), but **each bump must be green before merge** and merges are
> HITL/Steward-gated (no blind auto-merge).

---

## 0. START HERE — review + HITL gate (do this before any LBO)

The **first LBS action is to review the work in front of it**, because the backlog drifts:

1. **Re-triage from git/gh.** List open `app/dependabot` PRs; for each, re-run the relevance test
   (target manifest tracked on `main`? direct dep or transitive-only? already at/past target in the
   lockfile? security-driven?). Versions move — e.g. on 2026-06-29 `typescript` on `main` was already
   `5.8.3` while #67's recorded base said `5.9.3`. Re-confirm current-vs-target per PR.
2. **Re-confirm the split.** Verify the contained set below still matches reality and nothing has been
   merged/closed/superseded since 2026-06-29.
3. **Surface the §11 HITL questions and PAUSE.** Do not spawn LBOs until the human answers the
   blocking questions (release-path risk on `action-gh-release`; TS-major acceptance; sha2 escalation).

---

## 1. Goal & scope

In scope — the **contained** bumps (one LBO per group/singleton; LBO rates blast and may escalate):

- **`sha2` 0.10 → 0.11 (#77).** RustCrypto major used across ~5 crates (engine/embedder/py/napi/cli).
  LBO rates blast; **if the rating comes back `wide`, escalate to 0.8.20** rather than forcing it here.
- **`typescript` 5 → 6 (#67) + `@types/node` 25 → 26 (#92).** `src/ts` dev tooling, **grouped** (shared
  TS type-check). Risk = new type-check errors surfacing.
- **`actions/checkout` 6 → 7 (#97).** Pinned-by-SHA across every workflow (`ci.yml`, `release.yml`,
  `perf-canonical.yml`, `corpus-freeze.yml`).
- **`action-gh-release` 2 → 3 (#98).** Touches `release.yml` (publish path) → exercise via a **release
  dry-run** (`workflow_dispatch`), never a real `v*` tag/publish. **HITL-gated.**
- **`dependabot.yml` reconciliation** — confirm coverage matches real manifests so orphan PRs stop
  being generated.

**Out of scope (deferred):** `napi` 2→3 (#90 + #102) and `rusqlite` 0.31→0.40 + `sqlite-vec` (#103 +
#99) → **0.8.20** (engine/migration, `plan-0.8.20.md`). Already handled: #96 prettier (merged), the 7
orphans + #59 (closed). The 2 Dependabot security alerts (`idna`/`torch`) are **noise** (gitignored
local eval env `python/uv.lock`, not shipped code) — do not chase them here.

---

## 2. Requirements + acceptance criteria (DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal (falsifiable, offline) |
|----|-------------|------------------------------------------|
| R-SW-1 | Every in-scope bump is dispositioned: merged-green, escalated-to-0.8.20, or closed-with-reason | Per-PR final state recorded in `runs/STATUS-0.8.11.1.md`; no PR left dangling |
| R-SW-2 | Each merged bump is green on its full CI matrix before merge | CI rollup all-SUCCESS on the merge commit; recorded |
| R-SW-3 | `sha2` blast rating is evidenced (not assumed) | LBO posts call-site grep + CHANGELOG diff; `wide` ⇒ escalation recorded, not merged here |
| R-SW-4 | `action-gh-release` v3 proven non-breaking without publishing | `release.yml` `workflow_dispatch` dry-run succeeds; no `v*` tag pushed |
| R-SW-5 | `dependabot.yml` coverage matches tracked manifests | diff (or "no change needed") committed; orphan-target ecosystems confirmed absent |
| R-SW-6 | Label-only: no manifest version bump / tag / publish | `git diff` shows no `version =`/`"version":` change in shipped manifests; no tag created |

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | LBS setup + **review/re-triage + raise HITL questions** (§0/§11); stand up `runs/STATUS-0.8.11.1.md` | steward-review | — |
| **5** | LBO: **`sha2`** — blast-rate; bump or escalate; rust unit/integration | implementation | 0 (+ HITL clear) |
| **10** | LBO: **`typescript` + `@types/node`** (grouped) — type-check + TS suite green | implementation | 0 |
| **15** | LBO: **`actions/checkout` + `action-gh-release`** (grouped) — workflow bump + **release dry-run** | implementation | 0 (+ HITL clear on #98) |
| **20** | **`dependabot.yml` reconciliation** | steward-op | 0 |
| **40** | **Sweep verification + closure** — DoD R-SW-1..6, ledger, Steward readback | verification | 5,10,15,20 |

**Keystones / hard gates.** Slice 0's HITL gate blocks 5/15 (sha2 escalation call; `action-gh-release`
publish-path risk). Slice 15's `action-gh-release` merge is HITL-gated on a green dry-run.

**Tracks (parallelizable).** Slices 5 ∥ 10 ∥ 15 are independent **once HITL clears Slice 0** — each its
own LBO in its **own worktree** cut from a verified `origin/main` tip. Shared-file serialization:
`Cargo.lock` (sha2 LBO) vs `package-lock.json`/`src/ts` (ts LBO) vs `.github/workflows` (actions LBO)
do not overlap, so all three may run concurrently. If `sha2` rebuilds the Py/NAPI bindings, respect the
single `.venv`/`maturin` mutex.

---

## 4. Reserved-gap policy (carried)

Planned slices are multiples of 5; gaps `1–4, …` are the insertion mechanism for unplanned follow-on a
slice reveals (e.g. a bump exposes a needed new test). A reserved-gap slice is fully orchestrated (own
worktree, own closure), not an ad-hoc patch. **HALT to HITL if a gap band fills** — never overflow into
the next mod-5. (Full statement: `0.8.1-plan.md`.)

---

## 5. Cross-cutting DoD (bind every slice)

- **X1 — SDK parity.** No SDK surface changes here (dev-tooling/CI bumps), but the TS bump must keep the
  TS build + functional harness green; confirm no Py↔TS drift introduced.
- **X2 — `mkdocs build` stays green** if any `docs/` touched (none expected).
- **X3 — docs/changelog.** Label-only sweep ⇒ a single changelog/maintenance note (no version bump);
  update `runs/STATUS-0.8.11.1.md` per slice.

---

## 7. Prerequisites (before any LBO opens)

1. **`main` is clean and current** (`git rev-parse --abbrev-ref HEAD` = `main`; `== origin/main`).
2. **Worktree-per-LBO hygiene:** each LBO gets a unique worktree cut from a verified `origin/main` tip;
   never the shared/primary checkout (`shared-checkout-branch-can-be-stale-vs-session-env`,
   `agent-worktree-stale-base-trap`). One `maturin develop` at a time for any binding rebuild.
3. **0.8.11 is complete + merged** (PR #122) — this baselines off current `origin/main`.

---

## 8. Override / duplication register

Reviewed 2026-06-29: the only multi-source contract in scope is the **action-version pin** (each
`uses: …@<sha> # vX`) repeated across `ci.yml`/`release.yml`/`perf-canonical.yml`/`corpus-freeze.yml` —
the `actions/checkout` LBO must bump **all** occurrences consistently (drift = mixed runner versions).

---

## 9. Behavior-change register

Reviewed 2026-06-29: this sweep is intended to ship **no consumer-visible behavior change** (dev/CI
tooling + transitive hashing lib). If any LBO finds a behavior change (e.g. `sha2` output framing, a TS
type-narrowing that changes emitted JS), it **escalates** — that bump leaves the sweep.

---

## 10. Decisions taken (recorded)

- 2026-06-29 — Library Sweep program + this split established · F-12, HITL.
- 2026-06-29 — `0.8.11.1` pico label is a **one-time exception**, not the sweep model · HITL.
- 2026-06-29 — The 2 Dependabot security alerts are **noise** (gitignored eval env) · do not chase.

---

## 11. Open questions for the human (raise at Slice 0, before LBOs)

1. **`action-gh-release` v3 (#98)** — accept after a green `release.yml` dry-run, or defer to 0.8.20
   alongside other release-path work? (Recommendation: accept iff dry-run green; it gates publishing.)
2. **TypeScript 6 major (#67)** — accept new type-check errors as in-scope fix work, or defer if the TS
   suite turns out wide? (Recommendation: attempt; escalate if the type churn is large.)
3. **`sha2` escalation threshold** — confirm the rule "merge here iff blast = trivial/contained; else
   → 0.8.20." (Recommendation: yes.)
4. **Label-only confirmation** — confirm no publish/tag for this sweep (transitory micro).

---

## 13. Immediate next slice

**Slice 0 — LBS review.** Stand up `runs/STATUS-0.8.11.1.md`; re-triage the open Dependabot PRs from
git/gh; re-confirm the contained split; **post the §11 questions to HITL and PAUSE.** On clear, fan out
Slices 5 (sha2) ∥ 10 (ts-tooling) ∥ 15 (ci-actions), each as an LBO seeded from
`LIBRARY-BUMP-ORCHESTRATOR-TEMPLATE.md` in its own worktree.
