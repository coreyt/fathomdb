# Deterministic agentic-failure detectors — formalization

Detects, from Claude Code transcripts, the moments where an agent's plan/action
was **corrected** — by a human (HITL), by a reviewer, by itself, or by a scout
that falsified a load-bearing premise. Pure Python stdlib + regex. **No LLM is
invoked at detection time.** Operates only via streaming scripts over the JSONL;
no transcript is ever read into a model context.

## Pipeline
- `parse.py` — streaming normalizer. `iter_file(path)` two-passes one file
  (uuid→type map, then emit) yielding `{file,line_no,ts,type,role,is_sidechain,
  is_subfile,is_tool_result,is_hitl,uuid,parent_uuid,parent_type,text,
  tool_names,tool_input_text}`. Flattens `str | list[block]` content, joins
  assistant text blocks, extracts `tool_result` / `toolUseResult` text. Memory
  is bounded by lines-per-file, never by content size.
- `detectors.py` — families A–E; `detect_file(records)` runs all over one file's
  records (so structural detectors can use within-file order).
- `run_detectors.py <split> <out.jsonl>` — file-by-file (per-record text capped
  at 24 KB to bound RAM), writes candidate rows + `<out>.summary.json`. Every
  candidate is stamped with `parent_session` (session-coherent group id) and a
  stable content `fingerprint` (see below).
- `resplit_by_session.py` — **splits the transcript universe by PARENT SESSION,
  not by file** (fixes the fold-leak, F1). Every file of one session — parent
  transcript, all subagents, workflow journals — is assigned to the same fold, so
  a held-out validation file can never be a near-duplicate sibling (same prompt /
  task / review cycle) of an experiment file. Deterministic hash split targeting
  ~31 % validation; the two ground-truth RCA sessions are pinned to experiment.
- `seed_window.py` / `sample_seed.py` / `sample_seed_stride.py` — labeling aids
  that print only ≤±3 normalized lines or per-detector distinct-session samples.
- `make_seed_labels.py` — re-attaches hand-labels to candidates by **fingerprint**
  (`parse.candidate_fingerprint` = sha1 of `parent_session | file | line_no |
  matched_signal`), never by stride position (fixes the silent label-remap, F4).
  **N1: snippet is NOT part of the key.** Including `sha1(snippet)` (the earlier
  design) meant any ±window/regex tweak re-hashed every candidate and orphaned
  labels wholesale — against the current candidate set, the old snippet-inclusive
  key orphaned **all 49** labels (0 survivors, no like-for-like pre/post delta);
  the identity key `parent_session|file|line_no|matched_signal` keeps a still-valid
  detection pinned through a cosmetic window change and orphans only on a genuine
  move (the detection's line or matched_signal changed, or its session moved fold).
  The durable label store is `seed_labels_fp.jsonl`. A label whose fingerprinted
  candidate is still present is a **survivor** (scored); a label whose candidate is
  gone/changed is an **orphan** (reported loudly, needs relabel, not scored). Every
  label row is accounted for. Recomputes precision → `seed_labels.tsv` with Wilson CIs.

## Detectors

**Shared pre-filter `is_synthetic`** (drops injected user lines that look like
HITL): `<command-*>`, `<system-reminder>`, `Stop hook feedback:`, coordinator
relays, `Base directory for this skill:`, `<task-notification>`,
`[SYSTEM NOTIFICATION`, `NOT USER INPUT`, continuation/goal/review-target banners.
Anchored to the turn's leading 400 chars so a genuine HITL turn that merely
quotes a marker downstream is not dropped.

- **A / hitl-bounce** (rubric D1/D4/D7). MAIN-session user turns that are not
  tool_results and not synthetic. Four sub-detectors:
  - `hitl-strong-rejection` — turn-initial `no,` (neg-lookahead for "no
    problem/worries/rush…"), `that's wrong/not what`, `you missed/forgot/misread`,
    `reconsider`, `step back`, `wrong way/approach/about`, `why did you`.
  - `hitl-soft-redirect` — `actually…`, `re-read`/`re-check`, `do X instead`,
    `instead` (drops the `instead of` comparative). **Requires structural context**
    (replies to a prior assistant turn) — this alone removed the dominant FP mass
    (standing "Re-check … and resume only if cleared" resume-prompts).
  - `hitl-reversal-rework` — `revert/undo/roll back`, `redo it`, `do a … instead`;
    confidence bumped to high when the nearest prior assistant turn carried a
    mutating tool_use (Edit/Write/Bash git-commit).
  - `hitl-scope-topology-correction` — the RCA signatures (`SCOPE CORRECTED`,
    `wrong about the topology`, `which way is intended`, `is a duplicate`,
    `no live consumers`, `already superseded`, `~N sites`). Only `SCOPE CORRECTED`
    / `wrong about` are self-evident; the rest require a co-occurring correction
    cue **and** structural context (they otherwise fire on spawn-prompts).
  - sibling: `agent-to-agent-bounce` — coordinator→subagent relays, emitted as
    `info` (a bounce, but never HITL consent).
  - Structural helper: nearest-prior-**assistant**-by-sequence (parentUuid is
    unreliable in main sessions — it usually points at a tool_result line).
  - **Dedup (F5)**: one HITL turn = one candidate. When several sub-blocks fire on
    the same `(file,line_no)` turn, precedence `strong > reversal > scope > soft`
    keeps a single row so a turn is never counted as two independent TPs/FPs.

- **B / unwitnessed-premise** (premise-substitution — the CR-047 / 30-N root
  cause). Assistant text asserts a topology fact — `no consumers/callers`,
  `already/now superseded`, `is a duplicate`, `owned by X now`, `~N sites`,
  `nothing uses`, `safe to delete`, `dead code` — with **no witness** in a ±200-char
  window (no grep/rg/verified/`path:line`/code-fence) **and** a code-subject
  present (drops pure doc-supersession vocab). High confidence when the claim is
  action-adjacent (delete/finish/consolidate/skip) and unhedged. **Exclusions (F7)**:
  an **enumerated file/callsite list** (≥2 `path.ext` tokens) in the window counts
  as a witness (a change-summary that names the sites it touched); a **hardware/
  compute subject** (`K620`, `GPU`, `CUDA`, `VRAM`, `device`, `compute`…) is not
  code-deadness and is dropped; **retrospective/RCA meta-prose** (`RCA`, `retro`,
  `post-mortem`, `never grepped`, `the mistake was`, `got three things wrong`,
  `had to be … replaced`, `HITL-ratified`, `orchestrator scouted`…) quoting a past
  premise is dropped. **N2 — self-referential rubric guard**: this suite's OWN
  design/rubric prose describes the premise-substitution criteria and thereby quotes
  the very phrases the detector keys on (`RC-4 (unwitnessed plan premises)`, `is not
  a witness`, `criterion C8`, `detector`, `rubric`, `ratification`) — those windows
  are dropped so the detector does not fire on descriptions of itself. This removed
  all 3 self-referential B hits in the pinned rubric session `f57b5dee`; the genuine
  CR-047 premises (`No live consumers found → DELETE`, `~98 call sites`) still fire
  (see smoke-test).

- **C / review-catch** (rubric D5), **single narration channel (C1)**. A verdict
  token (`BLOCK`, `CONCERN`, `must-fix`, `P0/P1/P2`, a finding id) within 400 chars
  of a review anchor → `channel:"narration"`, med (or low for gate-policy narration
  like `BLOCK→HITL`). A code-file / git-log **echo** of a `codex §9` provenance
  mention is **dropped**, not demoted — it is provenance, not a catch: numbered
  source lines, `path:line`, git short-hash, **and (C2) numbered source-comment
  blocks that cite a finding-id without an adjacent `codex/§9` token** (≥2 leading
  `NNN #|//|*` comment markers — e.g. `101 # … (R-ONNX-2)`), which previously slipped
  the guard and fired as spurious catches across sibling subagent captures.
  - **No trustworthy "strict" channel (C1, finding option b).** An earlier design
    claimed a *strict* channel (reviewer tool_use/tool_result → high confidence).
    It was **dead code**: the verdict lands on the *user tool_result* line whereas
    the review-tool identity is only set from the *assistant* tool_use line, so the
    two never coincided (0/1127). A `tool_use_id` linkage back to the originating
    review tool was prototyped and *does* reach the result line — but this corpus's
    review mechanism is **codex-via-Bash emitting free-form prose** in which
    `BLOCK`/`CONCERN`/`P0`/finding-id tokens appear in PASS summaries, spec text
    (`decide_083 BLOCK on eu7<0.90`), commit lines and code echoes **indistinguishably
    from real catches** (~50 % FP even under a tight review-invocation + catch-verdict
    filter). So no deterministic regex-only *trustworthy* tier exists: **all C is
    narration-grade** and `channel` is a constant label. Downstream must not treat
    any C candidate as structurally vouched.

- **D / self-correction** (rubric D3). Assistant admits its OWN error —
  `I misunderstood/misread`, `I was wrong`, `that was incorrect`,
  `let me correct`, `I falsely assumed`, `on closer inspection`, `my mistake`,
  `you're right`/`good catch`. High confidence for `I was wrong` / `you're right`
  / `falsely assumed` / `misunderstood`. **Guard (F7)**: the edit-intent phrases
  `let me correct` / `correcting my` fire only when an explicit error term
  (`wrong`, `incorrect`, `mistake`, `misread`, `conflated`, `opposite`,
  `duplicate`…) co-occurs in the window — otherwise they catch ordinary
  edit intent (`let me correct and expand the note`) and are dropped.

- **E / halt-scout-falsify** (the RCA save-mechanism). Assistant text where a
  premise proved false → `SCOPE CORRECTED`, `I/we/that was WRONG`, `stand
  corrected`, `retract`, `there are still consumers`, `not a duplicate`, or an
  explicit `premise/assumption … was false/wrong`, optionally with a `HALT`/scout
  cue. **Guards (F7)**: a negation guard (30 c, real-negation regex — a bare
  `n't` no longer matches the `nt` inside `markdownli**nt**`) drops defended
  claims (`is NOT actually wrong`); a **rebuttal guard** drops adversarial
  severity-debate (`overstates … into a defect`, `only the cost side is
  half-wrong`, `in tension`); and a candidate must be a **credible reversal** —
  first-person (`I/we was wrong`, `my assumption…`), an explicit premise-now-false
  clause, or a concrete falsification (`still consumers`, `not a duplicate`, `still
  in use`). This is the ratification→reversal pair the task calls the highest-value
  class.

## Seed precision — MACRO stride, NOT a population estimate (F2)

Precision below is **macro seed precision over a capped per-detector
distinct-session stride** (N ≤ 10/detector). It is **not** a population estimate:
review-catch is ~88 % of real candidate volume but contributes only ~4 labels, so
this macro number under-weights the dominant class. Numbers are on the
**session-coherent** experiment split (F1) with labels **pinned by fingerprint**
(F4); `*` marks cells with N < 5 (insufficient sample, F8). Wilson 95 % CIs are
printed by `make_seed_labels.py`.

| detector | precision (N) | Wilson 95 % |
|---|---|---|
| self-correction | 7/7 = 1.00 | [0.65, 1.00] |
| halt-scout-falsify | 6/7 = 0.86 | [0.49, 0.97] |
| hitl-strong-rejection | 2/2 = 1.00 `*` | [0.34, 1.00] |
| hitl-reversal-rework | 1/1 = 1.00 `*` | [0.21, 1.00] |
| review-catch | 3/3 = 1.00 `*` | [0.44, 1.00] |
| unwitnessed-premise | 3/4 = 0.75 `*` | [0.30, 0.95] |
| hitl-soft-redirect | 2/3 = 0.67 `*` | [0.21, 0.94] |

| family | precision | confidence tier | precision | Wilson 95 % |
|---|---|---|---|---|
| D-self-correction | 7/7 = 1.00 | high | 5/5 = 1.00 | [0.57, 1.00] |
| A-hitl-bounce | 5/6 = 0.83 | med | 18/21 = 0.86 | [0.65, 0.95] |
| E-halt-scout-falsify | 6/7 = 0.86 | low | 1/1 = 1.00 | [0.21, 1.00] |
| B-unwitnessed-premise | 3/4 = 0.75 | **overall (macro)** | **24/27 = 0.89** | [0.72, 0.96] |
| C-review-catch | 3/3 = 1.00 | | | |

(N moved from 28→27 survivors after the C1/C2/N1/N2 round — the review-catch cell
lost 1 orphaned label and its FP survivor was among the echo-FPs the C2 guard now
drops, so the surviving review-catch seed is 3/3; see Known-Gap #1 for the orphan
accounting. The gain is not a like-for-like measured delta — it is a survivor-set
change — pending the fresh hand-label pass.)

**Candidate-weighted approximation** (each detector's seed rate × its real
volume, illustrative only): **≈0.99**, but this is now *dominated by an N = 3
review-catch cell* (88 % of volume × seed_rate 1.00) and is therefore even less
defensible than the ≈0.77 it replaced — do **not** read it as a population figure.
A real population number requires a **stratified random sample** (strata = detector
× confidence) with known inclusion probabilities, **Horvitz-Thompson-weighted**
precision + Wilson CIs, and a review-catch sample proportional to its ~88 % share
(review-catch is a single narration channel, C1, so its FP surface is real). That
sample is a Validate-phase task, not done here.

The `confidence_heuristic` is **monotonic in the seed** (high ≥ med ≥ low) but the
tier Ns are far too small to **confirm** the ordering (F3) — high N = 5, low N = 1;
their Wilson intervals overlap almost completely. Treat it as a suggestive trend,
not a validated threshold. Labels + reasoning are in `seed_labels.tsv`;
fingerprinted store in `seed_labels_fp.jsonl`.

## Smoke-test against the known positives (CR-047, 30-N)

This is an **existence smoke-test** — "do the canonical root-cause phrases fire?"
— **not** a recall figure (there is no enumerated denominator of correction
moments per episode, so no recall fraction is claimed, F8). The 30-N session
`f57b5dee` is pinned to the experiment split; CR-047 `2fa060bc` (memex-side) is
assigned by the session-coherent split.

- **30-N** (plan-delta): the pinned session `f57b5dee…` is the rubric-DESIGN session
  that *discusses* 30-N; its `no consumers` / `~N call sites` / `is a duplicate`
  matches were all **self-referential meta** describing the detector criteria (`RC-4
  (unwitnessed plan premises)`, `is not a witness`, the ratify→reverse RCA narrative)
  and are now correctly **suppressed** (N2 + F7-B) — all 3 B hits in this session are
  gone. The genuine 30-N ratify→reverse phrases live in the memex-side RCA doc.
- **CR-047** (finish-vs-delete): `2fa060bc…` fires **unwitnessed-premise ×2** on the
  live premises (`No live consumers found → DELETE`, L1617 `no_consumers+action`;
  `~98 call sites`, L1495 `n_sites`), and the save-mechanism reversal fires **E**.
  (Re-derived from `out/experiment_candidates.jsonl`, not memory: the earlier "×3"
  and its third premise `never called by any test` were fabricated — that string
  does not occur anywhere in session `2fa060bc`. A third B candidate does exist in
  the session-coherent group but it is a `not_used_anywhere` facade line in a
  subagent capture — a different signal, not one of the load-bearing CR-047
  premises — so the smoke-test counts only the two live premises above.)

Families B (premise) and E (reversal) fire on the CR-047 positive; D
(self-correction) fires across the corpus (76 candidates, 7/7 seed-precision).

## Known gaps / open items for the Validate phase

1. **Label re-verification**: after the C1/C2/N1/N2 round, 23 of 50 labels are
   orphaned; the **N1 identity fingerprint** (drops `sha1(snippet)`) is what makes
   the remaining 27 scoreable at all — under the old snippet-inclusive key every one
   of the 49 pre-round labels orphaned against the current candidates. The 23 orphans
   break down as: **11** moved to the sealed validation fold, **7** are FP-labels
   whose candidate a hardened guard (C2 echo / F7-B hardware/enumerated / N2 rubric)
   correctly dropped, **1** is a deliberate C2 regression label (`a2a9f976:18`,
   expected-orphan), **3** are pre-existing non-firing TP labels (did not fire in the
   pre-round baseline either — stale from an earlier candidate set), **1** is a dedup
   signal-drift (`0a6e63f1:716` now fires as `hitl-strong-rejection/step_back`, not
   the labeled `hitl-soft-redirect/actually_lead`). A fresh hand-label pass over the
   orphans is still owed for a like-for-like measured pre/post delta.
2. **Population precision** still owed: draw the stratified H-T sample described
   above (review-catch dominates volume and sets real-world precision).
3. **Family C is a single narration channel (C1)** and remains the biggest FP
   surface (med-tier prose narrating a verdict). There is **no trustworthy strict
   channel** in this corpus — the reviewer mechanism is codex-via-Bash free-form
   prose where catch-tokens are indistinguishable from PASS-summaries/spec/echoes by
   regex (see Family C above). A structurally-vouched signal would require a typed
   reviewer tool (`ReportFindings`), which is near-absent here (1 call in the split).
4. **hitl-scope-topology-correction** fires on ~1 candidate post-hardening; its RCA
   phrases are better carried by family B on the assistant side.

## Non-token-burning properties
- **Runtime**: ~25 s for the 2,296-file / ~178 k-record experiment split on one core.
- **No LLM at detection time** — regex + structural indexing only; fully
  deterministic and re-runnable.
- **Bounded context**: detection reads only aggregates + ≤300-char snippets;
  labeling reads only ≤±3 normalized lines. No transcript is ever opened whole.
- Outputs: `out/experiment_candidates.jsonl` (**1,269** candidates; the C1/C2/N1/N2
  round trimmed 10 from 1,279 — the C2 numbered-source-comment echo guard dropped 7
  review-catch (1,127 → 1,120) and the N2 rubric-meta guard dropped 3 self-referential
  B hits (20 → 17). Earlier drop from 1,919 was dominated by the Family-C echo removal
  (review-catch 1,763 → 1,127) plus the F7 B/D/E guards and the A-family dedup; the
  session-coherent re-split changed the file set only slightly, 2,326 → 2,296)
  + `out/experiment_candidates.jsonl.summary.json`.
