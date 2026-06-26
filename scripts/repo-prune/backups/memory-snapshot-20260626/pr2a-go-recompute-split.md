---
name: pr2a-go-recompute-split
description: "0.7.2 recall — PR-2a's mean GO was OVERTURNED; the gap was a measurement artifact (ANN 0.937 clears the floor)"
metadata: 
  node_type: memory
  type: project
  originSessionId: 5e309f70-21d0-432b-af9b-9e5bc364b025
---

**SUPERSEDED premise (kept for history):** PR-2a (2026-05-30) attributed EU-7's
real-corpus recall@10=0.828 to a mean-centering artifact (single-source first-256
pin) and returned GO, projecting recovery to ~0.945. HITL split the fix into
**PR-2b** (production mean recompute: drift detector + `doctor recompute-mean`;
on local main `b96c850`+`e187add`, drift policy 0.95/256/EWMA 1/256) and **PR-2c**
(representative initial pin: reservoir + deferred settle=2048; branch
`0.7.2-PR-2c-representative-pin` `e8d5538`). Both built + GREEN.

**ROOT CAUSE (2026-05-31) — the GO was wrong:** the gap is a **measurement
artifact**, not the mean/embedder/engine.
- Forcing the full-corpus mean on the real engine = **0.847** (only +1.9pp over the
  worst mean) → **the mean is a non-lever**. candle vs HF embeddings **bit-identical**.
- ~6pp of the offline→real gap = conservative harness methodology (target excluded
  AFTER top-10; body-string GT with 5.6% dup bodies).
- **Corrected ANN recall@10 = 0.937 (CI 0.913–0.957) → clears the 0.90 floor.**
- New IR-recall signal (EU-8) = **0.571** (embedder ceiling) ≪ 0.937 ANN fidelity →
  quantization is NOT the bottleneck; don't chase K/ANN tuning.

**RESOLVED + EXECUTED (2026-05-31, HITL-ratified):** the keep-vs-shelve reassessment
is decided and landed on local `main` (HEAD ~`5dd0b52`; nothing pushed). Memo:
`dev/plans/runs/0.7.2-PR-2bc-decision.md` (RATIFIED, with execution outcome table).
- **PR-2c: SHELVED** — branch `0.7.2-PR-2c-representative-pin` parked unmerged (git
  branch description set). Recall-negative (extends un-centered window 256→2048) for
  zero benefit; redundant with PR-2b recompute.
- **PR-2b: MODIFIED** — manual `doctor recompute-mean` + recompute core KEPT; the
  **automatic in-ingest drift detector + 200k cap + `MeanRecomputeDeferred` + `DriftAuto`
  DEFERRED to 0.8.x** (carved out, RED-guarded). Parked design:
  `dev/plans/prompts/0.8.x-auto-mean-drift-DEFERRED.md`.
- **Harness fixes LANDED**: ANN fix `a601d1c` (note: its `EU7_SEARCH_LIMIT` env seam was
  a codex BLOCK → replaced with `set_search_limit_for_test` atomic) + EU-8 IR `5658d39`.
- **PR-2 floor: KEPT at 0.90**; ADR-0.7.0-vector-binary-quant §2.4 amended to cite
  ANN-FIDELITY 0.937 (NOT IR); EU-8 IR 0.571 recorded as a separate ceiling, not a gate;
  sentinel `ac_013b_floor_matches_adr` added.

Executed via orchestrated in-harness Agent implementers (TDD) + `codex exec` review per
slice (one BLOCK→fix-1, two CONCERN-accepted). **Still open:** AC-019 stress MISS
(1201 vs 499 ms) contention-inflated — re-run on idle box; not a regression. Nothing
pushed. [[dont-dismiss-user-directed-subagents]]
