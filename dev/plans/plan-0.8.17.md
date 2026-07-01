# FathomDB 0.8.17 — Plan (state-machine ladder) · **Router hardening / forks**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.17-implementation.md` (authored at Slice 0); live state → `runs/STATUS-0.8.17.md`;
> deps/decision record → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` §4 (0.8.17 row) + §6 F-10. Run via
> `/goal complete 0.8.17` as an **orchestrator** session.
>
> **Theme.** Harden the in-library dispatcher shipped at 0.8.15 via three workstreams: (a) wire
> per-feature `(index, retrieval, α, pool_n, MMR, recency)` config tuples one-per-intent-class into the
> router, (b) productize the EXP-AF agent-feedback loop prototyped at 0.8.11, and (c) run the
> EXP-C/D/E GraphRAG-fork experiments as corpus lands. **OOB (odd) release** — all three workstreams
> are out-of-band relative to the even-line engine builds.
>
> **Footprint tag per workstream.**
> Config-tuple wiring: **IN-LIBRARY** (a compile-time registry over the shipped dispatcher).
> EXP-AF productization: **CALLER-SIDE-BYO-LLM** (`record_feedback` writes to a local JSONL sink,
> no egress; the agent's relevance signal is caller-supplied).
> EXP-C/D/E forks: **EVAL-ONLY / CALLER-SIDE-BYO-LLM** (C map-reduce stays eval-side; EXP-D is corpus
> acquisition; EXP-E is a HITL gate with no engine build at this release).
> Tag every technique IN-LIBRARY / CALLER-SIDE-BYO-LLM / OFFLINE-BUILD / EVAL-ONLY in each slice.

---

## 1. Goal & scope

### Workstream A — Per-feature config wiring (primary; no corpus dependency)

The 0.8.15 dispatcher classifies intent and selects a retrieval path, but the per-feature
`(index, retrieval, α, pool_n, MMR, recency)` config tuples are not yet permanently wired — the
EXP-B' joint-tuning results (landed by 0.8.11) and EXP-Fr-acc results (landed by 0.8.11) are the
data that pins them. 0.8.17 bakes those values into a **config-tuple registry**, enforces the
**forbidden-composition table**, and hardens the **override/hint API** to conformance grade.

The router selects the full tuple per feature, not merely an index. One function serves many
features with conflicting configs — `ce_rerank` wants a narrow `pool_n` for F1 needle-precision
but a wider pool for F2 recall-bound — so per-feature tuning is required rather than global
defaults (`initial-arch-planner-router-0.8.x.md` §5; `planner-router-psd-0.8.x.md` §II.B).

**Five intent classes (F1–F5) each get a canonical tuple wired at this release:**

- **F1 (needle / factoid memory):** fused-RRF + CE-rerank; α per EXP-B' tuned value (≈1.0 for
  Mem0-parity); pool_n narrow; MMR off; recency off.
- **F2 (multi_session):** wider candidate-gen; α/pool_n per EXP-B'; D2-as-router hint possible;
  MMR on.
- **F3 (temporal / knowledge-update):** F1-base + valid-time filter post-expansion (OD-4 order —
  expand → filter → rerank, not pre-filter); recency-judgment provider hook (OPP-2/0.8.12) if
  landed, else absent (safe fallback).
- **F4 (global sensemaking):** C (map-reduce QFS), LLM spend tier; CE-rerank **FORBIDDEN** on this
  path (F4-isolation constraint); cost tier surfaced per query.
- **F5 (multi-hop):** carries F1-base as a safe fallback until EXP-E resolves; documented as TBD
  pending Fork-E / HippoRAG-2.

**Forbidden-composition table** (enforced at the plan validator, not by convention):
C/map-reduce and community-summarization are valid **only** for F4/global and are **forbidden** on
F1/F2/F3/F5 paths. Reason: the blind-distiller penalty on needle paths is −0.362; routing a needle
query to C costs an LLM call and loses the exact fact
(`planner-router-psd-0.8.x.md` §II.B router-isolation constraint;
`0.8.x-parity-portfolio-strategy.md` §3 coupling note).

### Workstream B — EXP-AF productization (HITL-gated on 0.8.11 readout)

EXP-AF prototyped at 0.8.11 whether an agent relevance signal (via `record_feedback`) beats
`ce_score` alone, net of round-trip cost, on the EXP-OBS-enabled substrate. 0.8.17 executes the
**productization decision from the 0.8.11 readout**:

- **GO path** (signal beats `ce_score` net of cost): wire the VoI policy (ask-or-not threshold,
  the `ce_score`/route-margin at which an agent-ask earns its round-trip cost), productize as
  default-opt-in loop, execute any `record_feedback` governance migration that 0.8.11 triggered
  (per F-8b: if `record_feedback` was reclassified as a governed command at 0.8.11, this is where
  the conformance work lands — allowlist + Py/TS parity + X1 harness update).
- **NO-GO / KILL path** (signal does not beat `ce_score` net of cost): close the loop cleanly;
  confirm `record_feedback` stays off-by-default opt-in instrumentation; add a test asserting it is
  **not** wired into the default route-planning loop; no VoI policy built.

Either way, 0.8.17 closes the EXP-AF account with a registered verdict and a clean code state.

**KILL-path discipline (mirrors EXP-S's):** if the agent signal is net-negative, do not add it as
an "optional enhancement" — drop the loop entirely and keep the router on internal `ce_score`.
Opt-in overrides remain available to sophisticated callers.

### Workstream C — EXP-C/D/E forks (corpus-gated; reserved-gap slices, not blocking the release)

The three GraphRAG-fork experiments from `dev/design/0.8.4-closing-graphrag-gap.md`. The release
DoD closes on Workstreams A+B; the fork slices are **reserved-gap / conditional**.

- **EXP-C** (C productization as `query --global` mode, router-isolated from needle paths): the
  mode implementation can land on the existing AP-News BenchmarkQED corpus. The _formal
  registration_ (a powered `decide_084` verdict) requires ~269 entity-rich Q (EXP-D) — AP-News
  is capped at N=200 questions, comprehensiveness MDE 0.058 > ε, more runs do not help (bootstrap
  is question-clustered). EXP-C productization and EXP-D corpus acquisition are independent; ship
  C as a usable mode, gate the formal verdict.
- **EXP-D** (entity-rich AutoQ-style ~269 Q set — the only new corpus-acquisition item for F4):
  requires a HITL spend decision before execution. Proceeds if approved.
- **EXP-E** (Fork E — entity/Leiden graph): triple-gated — D1/RAPTOR must be built (NOT scoped in
  any current release), an entity-rich corpus must show a relationship gap C/D2 cannot close, and
  HITL must approve. At 0.8.17 this is a **decision gate only**; no implementation.

**Out of scope (explicitly):** EXP-FT free-threaded-Python ladder (→ 0.8.19); #13 benchmark-
harness (→ 0.8.19); even-line items #5/#11-full (→ 0.8.18); #15 F9 ranking (→ 0.8.16); any
engine schema migration.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
| --- | --- | --- |
| R-CFG-1 | Per-feature config registry: one canonical `(index, retrieval, α, pool_n, MMR, recency)` tuple per intent class F1–F5, carrying EXP-B' tuned values | Registry round-trip tests: each feature class → dispatcher → config reads back the canonical tuple; F5 carries the documented F1-base fallback |
| R-CFG-2 | Forbidden-composition enforced: C/map-reduce and community-summarization rejected for F1/F2/F3/F5 paths at the plan validator | Plan-validator rejects a C-on-needle plan; TDD RED (validator accepts) before wiring, GREEN after; codex §9 |
| R-CFG-3 | Override/hint API conformance-grade: agent `mode=needle\|global\|…` hint or explicit tuple exercised at the surface level | X1 Py/TS harness exercises hint round-trip and confirms the routed config matches the hint; `mkdocs build` green |
| R-CFG-4 | Cost tier surfaced per feature: each routed plan emits its spend tier (CPU / GPU / local-LLM / net-LLM) alongside the selected tuple | Smoke test: F4 plan emits `net-LLM`; F1 plan emits `CPU`; F3 emits `CPU` (filter is lossless/local) |
| R-AF-1 | EXP-AF productization decision executed in code, not merely documented | GO: VoI policy wired; `record_feedback` governance state matches 0.8.11 reclassification verdict. NO-GO: `record_feedback` confirmed off-by-default; test asserts it is NOT in the default route loop |
| R-AF-2 | EXP-AF verdict registered: VoI lift vs `ce_score`, round-trip cost, depth-bound decision documented in a result artifact and `dev/experiments-ledger.md` | Entry exists and is cited in `runs/STATUS-0.8.17.md` |
| R-FORK-C | EXP-C productized as `query --global` mode, router-isolated | C mode callable on AP-News; rejected by plan validator on a needle intent plan; per-query cost surfaced |
| R-FORK-D | EXP-D corpus-acquisition status documented: either corpus acquired + formal `decide_084` re-run, OR blocked + reason recorded | `runs/STATUS-0.8.17.md` §fork-D entry with verdict or carry-forward record |
| R-FORK-E | EXP-E HITL gate documented: evidence assessed, HITL decision recorded | `runs/STATUS-0.8.17.md` §fork-E entry; no implementation without explicit HITL GO |
| R-X-1 | Py/TS SDK parity for config-tuple reads and hint API | X1 cross-binding harness green for new surfaces |

**New ACs (candidates only — minted at Slice 0, HITL-decided):** R-CFG-1–4 are candidates for one
or more formal AC ids (config-tuple conformance, forbidden-composition gate). R-AF-1/2 are
candidates if EXP-AF GO path lands a governed-command surface change. No invented AC ids; track by
EXP-tag + TDD test names until Slice 0 HITL decision.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → [25] → [30] → [35] → 40
```

Three tracks fan out from Slice 0. Tracks A (config wiring) and B (EXP-AF productization) are the
**release backbone**. Track C (forks) is **reserved-gap / corpus-conditional** and does not gate
Slice 40.

| Slice | Title | Track | Work-type | Depends-on |
| ---: | --- | --- | --- | --- |
| **0** | **Setup + ADRs** — frozen requirements; config-tuple registry schema; forbidden-composition table; EXP-AF productization scope (go/no-go checkpoint against 0.8.11 readout + F-8b `record_feedback` state); EXP-C scope; stand up `runs/STATUS-0.8.17.md` | A+B+C | design-adr | 0.8.15 done; EXP-B' values on record; 0.8.11 EXP-AF readout available |
| **5** | **Config-tuple registry (Track A)** — implement one canonical tuple per F1–F5 wired into the dispatcher; EXP-B' tuned values baked in; TDD: round-trip RED → GREEN; F5 carries the F1-base fallback with a documented `[TBD: post-EXP-E]` annotation | A | implementation | 0 |
| **10** | **Forbidden-composition validator + cost-tier surface (Track A)** — plan validator rejects C-on-needle; each routed plan emits its spend tier; cross-feature regression guard (EXP-B'.5: a config change for Fx must not regress Fy on the registered decision rules) | A | implementation | 5 |
| **15** | **Override/hint API hardening + X1 parity (Track A)** — agent `mode=…` or explicit tuple exercised at conformance level; Py/TS parity for config-tuple reads and hint round-trip; codex §9 | A | implementation | 10 |
| **20** | **EXP-AF productization (Track B, HITL-gated at Slice 0)** — execute the 0.8.11 go/no-go: GO path = wire VoI policy + `record_feedback` governance migration per F-8b checklist; NO-GO path = confirm opt-in-only + test asserting not-in-default-loop; register EXP-AF verdict in `dev/experiments-ledger.md` | B | implementation + eval | 0 (parallel to Track A) |
| **[25]** | **EXP-C productization (Track C, reserved-gap — PROCEED on current corpus)** — `query --global` mode on existing AP-News; C router-isolated via the Slice 10 validator; per-query cost surfaced; registered as CALLER-SIDE-BYO-LLM mode (not an engine change) | C | reserved-gap | 0; AP-News on disk (confirmed) |
| **[30]** | **EXP-D corpus acquisition + decide_084 re-run (Track C, corpus-gated HITL)** — acquire ~269 entity-rich Q set (AutoQ-style, the only new F4 acquisition item); HITL spend decision required before execution; resilient harness required for the priced judge run | C | reserved-gap / corpus-gated | [25]; HITL spend approval at slice start |
| **[35]** | **EXP-E Fork-E decision gate (Track C, HITL-only)** — assess whether EXP-D corpus reveals a relationship gap C/D2 cannot close; HITL decision recorded; **no engine build at this slice** (D1/RAPTOR is a prerequisite that is not scoped in any current release) | C | reserved-gap / HITL gate | [30]; HITL required; D1 unbuilt = EXP-E cannot build |
| **40** | **Verification + Release Readiness (0.8.17)** — X1/X2/X3; R-CFG-1–4, R-AF-1/2, R-FORK-C gates green; R-FORK-D/E documented; master sequencing doc §4+§6 reconciled; dry-run publish via 0.8.18 release machinery | A+B+C | verification | 5,10,15,20 (backbone); [25] if complete |

### Keystones / hard gates

- **Slice 5 gates Slices 10 and 15** — the config-tuple registry must exist before the validator
  and override API can be built against it.
- **Slice 0 EXP-AF go/no-go checkpoint** — if the 0.8.11 EXP-AF readout is unavailable or
  ambiguous, escalate to HITL before setting Slice 20 scope. Slice 20 executes a known verdict;
  it does not re-run the experiment.
- **[Slice 25] PROCEED on current corpus** — AP-News BenchmarkQED is on disk (corpus-survey Cycle
  1, 2026-06-28 confirmed). EXP-C productization has no additional corpus prerequisite. It is
  reserved-gap only so it does not block Track A.
- **[Slice 30] BLOCKED until HITL approves EXP-D spend** — the ~269 Q acquisition is a new priced
  item; not pre-authorized. Do not begin until HITL approval.
- **[Slice 35] BLOCKED on two fronts** — EXP-D corpus (Slice 30) AND D1/RAPTOR (not scoped).
  Slice 35 is a gate document, not a build.
- **Slice 40 gates the 0.8.17 release DoD on Slices 5/10/15/20** (backbone). Reserved-gap slices
  [25]/[30]/[35] do NOT gate Slice 40 — they close "as corpus lands" or carry forward to 0.8.19.

### Tracks (parallelizable)

Track A (5 → 10 → 15) ∥ Track B (20), both fanning out from Slice 0.
Track C ([25] → [30] → [35]) is independent of A and B after Slice 0, and is corpus-gated.

---

## 4. Reserved-gap policy

Carried unchanged (`dev/plans/plan-0.8.1.md` §Numbering). Reserved gaps [25], [30], [35] are
named, gated, and explicitly open in the ladder. They close when their corpus/HITL prerequisite is
satisfied. They do NOT gate Slice 40 unless explicitly promoted by HITL decision at Slice 0.

Any slice still OPEN at Slice 40 closes as a "documented deferral" in `runs/STATUS-0.8.17.md` and
carries its status to 0.8.19 (see §8 corpus-carry-forward note). A documented deferral is not a
failure — the self-completion principle (F-10) applies to the backbone tracks A and B, not to the
reserved-gap fork slices.

---

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

**X1** SDK parity + harnesses — Py/TS surfaces for config-tuple reads and hint API; `record_feedback`
conformance state consistent with the 0.8.11 governance decision.

**X2** `mkdocs build` green — no new doc regressions; the markdown-lint gate was addressed at
0.8.9.1; every new doc emitted by 0.8.17 slices must be lint-compliant.

**X3** Docs + DOC-INDEX per slice — each slice that adds a new surface updates `dev/DOC-INDEX.md`.

`runs/STATUS-0.8.17.md` carries the per-slice X column. For eval-only slices ([25]/[30]/[35]) the
"shippable" DoD is a **landed result doc + reproducible script**, not a published artifact, per the
orchestrator contract (master §8).

---

## 6. AC policy

`dev/acceptance.md` locked at AC-073 (`acceptance-md-locked-no-feature-acs`). Track requirements
by R-id + EXP-tag + TDD test names. New ACs are **candidates only at Slice 0**, minted if HITL
decides. Specifically:

- R-CFG-1–4 are candidates for one or more new ACs (config-tuple registry + forbidden-composition
  validator). Steward decides at Slice 0 whether these clear the bar for formal AC inclusion.
- R-AF-1 is a candidate for a new AC if the EXP-AF GO path lands a new governed-command surface
  (i.e., if `record_feedback` was reclassified at 0.8.11 and the conformance-test count changes).
- R-FORK-C/D/E are eval-only; they do not mint ACs.
- If `record_feedback` was reclassified as a governed command at 0.8.11, its X1 conformance count
  change was recorded then. 0.8.17 does not re-mint that AC — it executes the conformance work the
  0.8.11 decision required and verifies the count is correct.

---

## 7. Prerequisites

1. **0.8.15 dispatcher closed (hard gate)** — the in-library EXP-Fr dispatcher exists over the
   EXP-S kind-tagged substrate (0.8.14), and the router locus decision is resolved. 0.8.17 hardens
   what 0.8.15 built; it cannot precede it. Verify via `dev/plans/runs/STATUS-0.8.15.md`.

2. **0.8.11 EXP-AF prototype and readout available (hard gate for Track B)** — the EXP-AF value
   test result — agent signal vs `ce_score`, round-trip cost, VoI break-even — must be on record
   before Slice 20 scope is set. Slice 20 executes a known result, not a new experiment. If the
   0.8.11 readout is incomplete, Slice 0 escalates to HITL.

3. **EXP-B' joint-tuning values confirmed (hard gate for Slice 5)** — the `(α, pool_n, MMR,
   recency)` values per intent class must be on record in `dev/experiments-ledger.md` (EXP-B'
   entry). These are the values Slice 5 bakes into the registry. If EXP-B' is missing a class,
   Slice 0 flags the gap and documents the fallback for that class.

4. **EXP-Fr-acc results on record** — the classifier accuracy, asymmetric mis-route cost matrix,
   and locus decision from 0.8.9–0.8.11 are inputs to the forbidden-composition table and
   cost-tier definitions. Verify presence in `dev/experiments-ledger.md`.

5. **`record_feedback` governance state known** — the F-8b mandatory reclassification review was
   due at 0.8.11. Verify the outcome in
   `dev/plans/runs/NOTE-0.8.8-to-orchestrator-record-feedback-reclassify.md` and the 0.8.11
   status doc. If unresolved, escalate at Slice 0.

6. **OPP-2 consolidation/recency provider (0.8.12)** — if landed, the F3 config tuple wires its
   recency-judgment hook. If not landed, F3 ships without it (the tuple is partial, not wrong —
   valid-time filtering still applies). Verify via `dev/plans/runs/STATUS-0.8.12.md`.

7. **AP-News BenchmarkQED on disk** — confirmed by corpus-survey Cycle 1 (2026-06-28) at
   `data/corpus-data/raw/apnews_benchmarkqed/`. EXP-C productization ([Slice 25]) PROCEEDS on
   this corpus. Licensing: Microsoft Research License, NON-COMMERCIAL, EVAL-ONLY, gitignored —
   never commit, never ship.

8. **HITL spend approval for EXP-D** — the ~269 entity-rich Q acquisition is a new priced item,
   not pre-authorized. Slice [30] cannot begin without it. Not a Slice 0 pre-condition; surfaced
   as a Slice [30] gate.

9. **D1 (RAPTOR recursive tree) unbuilt — consequence for EXP-E** — D1 is not scoped in any
   current release. Slice [35] is therefore a **HITL decision gate only** at 0.8.17; no
   EXP-E implementation is possible without D1 landing first.

10. **Worktrees and resilience** — worktrees off `$(git rev-parse main)` (not a stale base — see
    `agent-worktree-stale-base-trap`). Any priced run at [Slice 30] requires a resilient harness:
    incremental checkpoint, verified `--resume`, 429/5xx backoff, completeness guard, and a $ ledger
    (`priced-runs-need-resilience-before-spend`).

---

## 8. Dependencies / sequencing

### Hard edges (blocking)

| Edge | From | To | Class |
| --- | --- | --- | --- |
| 0.8.15 dispatcher | 0.8.15 (EXP-Fr; EXP-S substrate @0.8.14) | All tracks | **Physically hard** — no dispatcher to harden |
| EXP-B' joint-tuning values | 0.8.9–0.8.11 | Track A, Slice 5 | Physically hard — Slice 5 bakes the tuned values |
| EXP-AF 0.8.11 readout | 0.8.11 | Track B, Slice 20 | Physically hard — Slice 20 executes a known verdict |
| AP-News BenchmarkQED | On disk (confirmed) | Track C, Slice 25 | Satisfied |
| HITL spend approval | HITL | Track C, Slice 30 | Administrative pre-condition |
| D1 / RAPTOR | NOT YET SCOPED | Track C, Slice 35 (EXP-E build) | Physically hard — Slice 35 is gate-only at this release |

### Corpus-gating (LOUD FLAG)

> **EXP-C registration is corpus-capped at N=200 (AP-News max).** The `decide_084`
> comprehensiveness MDE is 0.058 > ε at N=200; because the bootstrap is question-clustered, more
> runs do not tighten it — only more questions can (`0.8.4-COMPREHENSIVE-REPORT.md` banner;
> `planner-router-psd-0.8.x.md` §III.A "corpus cap"). A powered formal `decide_084` verdict
> requires ~269 entity-rich Q (EXP-D), which is NOT yet on disk. **Do NOT state C's verdict as
> "registered" at this release** unless [Slice 30] completes with an EXP-D corpus and a cleared
> decide_084 run.

**Corpus availability summary for 0.8.17 forks:**

| Corpus | Status | Needed for | Blocker |
| --- | --- | --- | --- |
| AP-News BenchmarkQED (1,397 articles) | ON DISK | EXP-C productization (Slice 25) | None — PROCEED |
| ~269 entity-rich Q set | NOT ON DISK | EXP-C formal registration; EXP-D | HITL spend approval |
| HotpotQA / 2WikiMultihopQA / MultiHop-RAG | NOT ON DISK | EXP-E (F5 multi-hop, if Fork E opens) | D1 unbuilt; Fork E triple-gated |
| MuSiQue (4,834 rows) | ON DISK | F5 fallback evaluation only | None |

**Carry-forward rule:** if [Slice 30] and/or [Slice 35] are still OPEN at Slice 40, they roll to
**0.8.19** as carry-forward items. The 0.8.19 release (EXP-FT + benchmark harness) is the natural
home for any fork overflow, since it is the end-of-0.8.x slot. Record the carry-forward in both
`runs/STATUS-0.8.17.md` and the master §4 0.8.19 row.

### Position in the overall sequence

- **After 0.8.18** — the #5 vector-equivalence + #11-full publish capstone must close first; the
  full publish pipeline then exists for 0.8.17's Slice 40 dry-run verify.
- **Before 0.8.19** — EXP-FT free-threaded-Python ladder (FT-1…5) and #13 benchmark harness are
  held for 0.8.19; 0.8.17 does not touch them. The EXP-S-KILL contingency (agent-side router as
  fallback) was resolved at 0.8.14/0.8.15; 0.8.17 does not re-litigate locus.
- **Odd line owns OOB / eval.** 0.8.17 is the correct slot for EXP-AF productization and the fork
  experiments; no even-release engine work belongs here (don't import even-line schema migrations
  into an OOB odd release).

### HITL decision points

| DP | Decision | When | Owner |
| --- | --- | --- | --- |
| **DP-0.17-A** | EXP-AF go/no-go: is the 0.8.11 readout conclusive? What is the verdict? | Slice 0 | Steward |
| **DP-0.17-B** | `record_feedback` governance state entering 0.8.17: instrumentation or governed command? (executes the 0.8.11 F-8b decision) | Slice 0 | Steward |
| **DP-0.17-C** | EXP-D spend approval: authorize acquisition of ~269 entity-rich Q set | Slice [30] start | HITL |
| **DP-0.17-D** | Fork E evidence gate: does EXP-D corpus reveal a relationship gap C/D2 cannot close? | Slice [35] | HITL |

**DP-0.17-A and DP-0.17-B must be resolved before Track B (Slice 20) proceeds.** All other DPs
are non-blocking to the backbone.

---

## 9. Immediate next slice

**Slice 0 — ADRs + frozen requirements.** Do the following in order before authoring any design doc:

1. Verify `git rev-parse HEAD` on `origin/main` — do NOT use the working tree (see
   `shared-checkout-branch-can-be-stale-vs-session-env`).
2. Confirm 0.8.15 dispatcher is **CLOSED** — check `dev/plans/runs/STATUS-0.8.15.md`. If OPEN,
   stop; 0.8.17 cannot begin.
3. Pull EXP-B' tuned `(α, pool_n)` values per feature class from `dev/experiments-ledger.md`.
   If any intent class has no registered value, document the fallback for that class in the Slice 0
   ADR (do not guess; use the conservative default and flag for a future EXP-B' extension).
4. Pull EXP-Fr-acc results (asymmetric mis-route cost matrix, forbidden compositions) from
   `dev/experiments-ledger.md`. These drive the forbidden-composition table.
5. Pull the EXP-AF 0.8.11 readout — is the result conclusive? If EXP-AF is not yet complete (0.8.11
   open), **ESCALATE TO HITL before setting Slice 20 scope** (DP-0.17-A).
6. Pull the `record_feedback` governance decision from
   `dev/plans/runs/NOTE-0.8.8-to-orchestrator-record-feedback-reclassify.md` and the 0.8.11 status
   doc. Determine: is `record_feedback` now a governed command or still instrumentation?
   (DP-0.17-B.)
7. Draft the config-tuple registry schema — one entry per F1–F5 with columns for `(index,
   retrieval, α, pool_n, MMR, recency, spend_tier)`. F5 entry documents F1-base fallback with
   `[TBD: post-EXP-E]` annotation.
8. Draft the forbidden-composition table: which ops are F4-only, which compose correctly, which are
   forbidden.
9. Set EXP-AF Slice 20 scope (GO or NO-GO path) based on steps 5–6.
10. Stand up `runs/STATUS-0.8.17.md` with all nine slices pre-populated as OPEN.
11. Fan out Track A (Slice 5) ∥ Track B (Slice 20) off Slice 0.
12. Set reserved-gap slices [25]/[30]/[35] status: [25] = PROCEED-WHEN-READY (corpus on disk);
    [30] = BLOCKED (HITL spend); [35] = BLOCKED (D1 + [30]).

**Escalate to HITL immediately if:** 0.8.15 is not closed; EXP-AF readout is absent; `record_feedback`
reclassification is unresolved from 0.8.11; EXP-B' values are missing for more than one intent
class.
