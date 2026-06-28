# ◆ GA sign-off — 0.8.0 GA / tag · track:GA · type:HITL-gate

## Purpose (1–2 sentences)

The final GA HITL gate: confirm an all-green per-AC scoreboard (incl. AC-075/076), the documented behavior-compat
events, and the real-embedder recall verdict on the **ruled** corpus basis → authorize **★ 0.8.0 GA / tag**. A
decision package the orchestrator assembles; **not** an agent to spawn.

## Prerequisites (verify ALL before starting — do not start if any is unmet)

- [ ] **◆ Slice-40 merged** (AC-075/076 on `main`). — verify: `git log --oneline main | grep -i "slice 40"` shows the
  merge **and** `git show main:dev/acceptance.md | grep -E "AC-075|AC-076"` resolves (or board §7 records the merge SHA).
- [ ] Slice-40 codex §9 PASS recorded. — verify: `ls dev/plans/runs/0.8.0-slice-40-review-*.md` with a PASS verdict.
- [ ] The recall floor on the ruled basis is GREEN (not breached). — verify: the Slice-40 `output.json` AC-075 bar
  is GREEN on the B-1-ruled corpus basis.
- (type:HITL-gate — orchestrator assembles; HITL decides + tags. Tagging is **HITL-only**.)

## Work to-do (the steps)

(Decision package the orchestrator assembles for HITL — not work steps.)

1. **All-green per-AC scoreboard** — every 0.8.0 AC incl. **AC-075** (recall floor) + **AC-076** (tiered AC-012
   latency) showing GREEN, sourced from the merged Slice-40 `output.json` + board §3.
2. **The 3 behavior-compat events** — documented (the intended observable behavior changes for 0.8.0).
3. **Recall verdict on the ruled basis** — eu7 real-embedder recall ≥0.90 on the B-1-pinned corpus (fidelity gate),
   stated separately from the IR/relevance axis ([[fathomdb-recall-fidelity-vs-relevance]]).
4. STOP → present the package + recommendation; **wait for HITL**. Do not tag.

## Output to the orchestrator (how this session reports back)

- Artifact(s): the GA sign-off decision recorded on the board (`STATUS-0.8.0.md` §7); on HITL approval, the
  **`v0.8.0` tag** (HITL-only — see [[release-publish-gotchas]]: a `v*` tag auto-fires the real publish; dry-run first).
- Schema/contract: package = {all-green scoreboard, 3 behavior-compat events, recall verdict on the ruled basis};
  the HITL ruling = GO/NO-GO + the tag authorization.
- Hand-off line: GA sign-off → **★ 0.8.0 GA / tag** (release). The IR-eval track continues post-GA (parallel).
- Discipline: `--release`+isolated for any measurement; read the REAL exit/numbers
  ([[background-exit-masks-real-exit]]); no fabricated numbers (TBD where unknown); **no push/tag without HITL**;
  board is orchestrator-owned.

## Full prompt / next

- Authoritative prompt: none — **HITL gate**; orchestrator prepares, HITL decides + tags.
  Context: `dev/plans/prompts/0.8.0-MASTER-ORCHESTRATOR-HANDOFF.md` §"Per-gate decision packages".
- On completion → **★ 0.8.0 GA**; IR-eval track (IR-B/C/D → IR-E → ◆ IR-gate) proceeds post-GA.
