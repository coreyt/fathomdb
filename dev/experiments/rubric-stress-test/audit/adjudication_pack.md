# Adjudication pack (v3 revision loop)

27 candidates. Each carries a +/-3 line window (>> marks the hit line).

## [1] DQ-LIMITED-SAMPLE::fathomdb/a6f7b8e5-3e3c-45cb-b551-77108e9bb892
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
     769 assi Launched (`bog16q10j`). Let me confirm clean startup, then do an early empty-rate spot-check once the first checkpoint lands (to verify the run itself is clean, not just the gate).
     770 assi 
     771 tres [RERANK-ACC][CLI] REBLEND arm: alpha=1.0 pool_n=10 [RERANK-ACC][CLI] 606 queries, 940 sessions | corpus_hash=1859817a32bc reused_cells=1212 [RERANK-ACC][CLI] REBLEND arm: alpha=1.0 pool_n=10 3827905 .venv/bin/python -m e
>>   772 assi Started clean (REBLEND α=1.0/pool_n=10, 606 queries, reused cells loaded). Process running. **Status — interactive α=1.0 arm is live (`bog16q10j`)** after a thorough de-risking pass: - **Diagnosis settled** (your framewo
     773 atta 
     774 syst 
     775 syst 
```

## [2] DQ-LIMITED-SAMPLE::fathomdb/b9422d49-4abd-4b4d-8145-f72f13b6974f
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
     100 assi 
     101 assi The script lints the whole tree (node_modules noise is pre-existing). Let me check only my file:
     102 assi 
>>   103 tres exit: 1 (1 = no matches = clean) exit: 1 (1 = no matches = clean)
     104 last 
     105 ai-t 
     106 mode 
```

## [3] DQ-LIMITED-SAMPLE::fathomdb/ea8ebf5d-7e29-4981-9565-4d4323076723
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
      48 assi 
      49 assi Publish verified (PyPI 0.8.9). Let me read the master's release allocation/next-up section.
      50 assi 
>>    51 tres 1:# FathomDB 0.8.6 → 0.8.16 — master release plan (sequencing & dependencies) 11:> This file is the **dependency graph + allocation + decision record**; each release's slice ladder lives 118:| **I-2** | 0.8.12 **EXP-S** 
      52 atta 
      53 assi 
      54 assi I've verified the live state against the handoff and the master schedule. Here's where things actually stand. ## Status (verified from git/registries, 2026-06-29) - **main = `fdc49769`**, CI green. The handoff snapshot (
```

## [4] DQ-LIMITED-SAMPLE::fathomdb/fed84f3a-fa50-4d04-a316-d6b1dbb51601
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
      27 mode 
      28 perm 
      29 assi 
>>    30 assi The clean Fable evaluation is in. Before I relay the full critique, let me put the specific decision question to the same agent so you get its direct position.
      31 assi 
      32 tres 
      33 assi 
```

## [5] DQ-LIMITED-SAMPLE::memex/78be23bc-2fcd-4e6d-979a-766567a0d48f
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
     117 mode 
     118 perm 
     119 assi 
>>   120 assi All four reviewers are in. Here's the consolidated enumeration — deduplicated across the reviewers and my own checks, grouped by area, ranked within each. **No changes were made; this is review-only.** --- ## A. Persiste
     121 atta 
     122 syst 
     123 syst 
```

## [6] DQ-LIMITED-SAMPLE::memex/87be1df7-b4c2-4114-b890-41911c76836c
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
      32 HITL <task-notification> <task-id>a2db4ff6ab8719440</task-id> <tool-use-id>toolu_01DDtfVZ6RKdrBJKk2TFnZB1</tool-use-id> <output-file>/tmp/claude-1000/-home-coreyt-projects-memex/87be1df7-b4c2-4114-b890-41911c76836c/tasks/a2db
      33 last 
      34 cust 
>>    35 agen 
      36 mode 
      37 perm 
      38 assi I have the implementation picture. Waiting on the 0.5.5 research-document agent.
```

## [7] DQ-LIMITED-SAMPLE::memex/a2383ecd-9194-4eca-a2bf-a14cb5c4fcc6
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
      32 mode 
      33 perm 
      34 brid 
>>    35 tres 1 # STATUS — ELPS Hybrid-Extraction implementation board 2 3 _The live state spine for the **hybrid-extraction / non-LLM-temporal** initiative (the 4 follow-on to GA ELPS). The **orchestrator** maintains this file — one 
      36 last 
      37 ai-t 
      38 mode 
```

## [8] DQ-LIMITED-SAMPLE::memex/b0c66cc3-096f-428b-93c9-c57195311b86
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
      26 assi 
      27 assi I'll start by reading the steward kickoff prompt in full, then do its required reading before proposing anything.
      28 assi 
>>    29 tres 1 # PAS steward kickoff prompt (HITL → start a steward session) 2 3 > When coreyt wants a program-steward pass, run the `/steward` custom command 4 > (optionally naming the focus, e.g. `/steward reconcile the boards vs g
      30 last 
      31 cust 
      32 agen 
```

## [9] DQ-LIMITED-SAMPLE::memex/cb249b95-e5dc-4b70-bfa5-eec25310ef6f
- detector: `DQ-LIMITED-SAMPLE`  mapped_criteria: ['C6', 'B7']  class: limited-sample-generalization

```
      25 file 
      26 HITL will these recommendations work for situations where there are document changes intra-document (e.g. if there are multiple ledgers in a single file - a ledger-per-component and the file itself tracks multiple components.
      27 assi 
>>    28 assi Right — this breaks the tail-cursor approach, and it's worth being precise about *why*: a byte/line offset is a **positional** cursor, and positional cursors only work when writes are append-only at EOF. The moment a wri
      29 last 
      30 ai-t 
      31 mode 
```

## [10] DQ-SHORTKNOWLEDGE::fathomdb/0a6e63f1-c44b-4702-b04f-c956a7ffcd34
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
    1715 assi The annotated decision record is written into the consult — all of D1–D5 + Q1–Q9 answered (Accept / Accept-with-change / Reject), Q3 + Q4 signed off, and the exact `instructions` + byte-offset field specs pinned in a "Pi
    1716 assi 
    1717 tres === brief: line anchors for the 5 pins === 9:## Background (why this exists) 25:## What to build 37:## Transport (pick the primary; the protocol is transport-agnostic) 49:## Protocol v1 — messages 70: "options":{"determi
>>  1718 assi I have the anchors. Applying the five ratified pins to the FathomDB brief — `instructions` field (Q1), byte-offset `source_span` (Q4), replay-determinism (Q3), `warnings.kind` enum (D5), per-doc timeout (Q7):
    1719 assi 
    1720 tres The file /home/coreyt/projects/fathomdb/dev/plans/prompts/IR-C-byo-llm-extraction-harness-memex.md has been updated successfully. (file state is current in your context — no need to Read it back)
    1721 assi 
```

## [11] DQ-SHORTKNOWLEDGE::fathomdb/1ca602a1-a2f1-4e36-bfc8-17ee640761da
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
     264 HITL perform (a) but consider the additional measurement work (and needed prerequisite work) as HITL-approved.
     265 atta 
     266 assi 
>>   267 assi Approved — applying (a) to both files, with the measurement work (D0 unblock + embedder probe + D2 harness-proxy + latency re-check) folded in as **HITL-approved** rather than pending-gate. Engine promotions (CLS fix, fi
     268 assi 
     269 file 
     270 last 
```

## [12] DQ-SHORTKNOWLEDGE::fathomdb/2d34f00b-44ad-4991-9e29-f4f059ed3f52
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
    1528 assi 
    1529 tres The file /home/coreyt/projects/fathomdb/dev/plans/runs/STATUS-0.8.11.md has been updated successfully. (file state is current in your context — no need to Read it back)
    1530 assi 
>>  1531 assi Committing the Slice 45 sign-off and closing the task.
    1532 assi 
    1533 tres committed === final 0.8.11 commit graph (recent) === ad5baf18 docs(0.8.11 Slice 45): release-readiness sign-off — substantive gates GREEN 2893e81f fix(0.8.11 Slice 45): pyright-clean the eval run-scripts (behavior-preser
    1534 last 
```

## [13] DQ-SHORTKNOWLEDGE::fathomdb/3020228d-f490-4df0-aac1-ae3f134936dd
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
     346 assi codex pass-3 still running. Let me clean worktrees meanwhile (they're in /tmp, independent of the main-repo review):
     347 assi 
     348 tres removed /tmp/fdb-cebatch-1724fdb5 removed /tmp/fdb-cererank-4df06a4a removed /tmp/fdb-gapdecomp-b8c412c6 removed /tmp/fdb-rerankacc-1724fdb5 removed /tmp/fdb-slice-10-20260621T221212Z removed /tmp/fdb-slice-10-phaseB-202
>>   349 assi Worktrees cleaned. Now delete the stale slice branches (content all on main):
     350 assi 
     351 tres Deleted branch 0.8.3-ce-batch-release (was a02b72b8). Deleted branch 0.8.3-ce-rerank-probe (was c4e1a65b). Deleted branch 0.8.3-gap-decomposition (was 6fe3f2a6). Deleted branch 0.8.3-rerank-accuracy (was 393ccdb5). Delet
     352 atta 
```

## [14] DQ-SHORTKNOWLEDGE::fathomdb/4befb269-585d-4206-abb0-0d42e992260e
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
     864 assi 
     865 tres === PLAN-C decouple boundary (§7 'do NOT wire the engine projection now') === doc: === does Commission C build ANY engine projection / EAV table now? (should be NO) === dev/design/entity-schema-registry/PLAN-C-convergenc
     866 assi 
>>   867 assi Verified from Memex's own `PLAN-C-convergence.md`: line 14 cites **"OPP-12 C-1 (ratified w/ FathomDB)"** and line 81 explicitly lists the **"C-1 engine-projection wiring … ≥0.9.x"** — i.e., Memex has *deferred* the engin
     868 assi 
     869 last 
     870 cust 
```

## [15] DQ-SHORTKNOWLEDGE::fathomdb/54df08ae-c01b-4eae-8862-514eb2cfd198
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
     394 atta 
     395 file 
     396 assi 
>>   397 assi Good orientation — there's `slice30_graph_arm.rs`, `source_id_writes.rs`, and `trace_source_ref.rs` to model conventions on. Let me record the schema-gate sign-off (it must persist), set up phase tracking, and read the k
     398 assi 
     399 tres File created successfully at: /home/coreyt/.claude/projects/-home-coreyt-projects-fathomdb/memory/g0-phase2-schema-gate-signed.md (file state is current in your context — no need to Read it back) --- name: g0-phase2-sche
     400 assi 
```

## [16] DQ-SHORTKNOWLEDGE::fathomdb/5b3699c3-d171-4708-ae6b-9c1cb2ac970e
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
     933 mode 
     934 tres The file /home/coreyt/projects/fathomdb/.claude/worktrees/0.8.8-slice5-close/dev/design/0.8.8-explain-and-telemetry-adr.md has been updated successfully. (file state is current in your context — no need to Read it back)
     935 assi 
>>   936 assi Now replace the §A.3 proposed pseudo-shape with the ratified Rust shape:
     937 assi 
     938 tres The file /home/coreyt/projects/fathomdb/.claude/worktrees/0.8.8-slice5-close/dev/design/0.8.8-explain-and-telemetry-adr.md has been updated successfully. (file state is current in your context — no need to Read it back)
     939 assi Now §A.4 (close Q1–Q3) — replace the open questions with ratified answers:
```

## [17] DQ-SHORTKNOWLEDGE::fathomdb/5ecb659d-eb0a-438a-82a8-523ac501461b
- detector: `DQ-SHORTKNOWLEDGE`  mapped_criteria: ['C7', 'C8']  class: short-knowledge-decision

```
     357 assi 
     358 tres Your questions have been answered: "#137 (TypeScript 6 + @types/node 26) is reviewed above — 2 dep bumps + 1 tsconfig line, byte-identical emit, green. Merge it to close the sweep?"="Merge #137 now". You can now continue
     359 assi 
>>   360 assi Approved — merging #137 and closing out the sweep.
     361 assi 
     362 tres === verify #137 merged === MERGED 2026-06-30T03:14:21Z === origin/main top === 3c695971 build(deps): bump typescript 5->6 + @types/node 25->26 (sweep 0.8.11.1) (#137) 1c73d3f6 build(deps): reconcile dependabot.yml covera
     363 assi 
```

## [18] DQ-ASSUME-STRUCTURAL::fathomdb/54df08ae-c01b-4eae-8862-514eb2cfd198
- detector: `DQ-ASSUME-STRUCTURAL`  mapped_criteria: ['B7', 'C1']  class: unverified-assumption

```
     890 assi 
     891 tres thread 'test_graph_arm_source_id_deterministic_with_multiple_edges' (892814) panicked at src/rust/crates/fathomdb-engine/tests/pr_g0_phase2_frontier.rs:235:9: assertion `left == right` failed: earliest-written edge (writ
     892 assi 
>>   893 assi It returned `docLate` — my assumption about `write_cursor` ordering (early < late) was wrong; the natural ordering gives a different but presumably stable winner. The codex requirement is **determinism**, not a specific 
     894 assi 
     895 tres The file /home/coreyt/projects/fathomdb/src/rust/crates/fathomdb-engine/tests/pr_g0_phase2_frontier.rs has been updated successfully. (file state is current in your context — no need to Read it back)
     896 assi 
```

## [19] DQ-ASSUME-STRUCTURAL::fathomdb/60b48af5-141d-42fb-8d85-51d4010f8eb3
- detector: `DQ-ASSUME-STRUCTURAL`  mapped_criteria: ['B7', 'C1']  class: unverified-assumption

```
       2 mode 
       3 perm 
       4 atta 
>>     5 file 
       6 HITL You are a Steward agent. Read dev/plans/runs/STEWARD-SESSION-HANDOFF-2026-07-02.md. Notes: We should be on main, and continuing the 0.8.12 work. The previous orchestrator has stood-down. The previous orchestrator has com
       7 atta 
       8 atta 
```

## [20] DQ-ASSUME-STRUCTURAL::fathomdb/b9c3de22-af95-4fc9-88f8-d95dc9c4f682
- detector: `DQ-ASSUME-STRUCTURAL`  mapped_criteria: ['B7', 'C1']  class: unverified-assumption

```
    2352 pr-l 
    2353 brid 
    2354 assi 
>>  2355 assi Both verified. Here's where each lands. ## 1. Getting a Memex-consumable build onto the package registries **The "move the tag so it looks like 0.8.9" model doesn't work — and you don't need it to.** Registries version b
    2356 assi 
    2357 tres Your questions have been answered: "How do you want to proceed on publishing a Memex-consumable FathomDB build?"="Prep + dry-run, hold tag", "Do you want me to reconcile the Memex⇄FathomDB Leverage Ledger to FathomDB's c
    2358 assi 
```

## [21] DQ-STALE-VERSION::fathomdb/b9c3de22-af95-4fc9-88f8-d95dc9c4f682
- detector: `DQ-STALE-VERSION`  mapped_criteria: ['E7']  class: stale-artifact-reliance

```
     426 atta 
     427 assi 
     428 assi 0.8.8 landing on main changes PR #93's picture — I need to verify, not assume. Key question: did 0.8.8's push fix the macOS/Windows pyo3 link (PR #93's main red)? Let me check the new main state, main's CI, and both PRs'
>>   429 assi 
     430 tres === new origin/main tip + last 6 === 1c9737ff docs(0.8.8): HITL close-out notes — id-contract substrate dependency + record_feedback reclassify@0.8.11 efcda140 docs(0.8.8 Slice 40): integrated verification GREEN + STATUS
     431 atta 
     432 assi 
```

## [22] DQ-STALE-VERSION::fathomdb/ebec94c7-c59d-493e-816d-ef55e9d6eea5
- detector: `DQ-STALE-VERSION`  mapped_criteria: ['E7']  class: stale-artifact-reliance

```
     166 file 
     167 assi Exact strings captured. Applying the three design-doc edits, then creating the mirror file:
     168 assi 
>>   169 tres The file /home/coreyt/projects/fathomdb/dev/design/record-lifecycle-protocol/README.md has been updated successfully. (file state is current in your context — no need to Read it back)
     170 assi 
     171 tres The file /home/coreyt/projects/fathomdb/dev/design/record-lifecycle-protocol/api-surface.md has been updated successfully. (file state is current in your context — no need to Read it back)
     172 assi 
```

## [23] DQ-IGNOREDESIGN-STRUCTURAL::fathomdb/b9422d49-4abd-4b4d-8145-f72f13b6974f
- detector: `DQ-IGNOREDESIGN-STRUCTURAL`  mapped_criteria: ['C2', 'C3']  class: ignore-design

```
     201 assi 
     202 assi Now standing up the STATUS board (verdict tracker + `$` ledger) in the worktree:
     203 assi 
>>   204 file 
     205 tres File created successfully at: /home/coreyt/projects/fathomdb-worktrees/0.8.11.2/dev/plans/runs/STATUS-0.8.11.2.md (file state is current in your context — no need to Read it back) # STATUS — 0.8.11.2 pico umbrella (OPP-1
     206 atta 
     207 assi 
```

## [24] BLOCK-OVERRIDE::fathomdb/ebec94c7-c59d-493e-816d-ef55e9d6eea5
- detector: `BLOCK-OVERRIDE`  mapped_criteria: ['B2']  class: block-override

```
     846 assi 
     847 tres === origin/main tip (was 6b727391) === 6b727391 === commits since my last (6b727391) === === board: Slice 20 status now === - **Next:** Slice 20 (eu7 no-op regression per D6 + v15→v17 migration verify) — off a fresh `ori
     848 assi 
>>   849 assi No — not finished yet, and I want to be precise rather than guess. Verified from git just now: - **`origin/main` unchanged at `6b727391`** (my last steward commit) — **no new commits** since the eu7 escalation. - **Slice
     850 atta 
     851 syst 
     852 syst 
```

## [25] DQ-NETNEW-DRIFT::fathomdb/60b48af5-141d-42fb-8d85-51d4010f8eb3
- detector: `DQ-NETNEW-DRIFT`  mapped_criteria: ['C3', 'H6', 'E7']  class: design-net-new-drift

```
    1736 perm 
    1737 brid 
    1738 assi 
>>  1739 assi Done on 1 & 2; 3 is running. Status: **Docs folded (task 2)** — `structural-lifecycle-contract.md` §2 now carries the `logical_id` prerequisite, the **`SearchHit.id: write_cursor → logical_id` swap as a GATING co-requisi
    1740 atta 
    1741 syst 
    1742 syst 
```

## [26] DQ-SHORTKNOWLEDGE-TEXTUAL::fathomdb/60b48af5-141d-42fb-8d85-51d4010f8eb3
- detector: `DQ-SHORTKNOWLEDGE-TEXTUAL`  mapped_criteria: ['C7', 'C8', 'C1']  class: short-knowledge-decision

```
    1773 assi 
    1774 tres The file /home/coreyt/.claude/projects/-home-coreyt-projects-fathomdb/memory/MEMORY.md has been updated successfully. (file state is current in your context — no need to Read it back)
    1775 assi 
>>  1776 assi Recorded and indexed (that earlier "recorded to memory" claim is now actually true — my mistake to state it before doing it). Where things stand: the code-grounded audit is complete and saved (`code-grounded-audit.md`); 
    1777 atta 
    1778 syst 
    1779 syst 
```

## [27] DQ-SHORTKNOWLEDGE-TEXTUAL::fathomdb/ebec94c7-c59d-493e-816d-ef55e9d6eea5
- detector: `DQ-SHORTKNOWLEDGE-TEXTUAL`  mapped_criteria: ['C7', 'C8', 'C1']  class: short-knowledge-decision

```
      65 cust 
      66 mode 
      67 assi 
>>    68 assi Read in full. Yes — and it's **substantive but non-blocking.** ## Steward — Memex seq 9 (`voice=MEMEX`, `kind=option`, `converging`) **Verified from the shared ledger** (full unfiltered tail; 9 lines, seq contiguous 1→9)
      69 queu 
      70 queu 
      71 atta 
```

