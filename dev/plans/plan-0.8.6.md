# FathomDB 0.8.6 — Plan (state-machine ladder) · **Foundations & shippability**

> **Plan-as-state-machine.** Mod-5 slice ladder + reserved-gap policy + the live "Immediate Next Slice"
> pointer. Authoritative slice contracts graduate to `0.8.6-implementation.md`; live state lives in
> `runs/STATUS-0.8.6.md`. Sequencing/deps are in `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (the program
> decision record). Run via `/goal complete 0.8.6` as an **orchestrator** session
> (`prompts/0.8.x-PROGRAM-STEWARD-HANDOFF.md`).
>
> **Theme.** The enabler layer: collapse N provider contracts into one (#8), migrate the consumer onto
> governed read verbs (#9), and stand up the **minimal viable publish path** (#11-min) so every
> micro-release from here on is genuinely DoD-shippable. **All-$0** — mechanism/CI only, no priced runs,
> no experiments. Designed to run beside the experiment program without contending for budget.
>
> **Footprint.** #8/#7-seam = CALLER-SIDE-BYO-LLM (no LLM in the library). #9 = IN-LIBRARY governed
> surface. #11 = CI/CD. Library query path stays CPU-only/deterministic.

---

## 1. Goal & scope

Land the three prerequisites the rest of the 0.8.6→0.8.16 line depends on:

- **#8 — Generalized provider protocol (OPP-8, AGREED).** One typed-task provider contract (transport +
  schema + error model) that ELPS-extract, consolidation (OPP-2), and community-summarize (OPP-4) all
  ride on, replacing N narrow sibling contracts. *Why first:* the ledger requires OPP-2 be built "as
  ONE generalized provider per OPP-8" — building 0.8.10's consolidation provider before this forces a
  throwaway contract + rewrite.
- **#9 — Governed-verb coupling hygiene (OPP-5, AGREED).** Ensure the governed read surface (G2–G4 read
  verbs, `graph.search_expand` G6) is complete/stable enough to be the consumer boundary, and land the
  FathomDB-side changes the Memex migration needs. *Why first:* HITL ruled the consumer migrates onto
  governed verbs **before** OPP-1/2/4 layer on top — it is a correctness prerequisite for 0.8.10.
- **#11-min — Minimal viable publish path.** Rewrite `set-version.sh` to the two version axes (Axis W
  workspace+bindings lockstep; Axis E independent `fathomdb-embedder-api`), restore `release.yml` to the
  8-tier topological publish order (`design/release.md`), wire `verify-release-gates`/
  `check-version-consistency`, **dry-run publish**, and push the ~178-commit `main` backlog (HITL-gated).
  *Why first:* "Fathom is DoD at the end of each micro-release" is hollow without a working publish path.

**Out of scope (deferred):** the heavy publish matrix (multi-OS napi prebuilds, full cross-ecosystem
gate) → **0.8.16 #11-full**; the benchmark/robustness harness → **0.8.16 #13**.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-PP-1 | One provider protocol covers extract/consolidate/summarize as typed tasks | A single trait/schema; the existing ELPS extractor re-expressed on it with **byte-identical** output; codex §9 confirms no second transport remains |
| R-PP-2 | Provider protocol is BYO-LLM, caller-side only | No LLM/network symbol reachable from the library query path; footprint test green |
| R-CH-1 | Governed read surface is the complete consumer boundary | Conformance test enumerates the governed verbs a consumer needs; no internal-engine reach required for the OPP-5 read paths |
| R-CH-2 | Consumer-facing change lands in **both** Py + TS SDKs | X1 cross-binding functional harness green |
| R-REL-1 | `set-version.sh` enforces both version axes | `--check-files` passes on a deliberately-skewed fixture (RED→GREEN) |
| R-REL-2 | `release.yml` restored to 8-tier order; **dry-run publish green** | `workflow_dispatch` dry-run succeeds end-to-end; no real publish |
| R-REL-3 | The `main` backlog is pushable and pushed | `origin/main` == local `main` after HITL-gated push; pre-push hook restored |

New ACs (if minted): candidates **AC-078+** at Slice 0 (provider-protocol conformance) and at Slice 40
(release-readiness), per the locked-acceptance policy below.

---

## 3. Slice ladder (mod-5)

```
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR Kickoff — board + plan triad; **provider-protocol ADR** (OPP-8 typed-task contract); coupling-hygiene scope ADR (OPP-5 governed-surface completeness); release-policy confirmation | design-adr | — |
| **5** | **Provider-protocol KEYSTONE** — the one typed-task trait + schema + error model; re-express the ELPS extractor on it (byte-identical) | implementation | 0 |
| **10** | **Coupling hygiene** — complete/stabilize the governed read surface (G2–G4 / G6) as the consumer boundary; Py+TS parity | implementation | 0 |
| **15** | **Release-enablement** — `set-version.sh` two-axis rewrite + `release.yml` 8-tier restore + verify-gates + dry-run publish | implementation (CI) | 0 |
| **20** | **Backlog push** — pre-push hook restore; HITL-gated push of `main` to `origin` | release op | 15 |
| **40** | **Verification + Release Readiness (0.8.6)** — X1/X2/X3 + R-* AC gate + dry-run publish as final check | verification | 5,10,15,20 |

**Keystones / hard gates.** Slice 5 (provider protocol) gates 0.8.10's #6/#7. Slice 15 (release
machinery) gates Slice 20 and every later release's "shippable" DoD. **Slice 20 push is HITL-gated**
(178 commits; no tag without sign-off — `release-publish-gotchas`).

**Tracks (parallelizable).** Provider track **5** ∥ hygiene track **10** ∥ release track **15 → 20**;
all three off Slice 0. Three independent worktrees.

---

## 4. Reserved-gap policy (carried, unchanged)

Planned slices are multiples of 5; gaps `1–4, 6–9, 11–14, 16–19, 21–39` are the insertion mechanism for
unplanned follow-on a slice reveals. A reserved-gap slice (N+1…N+4) is **fully orchestrated** (own
prompt, own worktree off a fresh `main` baseline, own `output.json`, own codex §9, own CLOSED block) —
not an ad-hoc patch. **HALT rule:** if a gap band fills and follow-on remains, the slice was mis-scoped
→ HALT to HITL; never overflow into the next mod-5 number. (Full statement: `0.8.1-plan.md` §Numbering.)

---

## 5. Cross-cutting Definition of Done (X1/X2/X3 — bind EVERY slice)

- **X1 — SDK parity + functional harnesses.** Any surface/behavior/error/result change lands in **both**
  Py and TS SDKs in the same slice, each with a live functional harness (not symbol presence).
- **X2 — `mkdocs build` stays green.** Any `docs/` add/rename updates `mkdocs.yml` nav same-slice;
  Slice 40 enforces it as a release gate.
- **X3 — docs updated per slice + `dev/DOC-INDEX.md` maintained** in the closing docs commit.

`runs/STATUS-0.8.6.md` carries an X1/X2/X3 per-slice column.

---

## 6. Acceptance-criteria policy (carried)

`dev/acceptance.md` stays **locked** (`acceptance-md-locked-no-feature-acs`). Slices track work by
G-gap / OPP-id + TDD test names, NOT invented AC ids. New ACs (AC-078+) are minted **only at gated
slices** (Slice 0 conformance, Slice 40 release-readiness), decided HITL.

---

## 7. Prerequisites (before any slice opens)

1. **0.8.5 (CE-rerank α/pool_n exposure) closed** and its docs reconciled — this line baselines off the
   `main` that carries EXP-0.
2. **Worktree hygiene:** pre-create + verify each slice worktree off `$(git rev-parse main)`
   (`agent-worktree-stale-base-trap`); GPU/maturin builds happen on the MAIN tree only.

---

## 8. Out-of-band / parallel notes

- **0.8.7 (OOB) — GPU embedder (#3)** runs in parallel to this release; it is byte-stable/opt-in and
  shares no files with 0.8.6's provider/CI work.
- This entire release is **$0** and mechanism-only — it can run on a separate agent track concurrent
  with the experiment program without spending priced-run budget.

## 9. Immediate next slice

**Slice 0 — Setup + ADR Kickoff.** Stand up `runs/STATUS-0.8.6.md` (mirror STATUS-0.8.2 shape), draft
the provider-protocol ADR (the load-bearing one — it gates 0.8.10), and confirm the release policy
against `design/release.md`. Then fan out Slices 5 ∥ 10 ∥ 15.
