# DOC-HYGIENE-3 — before/after citation-set diff (TC-100 + TC-94 (1)(2))

**Artifact of record for the HITL `seq-155` DoD.** Both fixes change *which* design
documents a slice's commission brief cites, so "it still exits 0" proves nothing: this
diff is the evidence they changed it **correctly**.

- **Before** = `scripts/commission-manifest.sh` at `ce70973e` (branch point, `origin/main`).
- **After**  = the same script at `92c37cb6` on `doc-hygiene-3-machinery`.
- **Scope**  = every LANDED 0.8.20 slice: 0, 5, 10, 15, 20, 21, 22, 23, 25, 30.
- **State file is unchanged.** No `design_refs`, `short` or `title` was edited to
  produce this diff; every movement below is the selector's, not a curation's.

Reproduce:

```text
for s in 0 5 10 15 20 21 22 23 25 30; do scripts/commission-manifest.sh 0.8.20 $s; done
```

The set below is the union of the design documents a brief cites in **§4 CONTRACT
PATHS** (`design contract` rows) and **§6 DESIGN DOCS** (CURATED + SCANNED rows).
Fixed infrastructure citations (`dev/acceptance.md`, `AGENTS.md`, the allowlist, the
pin) are not design documents and are excluded; none of them moved.

## 1. Summary

| slice | docs before | docs after | added | removed | token list changed |
|---|---|---|---|---|---|
| 0 | 1 | 1 | 0 | 0 | no |
| 5 | 1 | 1 | 0 | 0 | no |
| 10 | 1 | 2 | 1 | 0 | no |
| 15 | 20 | 17 | 3 | 6 | no |
| 20 | 11 | 14 | 4 | 1 | no |
| 21 | 4 | 7 | 3 | 0 | no |
| 22 | 13 | 17 | 4 | 0 | yes |
| 23 | 5 | 8 | 3 | 0 | no |
| 25 | 2 | 2 | 0 | 0 | no |
| 30 | 3 | 3 | 0 | 0 | no |
| **total** | **61** | **72** | **+18** | **-7** | |

**Every removal is a substring artefact and every addition is a higher-authority tier**
— the two are audited one by one in §3 and §4. Four slices (0, 5, 25, 30) do not move
at all, which is itself load-bearing: the change is not a blanket re-selection.

## 2. Per-slice diff

### Slice 0

- tokens before: `(none derived)`
- tokens after: `(none derived)`
- **citation set UNCHANGED** (1 doc(s)).

### Slice 5

- tokens before: `R-20-E1`
- tokens after: `R-20-E1`
- **citation set UNCHANGED** (1 doc(s)).

### Slice 10

- tokens before: `R-20-RV, R-20-NV`
- tokens after: `R-20-RV, R-20-NV`
- **added (1):**
  - `dev/interfaces/rust.md` — matched on `R-20-RV, R-20-NV`

### Slice 15

- tokens before: `TC-33, TC-34, R-20-PR, R-20-EAV, C-1`
- tokens after: `TC-33, TC-34, R-20-PR, R-20-EAV, C-1`
- **added (3):**
  - `dev/interfaces/python.md` — matched on `TC-33, TC-34, R-20-PR, C-1`
  - `dev/interfaces/rust.md` — matched on `TC-34, R-20-PR, C-1`
  - `dev/interfaces/typescript.md` — matched on `TC-33, TC-34, R-20-PR, C-1`
- **removed (6):**
  - `dev/design/0.8.20-erasure-and-h-end-state-v4.md` — had matched on `C-1`
  - `dev/design/0.8.4-graphrag-sensemaking.md` — had matched on `C-1`
  - `dev/design/code-markers-evaluation-2026-07-09.md` — had matched on `C-1`
  - `dev/design/free-threaded-python-value-lift-and-experiments.md` — had matched on `C-1`
  - `dev/design/record-lifecycle-protocol/api-surface.md` — had matched on `C-1`
  - `dev/design/record-lifecycle-protocol/structural-lifecycle-contract.md` — had matched on `C-1`

### Slice 20

- tokens before: `dense_readiness, flush_embeddings, TC-45, R-20-DR`
- tokens after: `dense_readiness, flush_embeddings, TC-45, R-20-DR`
- **added (4):**
  - `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md` — matched on `dense_readiness`
  - `dev/interfaces/python.md` — matched on `flush_embeddings, R-20-DR`
  - `dev/interfaces/rust.md` — matched on `dense_readiness, flush_embeddings, R-20-DR`
  - `dev/interfaces/typescript.md` — matched on `R-20-DR`
- **removed (1):**
  - `dev/design/0.8.20-tc57-write-race-characterization.md` — had matched on `dense_readiness`

### Slice 21

- tokens before: `ac_002, TC-57, TC-71, R-20-CR`
- tokens after: `ac_002, TC-57, TC-71, R-20-CR`
- **added (3):**
  - `dev/interfaces/python.md` — matched on `TC-71`
  - `dev/interfaces/rust.md` — matched on `TC-71`
  - `dev/interfaces/typescript.md` — matched on `TC-71`

### Slice 22

- tokens before: `TC-67, TC-68, R-20-VC`
- tokens after: `TC-67, TC-68, R-20-VC, #18, #99`
- **added (4):**
  - `dev/adr/ADR-0.6.0-error-taxonomy.md` — matched on `#18`
  - `dev/interfaces/python.md` — matched on `TC-67, TC-68, R-20-VC, #18`
  - `dev/interfaces/rust.md` — matched on `TC-67, TC-68, R-20-VC, #18`
  - `dev/interfaces/typescript.md` — matched on `TC-67, TC-68, R-20-VC, #18`

### Slice 23

- tokens before: `TC-90, TC-91, R-20-SV`
- tokens after: `TC-90, TC-91, R-20-SV`
- **added (3):**
  - `dev/interfaces/python.md` — matched on `R-20-SV`
  - `dev/interfaces/rust.md` — matched on `R-20-SV`
  - `dev/interfaces/typescript.md` — matched on `R-20-SV`

### Slice 25

- tokens before: `R-20-SUR`
- tokens after: `R-20-SUR`
- **citation set UNCHANGED** (2 doc(s)).

### Slice 30

- tokens before: `R-20-H7, RUBRIC-H7`
- tokens after: `R-20-H7, RUBRIC-H7`
- **citation set UNCHANGED** (3 doc(s)).

## 3. Audit of every REMOVAL (7) — each is a substring artefact

The word-boundary fix can only ever remove. Each removal below is listed with the
**only form in which the token actually occurs in that document**, extracted with
`re.finditer` over the file and widened to the surrounding identifier — so the
claim "this was a substring artefact" is measured, not asserted.

| slice | removed document | token | the form it actually occurs in |
|---|---|---|---|
| 15 | `dev/design/0.8.4-graphrag-sensemaking.md` | `C-1` | `AC-10`, `AC-15` |
| 15 | `dev/design/0.8.20-erasure-and-h-end-state-v4.md` | `C-1` | `TC-11`, `TC-15`, `TC-17` |
| 15 | `dev/design/code-markers-evaluation-2026-07-09.md` | `C-1` | `TC-1..TC-9`, `TC-10` |
| 15 | `dev/design/free-threaded-python-value-lift-and-experiments.md` | `C-1` | `R-SEC-1.` |
| 15 | `dev/design/record-lifecycle-protocol/api-surface.md` | `C-1` | `TC-11` |
| 15 | `dev/design/record-lifecycle-protocol/structural-lifecycle-contract.md` | `C-1` | `TC-11` |
| 20 | `dev/design/0.8.20-tc57-write-race-characterization.md` | `dense_readiness` | `slice20_dense_readiness.rs` |

**Six of the seven are unambiguous.** `R-SEC-1.` is the clearest of them: a
security-requirement id whose last four characters happened to spell the OPP-12
contract id, which is how a free-threaded-Python experiments memo became required
reading for the projection registry.

**The seventh is a judgement call and is flagged for the Steward rather than
resolved here.** `0.8.20-tc57-write-race-characterization.md` mentions
`dense_readiness` ONLY inside the test filename `slice20_dense_readiness.rs`, so
whole-token matching drops it from Slice 20's set. That is arguably a real
mention. Three reasons it is left as it falls:

1. Slice 20 is **LANDED**; nothing is being commissioned against this set.
2. The document is already the **curated `design_refs` entry for Slice 21**, so it
   is not lost to the release — and `design_refs` is exactly the mechanism for a
   document a selector cannot justify on its own.
3. The alternative — allowing a snake_case token to match inside a longer
   snake_case identifier — reopens TC-100 for the whole `[a-z_]+` pattern class,
   which is the largest and least distinctive of the seven.

**Nothing else was dropped.** Arm 12e re-derives this claim from the live state
file on every run: it re-reads every document a manifest cites, re-checks every
token the manifest REPORTS as matched, and fails if any of them occurs only
inside a longer id. It inspected 100 (document, token) pairs at this commit,
0 spurious — against 66 pairs, 8 spurious, before the fix.

## 4. Audit of every ADDITION (18) — each is a higher-authority tier

All 18 come from tiers the scan could not reach before. **None** comes from
relaxing a match: word-boundary matching is strictly stricter than what it
replaced, so no addition can be a new false positive from that half of the change.

| added document | slices | matched on |
|---|---|---|
| `dev/interfaces/rust.md` | 10, 15, 20, 21, 22, 23 | the slice's own requirement/carry ids |
| `dev/interfaces/python.md` | 15, 20, 21, 22, 23 | the slice's own requirement/carry ids |
| `dev/interfaces/typescript.md` | 15, 20, 21, 22, 23 | the slice's own requirement/carry ids |
| `dev/adr/ADR-0.6.0-error-taxonomy.md` | 22 | `#18` |
| `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md` | 20 | `dense_readiness` |

**The two ADRs are the case TC-94 (1) was filed on.**
`ADR-0.6.0-error-taxonomy.md` opens `**Status:** accepted (HITL 2026-04-27)` and
its second line reads *"Phase 2 #18 design ADR"* — it is the ruling document for
the decision-#18 leg of Slice 22, and before this change **no route but hand
curation could put it in a brief**, at any status. It arrives via the bare-number
token, so TC-94 (1) and TC-94 (2) close each other's last mile:
widening the roots without `#18` would still not have found it.

**The three interface surfaces are not padding.** They are matched on the slice's
own ids because they carry that slice's surface change verbatim — e.g.
`dev/interfaces/rust.md` contains
`> **BREAKING (0.8.20 Slice 22, decision #18).**` and
`**⚠ 0.8.20 Slice 23 (R-20-SV) correction (TC-39 class):**`. `AGENTS.md` obliges
these files to be updated when an error surface changes, and TC-39 records that
obligation as routinely missed — so putting them in front of the orchestrator of
the slice that changes them is the point, not a side effect. They appear only for
slices whose ids they actually name: Slices 0, 5, 25 and 30 do not get them.

## 5. What this diff does NOT show

- **Slices 31, 32, 33 and 40 are absent by design.** 32/33/40 have no design of
  record yet and correctly HARD-FAIL the TC-37 vacuous-pass guard; 31 is the next
  slice and is unlanded, so it is outside the "landed slices" scope this artifact
  was commissioned for. Slice 31 does generate — `scripts/commission-manifest.sh
  0.8.20 31` exits 0 — and is covered by the live test arms, which read
  `landed + next_slice` from the state file.
- **The TC-37 guard is unchanged in both directions.** No slice that hard-failed
  before now passes, and no slice that passed before now fails. The `#[0-9]{2,}`
  two-digit floor is what holds the first half: `Library Sweep #3` in Slices
  31/32/33 is a prose ordinal that would have matched 11 incidental documents
  apiece and turned two honest hard failures into briefs that merely look
  supported.

