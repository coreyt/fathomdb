# FathomDB 0.8.10 — Plan (state-machine ladder) · **Memory-quality plumbing**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.10-implementation.md`; live state → `runs/STATUS-0.8.10.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.10` as an **orchestrator** session.
>
> **Theme.** Build the two memory-quality capabilities that sit at the head of the retrieval virtuous
> loop, now that the provider protocol (#8) and governed-verb boundary (#9) from 0.8.6 exist: lift ELPS
> extraction coverage (#6) and add the consolidation/recency provider (#7, the Mem0-parity update/
> temporal axis). Both are **CALLER-SIDE BYO-LLM** seams — no LLM enters the library query path.
>
> **Footprint.** The provider callbacks are caller-side BYO-LLM (OFFLINE-BUILD / caller's own model);
> the in-library write/index path stays CPU-only, deterministic. Tag every technique.

---

## 1. Goal & scope

- **#6 — ELPS extraction coverage (OPP-6).** Extraction coverage is ~1% on the consumer corpus, which
  starves every graph- and coverage-dependent measure and the D2 path. Lift coverage via the **one**
  generalized provider (0.8.6 #8), and verify the lift with a $0 LLM-free coverage probe before any
  priced extraction run. This enriches the inputs OPP-1/OPP-4/D2 consume "for free" and is the head of
  the real-gold partnership (OPP-9, 0.8.8 #10).
- **#7 — Consolidation/recency provider (OPP-2, AGREED).** An ELPS-shaped, prompt-and-regenerate
  consolidation/recency callback that merges/supersedes facts on the update axis (Mem0 parity), built
  **as one generalized provider per OPP-8** (not a new sibling contract). Gated on a **lossiness-vs-
  latency value test** — measure that consolidation buys accuracy worth its cost before shipping it on.

*Why this order in the line:* both hard-depend on 0.8.6 — #7 must ride the OPP-8 protocol (else a
throwaway contract), and HITL ruled the consumer migrates onto governed verbs (#9) **before** OPP-2/4
layer on. Coverage (#6) feeds the M5/M6 experiments, so coordinate with the M-work owner.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-COV-1 | $0 LLM-free coverage probe gates any priced extraction run | Probe reports per-class coverage on a fixed corpus; a failing probe blocks the priced run (records the negative) |
| R-COV-2 | Coverage lift is measured, pre-registered | Δcoverage vs the ~1% baseline on the frozen corpus, power-sized; reported with CI; no claim on an under-powered class |
| R-COV-3 | Extraction runs on the OPP-8 provider protocol | Re-expressed extractor uses the one protocol; no second transport (codex §9) |
| R-CON-1 | Consolidation/recency provider merges/supersedes facts via BYO-LLM callback | Functional harness: ingest conflicting/updated facts → consolidated result with correct supersession + temporal bounds |
| R-CON-2 | Lossiness-vs-latency value test passes before shipping-on | Pre-registered: accuracy gain ≥ tolerance at an acceptable latency/lossiness; a failing test ⇒ provider stays opt-off, negative recorded |
| R-CON-3 | Footprint honesty | Provider is caller-side BYO-LLM; library query path unchanged/CPU-only; tags present |
| R-X-1 | Py + TS SDK parity for both seams | X1 cross-binding harness green |

New ACs: candidates at Slice 0 (provider conformance) and the consolidation value-gate.

---

## 3. Slice ladder (mod-5)

```
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR — coverage-probe design + pre-registration; consolidation-provider ADR (OPP-2 on the OPP-8 protocol); the lossiness-vs-latency value-test design | design-adr | — |
| **5** | **Coverage probe (\$0)** — LLM-free per-class coverage measurement on the frozen corpus; the gate that precedes any priced extraction | implementation (measurement) | 0 |
| **10** | **ELPS coverage lift** — extractor on the OPP-8 protocol; priced run gated by Slice 5; measured Δcoverage | implementation (+priced, HITL-gated) | 5 |
| **15** | **Consolidation/recency provider** — BYO-LLM merge/supersede callback on the OPP-8 protocol | implementation | 0 |
| **20** | **Consolidation value-test** — lossiness-vs-latency pre-registered gate; ship-on only if it clears | implementation (eval) | 15 |
| **40** | **Verification + Release Readiness (0.8.10)** — X1/X2/X3 + R-COV/R-CON AC gate | verification | 5,10,15,20 |

**Keystones / hard gates.** **Slice 5 coverage-probe gates Slice 10's priced extraction** (cheap-
validate-before-spend). **Slice 20 value-test gates shipping consolidation on by default.** Any priced
run uses the **resilient harness** (auto-resume, atomic checkpoint, 429/5xx backoff, failure≠abstention,
completeness guard) and a $ ledger — `priced-runs-need-resilience-before-spend`.

**Tracks (parallelizable).** Coverage track **5 → 10** ∥ consolidation track **15 → 20**, off Slice 0.

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering).

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

X1 SDK parity + harnesses · X2 `mkdocs build` green · X3 docs + DOC-INDEX per slice. `runs/STATUS-0.8.10.md`
carries the per-slice X column.

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked; track by OPP-id + TDD names; new ACs only at gated slices, HITL-decided.

## 7. Prerequisites

1. **0.8.6 closed** — the generalized provider protocol (#8) and governed-verb boundary (#9) exist
   (hard deps for #7 and the #6 refactor).
2. **0.8.7 (OOB GPU) recommended-landed** — coverage/extraction re-embeds are far cheaper on GPU
   (soft, not blocking).
3. **Frozen corpus + gold** for the coverage probe and value-test (reproduced locally per slice;
   coordinate the gold with 0.8.8 #10 real-gold if available).
4. Worktrees off `$(git rev-parse main)`; priced runs HITL-gated with a resilient harness.

## 8. Out-of-band / parallel notes

- **Coordinate with the M-work owner:** ELPS coverage directly feeds the M5/M6 measures — align the
  coverage probe and frozen corpus with the active experiment so the lift is measured on the same basis.
- Priced extraction (Slice 10) is the only spend in this release and is HITL-gated; everything else is $0.

## 9. Immediate next slice

**Slice 0 — coverage-probe + consolidation-provider ADRs.** Pre-register the coverage probe and the
lossiness-vs-latency value test; confirm both seams ride the OPP-8 protocol; stand up
`runs/STATUS-0.8.10.md`. Then fan out Slices 5 ∥ 15.
