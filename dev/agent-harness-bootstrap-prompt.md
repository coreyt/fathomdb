# Agent-harness bootstrap prompt — the FathomDB orchestration method, distilled

A **self-contained, portable** statement of how orchestrated work runs in this repo.
It *teaches the method*, not just points at it: a fresh agent (or a fresh repo) could
recreate the system from this file alone. It is the on-ramp; the binding mechanics live
in `dev/design/orchestration.md` (the runbook) and `AGENTS.md` (the invariants), and
this file maps every principle to those concrete artifacts.

Read this when: bootstrapping a new release, onboarding a new orchestrator, or porting
the method elsewhere. For day-to-day execution, the runbook is the source of truth.

---

## The one distinction that makes this work

- **The plan is the to-do list.** `dev/plans/plan-<release>.md` — scope, the mod-5
  slice ladder, requirements + acceptance, DoD. It answers *"will it get done?"* It is
  **per-release** and changes every release. Author it from
  `dev/plans/prompts/PLAN-TEMPLATE.md`.
- **The runbook is the method.** `dev/design/orchestration.md` — roles, state spine,
  preflight, the decision loop, recovery. It answers *"how do we run work?"* It is
  **release-independent** and stable across releases.

Everything that "must get done" becomes a **checkable plan item** that traces to a
requirement and is gated by a DoD — never prose hope. **Lesson learned about *how* to
run work → edit the runbook. New deliverable → edit the plan.** Confusing the two is the
most common drift: a plan bloated with method, or a runbook pinned to one release.

---

## The 7 core principles (each mapped to where it lives)

1. **Witness-derived state, never belief.** A slice's state is the furthest state whose
   on-disk witness exists and verifies; `STATUS-*.md` is a *cache* of that derivation.
   On any conflict, witnesses win. → `orchestration.md § 1.5` (witness table + 4
   invariants).
2. **Traceability: need → acceptance → pack → DoD.** Every requirement (`needs.md` →
   `requirements.md` → `acceptance.md`) has a falsifiable acceptance signal; every slice
   traces to a requirement and a DoD; every doc traces to an owning slice in
   `DOC-INDEX.md`. → `dev/traceability.md`, `dev/DOC-INDEX.md`, plan § 2/§ 3.
3. **DoD is an objective checklist.** Each item is independently verifiable (a test
   passes, a verdict file exists, a gate is answered) — nothing satisfiable by assertion
   alone. → plan § 2 (R-* acceptance) + § 5 (X1/X2/X3 cross-cutting).
4. **Orchestrator ≠ implementer.** The main thread orchestrates and never edits in a
   subagent's worktree; implementers write code in a main-thread-owned worktree and
   cannot spawn agents. → `AGENTS.md § 7`, `orchestration.md § 1`–§ 2.
5. **Independent-model review before merge.** A different model (codex, read-only
   sandbox) reviews the implementer's worktree; the main thread promotes the verdict.
   → `orchestration.md § 3`–§ 4.
6. **HITL gates, asked early.** Gates are named in the plan up front (kickoff + the
   release's own design questions), not discovered at sign-off. → plan § 10/§ 11,
   `orchestration.md § 12.7`.
7. **Surface, don't paper over.** Every silent behavior change is registered → a
   changelog line; forced deviations are logged loudly and escalated; "no silent change"
   is a claim you checked. → plan § 8/§ 9, SLICE-TEMPLATE § 6.

---

## The artifact set (role → this repo's file)

| Role in the method | FathomDB artifact |
| ------------------ | ----------------- |
| Invariants (front-loaded, cached, ≤300 lines) | `AGENTS.md` |
| Runbook / method (release-independent) | `dev/design/orchestration.md` |
| Preflight gate (pre-spawn) | `scripts/preflight.sh` (`orchestration.md § 1.6`) |
| Plan-with-DoD (per-release) | `dev/plans/plan-<release>.md` from `dev/plans/prompts/PLAN-TEMPLATE.md` |
| Pack/implementer prompt (per-slice) | `dev/plans/prompts/0.8.0-SLICE-TEMPLATE.md` |
| Live STATUS board (cache of witness state) | `dev/plans/runs/STATUS-<release>.md` (shape: `STATUS-0.8.2.md`) |
| Multi-pack / program handoff | `dev/plans/prompts/0.8.x-PROGRAM-STEWARD-HANDOFF.md` |
| Closure artifact (per-slice witness) | `dev/plans/runs/<slice>-output.json` (`orchestration.md § 8`) |
| Promoted review verdict | `dev/plans/runs/<slice>-review-<ts>.md` (`orchestration.md § 4`) |
| Traceability matrix | `dev/traceability.md`, `dev/DOC-INDEX.md` |
| Docs reconciliation + epoch marker | `dev/update-docs.md` |
| Failure & recovery catalog | `orchestration.md § 13` |
| Persistent memory (auto-loaded) | `MEMORY.md` + the `feedback_*`/`project_*` files it indexes |

---

## Copy-able skeletons

Two skeletons are small and load-bearing enough to inline (so a fresh repo can recreate
them). The larger fill-in prompts are full templates — copy those files, don't paraphrase.

**Witness state table** (the spine — derive position from disk, gate every transition):

| State | Witness (what proves it on disk) |
| ----- | -------------------------------- |
| `WORKTREE_CREATED` | `git worktree list` shows WT at the chosen baseline (+ `preflight.sh` passed) |
| `IMPLEMENTED` | `output.json` present **and** branch head advanced past baseline |
| `CHERRY_PICKED` | equivalent commit verified on mainline |
| `REVIEWED` | `<slice>-review-<ts>.md` with a `## Verdict:` line |
| `CLOSED` | plan has a `Slice/Phase <id> CLOSED` block + promoted verdict |
| `CLEANED` | `git worktree list` no longer shows the WT |

**Release DoD checklist** (the bar a release clears — every item independently checkable):

- [ ] All slices CLOSED with a promoted codex PASS (CONCERN fixed or overridden; no BLOCK override).
- [ ] Every R-* acceptance signal green (offline, on the target `main`).
- [ ] Full `./scripts/check.sh` green (incl. `AGENT_LONG=1` variants + `mkdocs --strict`).
- [ ] X1 cross-binding functional harnesses green in both SDKs.
- [ ] Override/duplication register reviewed; behavior-change register shipped as changelog lines.
- [ ] `dev/DOC-INDEX.md` is the accurate map of the shipped surface (X3); docs reconciled per `update-docs.md`.
- [ ] HITL kickoff + gate questions answered and recorded on the plan.
- [ ] End-to-end real-artifact smoke green (install the wheel; open/close/exit — not "green CI = done").
- [ ] Nothing pushed/tagged without explicit HITL approval; sign-off written.

The two big templates — **`PLAN-TEMPLATE.md`** (with the plan-authoring checklist) and
**`0.8.0-SLICE-TEMPLATE.md`** (with the anti-under-spec authoring checklist) — are
copied, not inlined; each carries its own fill-in guide.

---

## The kickoff HITL set (ask before code)

- **K1 — Working branch.** Land on local `main` per the SLICE-TEMPLATE merge policy, or
  a release branch? (Default: local `main`; push/PR/tag are HITL-only, orchestrator-run.)
- **K2 — Who runs the real-artifact smoke at sign-off** — orchestrator via an isolated
  instance, or the human? (`feedback_release_verification.md`: never "green CI = done".)
- **K3 — Release finalization** — version bump + changelog + LOCAL tag is the default;
  push/publish is a *separate* approval (`release-publish-gotchas`: a `v*` tag auto-fires
  the real publish).
- **K4 — The release's own design questions** — the plan's § 11 open questions.

Record answers on `STATUS-<release>.md` and the plan § 10.

---

## The operating loop

```
Kickoff HITL (K1–K4) → author plan from PLAN-TEMPLATE → independent design review (Slice 0 ADR)
→ per-slice loop:
    preflight (orchestration.md § 1.6: stale-base + dep-CLOSED + disk)
    → create main-thread-owned worktree off $(git rev-parse main)
    → author slice prompt from SLICE-TEMPLATE → spawn `implementer` (run_in_background)
    → gate on witness (output.json + commits past baseline; else FAILED → triage)
    → cherry-pick → codex review → promote verdict
        PASS → close · CONCERN(structural) → override+close · CONCERN(substantive)/BLOCK → fix-N
    → edit plan: add CLOSED block, advance "Immediate Next Slice" in the SAME commit
    → worktree cleanup after the phase family closes
→ release finalization: real-artifact smoke → version+changelog → local tag → docs reconciliation → DoD sign-off
```

Parallelize independent tracks (siloed reviewers); serialize anything sharing an editable
file. Compaction-safe: everything that must survive `/compact` lives on disk
(`orchestration.md § 12`).

---

## What to bootstrap, in order

1. **Invariants + runbook** — `AGENTS.md` (principles) + `dev/design/orchestration.md`
   (mechanics). Wire `scripts/preflight.sh`.
2. **Traceability** — `needs.md` → `requirements.md` → `acceptance.md`; stand up
   `DOC-INDEX.md`.
3. **Plan** — copy `PLAN-TEMPLATE.md` → `plan-<release>.md`; fill scope → requirements →
   ladder → registers → DoD; get an independent design review at Slice 0.
4. **STATUS board + program handoff** — `STATUS-<release>.md` (mirror `STATUS-0.8.2`) +
   the `0.8.x-PROGRAM-STEWARD-HANDOFF.md` entry point.
5. **Per-slice loop** — author each slice from `0.8.0-SLICE-TEMPLATE.md`; run the loop above.
6. **Docs reconciliation** — `dev/update-docs.md` with the epoch marker + the
   `mkdocs --strict` / version-consistency verify gate.

---

## What else matters (hard-won)

- **Make verification mechanical and gated** or it drifts: `./scripts/agent-verify.sh`
  (agent-loop gate) and `./scripts/check.sh` (broad CI gate incl. `mkdocs --strict`).
- **Preflight every spawn** — the stale-base trap cost two slices; `§ 1.6` is cheap insurance.
- **Independent review is load-bearing, not redundant** — codex § 9 has caught vacuously
  green tests a subagent audit passed (`conformance-rewrite-vacuous-green-trap`).
- **Keep the orchestrator's context lean** — delegate large reads; chat is throwaway;
  state lives on disk.
- **Register every silent behavior change** — it is a changelog line, not a footnote.
- **Halt to the human when there is no satisfiable next step** rather than improvising —
  a clean halt with state on disk is a good outcome.
- **The failure catalog is in the runbook** (`orchestration.md § 13`) — read it; these
  are real incidents, not hypotheticals.

> **Degraded (single-agent) mode.** The plan / DoD / witness / traceability discipline is
> usable by one agent with no fan-out: only the parallel-orchestration and
> independent-review pieces need the multi-agent capability. The method still holds —
> author the plan, derive state from witnesses, gate on the DoD, surface don't paper over.
