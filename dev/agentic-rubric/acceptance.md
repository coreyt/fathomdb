# Acceptance — operational agent-audit harness

Falsifiable signal per requirement in `requirements.md` (matched by ID), mapped to the slice
that satisfies it. A requirement is met only when its signal is demonstrated by a test or a
recorded witness — never by assertion (rubric protocol rule 2, applied to this tool itself).

| ID | Slice | Acceptance signal (falsifiable) |
| --- | --- | --- |
| OR-AC-1 | 5 | Ingesting the same fixture inputs twice yields byte-identical `AuditSubject` hashes; a mutated input yields a different hash. |
| OR-AC-2 | 5 | A `[D]` criterion (e.g. E1/E2 ledger integrity) scores MET/UNMET with no network call; the class dispatcher routes `[L]`/`[H]` criteria to the judge seam, verified by a call-count assertion. |
| OR-AC-4a | 5 | Re-assembling the 0.8.19 scorecard from repo artifacts under `FakeJudge` reproduces the recorded per-dimension %MET (A 60.9 · B 91.3 · C 100 · D 95.2 · E 75.0 · F 86.7 · G 73.3 · H 86.7) and the HARD-gate PASS. |
| OR-AC-3 | 10 | Configuring the judge family equal to the audited family raises a self-preference error before any call; a `[L]` verdict with no citation is rejected as UNMET-not-scorable; an empty/garbage completion is marked ABSENT and excluded from the denominator. |
| OR-AC-4b | 10 | A single injected HARD UNMET fails the subject regardless of a high weighted mean; weights come from `audit/severity_vector_v3.json` (swapping the file changes the aggregate). |
| OR-AC-5 | 10 | A projected cost above `--max-usd` refuses before any network call; a killed run resumes and re-judges only ABSENT/missing `custom_id`s; no code path passes a raw transcript body to the judge (asserted). |
| OR-AC-6 | 15 | A seeded report produces a decision package whose proposals each carry `target_path`, `criteria_addressed`, `finding_refs`, a patch, `risk_tier`, `acceptance_signal`, and `rollback`; a proposal that targets the rubric doc is rejected (subject-only). |
| OR-AC-7 | 20 | Apply with no approval manifest is refused; with one, patches land in a fresh worktree, `agent-verify` runs, and a verify failure aborts the apply leaving the target checkout untouched. |
| OR-AC-8 | 25 | A milestone re-audit over a held-out set emits per-dimension/per-criterion deltas with bootstrap CIs; the seed is a parameter (deterministic across two runs). |
| OR-AC-9 | 20 | Each run/proposal/apply/milestone appends one well-formed `ledgerwrite` record with a monotonic `seq` and a `decider`; `ledgerwatch --validate` passes; a delta read returns only new entries. |
| OR-AC-10 | 25 | Judge↔human κ on the existing packs is computed per rubric version via `audit/compute_irr.py` and recorded to the ledger; a rubric version that lowers κ is flagged even if its mean %MET is higher. |

## Definition of done (per slice)

A slice is done when its acceptance rows pass as tests, `./scripts/agent-verify.sh` is green,
and — for Slice 5 — the dry-run reproduces the 0.8.19 scorecard (`OR-AC-4a`) from repo artifacts
under `FakeJudge`. Slice 10 additionally requires a small priced airlock pilot (with `--max-usd`)
whose automated scorecard is compared against the hand-run ground truth.
