# Perf regression detection (0.7.2 PR-7)

**Status:** landed (0.7.2 PR-7). Owns `dev/perf-history/` + the
`perf-regression-check` binary (`fathomdb-cli` crate, layout (A) — a
`[[bin]]` entry, NOT a new workspace crate).

## Purpose

Persist canonical-CI / release-time performance numbers per commit and
surface regressions automatically with a deliberately **low false-positive
rate**. The store is the ship-verdict history; the bin reads it, groups by
`(ac_id, n)`, and compares the single most-recent run in each group against
the rolling median of the prior runs in that group.

## Append-only invariant

`dev/perf-history/` is **append-only**. The `perf-regression-check` binary
**only reads** the directory — it never writes, mutates, or deletes any
file. New rows are written by CI (or by a human at release time, for the
locally-measured AC-013 / AC-013b / AC-019 numbers). This keeps the history
a tamper-evident, git-tracked audit trail: a regression is a new file whose
numbers the bin flags, not an edit to an old file.

## Record schema

One JSON file per canonical run (filename convention:
`<ac_id>-<commit_sha:8>-<n>.json`, e.g. `AC-013-c893d8b0-10000.json`). The
bin globs `*.json` in the directory and parses each file as one record:

```json
{
  "commit_sha": "c893d8b0",
  "ac_id": "AC-013",
  "n": 10000,
  "p50_ms": 36.0,
  "p99_ms": 49.0,
  "recall": 0.937,
  "timestamp": "2026-06-01T07:23:39Z"
}
```

| field        | type            | required | notes |
|--------------|-----------------|----------|-------|
| `commit_sha` | string          | yes      | commit the run was measured at |
| `ac_id`      | string          | yes      | `AC-012` \| `AC-013` \| `AC-013b` \| `AC-019` \| … |
| `n`          | integer         | yes      | corpus row count; part of the grouping key |
| `p50_ms`     | number          | no\*     | median latency; omit for recall-only ACs |
| `p99_ms`     | number          | no\*     | tail latency; omit for recall-only ACs |
| `recall`     | number \| null  | no       | recall@10; omit/null for latency-only ACs (AC-012, AC-019) |
| `timestamp`  | RFC3339 string  | yes      | run time; used to pick the single latest run per group |

\* A record must carry at least one comparable metric (a latency pair or a
recall value), else it is malformed.

**Forward-extensibility:** unknown fields are **ignored, not rejected**.
Adding fields to the schema later does not break older readers or invalidate
older files. (`serde` with default `deny_unknown_fields` OFF.)

## Grouping — tier-aware

Records are grouped by the **`(ac_id, n)`** pair. This is deliberate:

- AC-013 @ N=10k is never compared against AC-013 @ N=100k (different
  latency tier — see `ADR-0.7.0-text-query-latency-gates-revised.md`, the
  10k binding tier vs the 100k/1M tracked tiers).
- The real-corpus anchor (N=7667) is a **different group** from the
  synthetic N=10000 run — real bge and synthetic isotropic data have
  different absolute numbers and must not cross-contaminate a median.

**Regime caveat (grouping is necessary but not sufficient).** `(ac_id, n)`
does not capture every comparability axis. The K-sweep that landed in 0.7.1
(top-K bit-candidates 64 → 192, the EU-7/PR-2 recall fix) raised rerank cost,
so pre-0.7.1 (K=64) latency is **not** directly comparable to 0.7.1+ (K=192)
latency even at the same `(ac_id, n)`. The committed store therefore does
**not** mix K=64 and K=192 latency in one group — the synthetic AC-013 @
N=10000 latency history begins at the K=192 PR-3 baseline (`c893d8b`). The
K=64-era synthetic recall arc (AC-013b @ N=10000) is kept as one
internally-consistent group because every point in it is K=64. When a future
regime change (a new K, an ANN index) lands, start a fresh comparison rather
than letting the detector flag the deliberate step as a regression.

## Detection thresholds (HITL Gate 1 — LOCKED 2026-06-01)

Per HITL sign-off on 2026-06-01, the thresholds are:

| metric  | flag condition                                            |
|---------|-----------------------------------------------------------|
| latency | `p50` **or** `p99` degraded **> 15%** vs the rolling median |
| recall  | recall degraded **> 0.03 absolute** vs the rolling median   |
| window  | rolling median of the prior **up to 10** runs per group     |
| latest  | the **single most-recent** run (by timestamp) per group     |

**Rationale (false-positive resistance is the design priority — codex's
review focus):**

- Recall σ = **0.0116** (PR-3 measured, N=7667 anchor;
  `0.7.2-PR-3-perf-data.md`). A **0.03** absolute threshold is ≈ **2.4σ**,
  i.e. roughly a **~1% jitter false-flag rate** — the deliberate
  low-cry-wolf choice. We would rather miss a marginal real drift than fire
  on noise, because a fired flag costs human triage on every canonical run.
- **Tradeoff acknowledged:** a 0.03 recall drop from the 0.937 anchor lands
  at **0.907**, which is right above the **0.90** floor. So this threshold
  favours **false-positive resistance over early-warning sensitivity**: a
  recall decay that creeps toward the floor in <0.03 steps will not trip the
  detector before it nears 0.90. That is an accepted limitation; the hard
  recall floor is enforced separately by the AC-013b gate, not by this
  drift detector.
- **15% latency** allows for normal run-to-run variance on shared CI
  hardware (the canonical runner is a 4-core EPYC slice; cold-cache and
  neighbour-noise swing latency more than recall).

The "latest = single most-recent run vs the median of the prior runs in the
window" definition means a one-off bad run flags immediately (good for catch
latency) but a single good run after a bad streak does not mask the streak
(the median is robust to one outlier).

## Insufficient history

A group with **fewer than 2 runs** (i.e. no prior runs to form a median
after removing the latest) is reported as **insufficient-history** — NOT a
regression and NOT a crash. The AC-012 @ N=1M group is exactly this case
today (one documented canonical run, v0.6.1 `603a4bc`).

## Timestamp parsing

`timestamp` is parsed as **strict RFC3339** (`YYYY-MM-DDTHH:MM:SS(.frac)?(Z|±HH:MM)`)
to UTC epoch seconds at load time, and the **parsed epoch** — not the raw
string — is the ordering key for "latest run". A lexical string sort would
silently mis-order offset-bearing or differently-formatted timestamps and
thus pick the wrong "latest" run; instead, a timestamp that does not parse is
a **data-integrity failure (exit 2)**, surfaced loudly rather than tolerated.
The parser is dependency-free (the workspace pulls in no date crate); see
`parse_rfc3339_epoch` + its unit tests in the bin source. Ties on the same
instant break deterministically on `commit_sha`.

## Exit codes

| code | meaning |
|------|---------|
| `0`  | no regression flagged (includes all-insufficient-history) |
| `1`  | at least one group flagged a regression |
| `2`  | data missing or malformed (directory absent, unreadable JSON, record with no comparable metric, **or an unparseable timestamp**) |

## Output

- Default: human-readable report to stdout (one block per group; flagged
  groups marked clearly).
- `--json`: machine-parseable JSON object to stdout (per-group verdicts +
  an overall `flagged` boolean), for CI and tooling to consume.

## CI integration (`.github/workflows/perf-canonical.yml`)

After the AC-012 canonical run, the workflow builds and runs
`perf-regression-check dev/perf-history/`, echoes the report to the step log,
and writes it to **`$GITHUB_STEP_SUMMARY`**.

- **Channel choice:** this workflow is **`workflow_dispatch`-only**, so there
  is no `pull_request`/`push` event to comment on — a PR-comment step would be
  dead code on every real run, and `pull-requests: write` would be an unused
  permission over-grant. The step summary is the `workflow_dispatch`-safe
  surface, always present, and visible on the run page. The workflow keeps
  only `contents: read`.
- Exit **1** (regression flagged): the step **continues** — the summary is
  the signal, not a hard failure. (A regression on a diagnostic dispatch run
  should not red-X the run; a human reads the summary.)
- Exit **2** (data integrity): the step **fails loudly** (`::error::` + non-zero
  exit) — malformed history is a real defect in the store and must block.

The workflow stays `workflow_dispatch`-only (no push/PR/schedule triggers
added) and keeps within GitHub's workflow_dispatch input cap (≤ 8 inputs, per
the prior consolidation fix).

## Backfill provenance

The committed `dev/perf-history/` rows are backfilled from:

- **AC-012 @ N=1M** — v0.6.1 canonical CI run `603a4bc` (run 26346417896,
  2026-05-23): p50=140.95, p99=458, verdict RED (over the 20/150 ADR
  budget). Source: `dev/notes/perf-canonical-runner-2026-MM.md`. **This is
  the only documented AC-012 canonical run → its group is sparse
  (insufficient history).** It is an honestly-RED historical point, not a
  regression the detector should flag (no prior to compare against).
- **AC-013b synthetic @ N=10000** — the 2026-05-27 PERF-VECTOR-QUANT recall
  arc (K=64 throughout), the full four-point sequence:
  - `035cfa3` (the buggy parent of the fix): recall **1.0** — **DEGENERATE**.
    The batch-collapse left ~10 distinct vec0 rows per 10K writes, so the
    brute-force ground truth and production both returned the same
    trivially-small set, making recall *falsely perfect*
    (`STATUS-perf-vector-quant.md:138`). This is a **measurement artifact, not
    a verdict** (so marked in the row's `note`).
  - `4a95cfd` (the fix) + scanner fix `53a270d`: recall **0.1572** — the
    *honest* number once the collapse was repaired. Low not because of the
    bug but because `VaryingEmbedder` produces sparse vectors (6/768 non-zero)
    — a synthetic-fixture pathology (`:143`).
  - `38f5e3a` (Option-1 dense fixture): recall **0.5124**.
  - `38d5f4f` (v0.7.0 ship): recall **0.5124** — AC-013b NOT MET on the
    synthetic fixture (deferred to 0.7.1 EMBEDDER-UNDEFER).
- **AC-013 / AC-013b / AC-019 real bge @ N=7667** and **AC-013 synthetic @
  N=10000 (K=192)** forward baseline — v0.7.1 / 0.7.2 (`c893d8b`, 2026-06-01) +
  PR-3 local data. Source: `dev/plans/runs/0.7.2-PR-3-perf-data.md`. AC-013
  rows are latency-only; AC-013b rows are recall-only (the two ACs measure
  different things).

### Honesty note — what the batch-collapse bug actually did, and what this detector can and cannot catch

The handoff success criterion says the bin should "flag the 2026-05-27
batch-collapse bug." Tracing the real numbers (`STATUS-perf-vector-quant.md`)
shows the premise needs correcting, and we record the truth rather than
contorting the data to fit it:

**The batch-collapse bug masqueraded as an *improvement*, not a regression.**
With the index collapsed to ~10 rows, recall read a *false* 1.0 and latency
was *anomalously fast* (the bit-KNN searched a ~10-row partition instead of
10K). A degradation-only detector — flag recall *drops* and latency *rises*,
which is exactly what HITL locked — **structurally cannot flag that bug**;
the bug's signature is the opposite of a degradation. What the detector *does*
catch is the **regression-shaped correction** that follows: when `4a95cfd`
repaired the collapse, recall fell from the degenerate 1.0 to the honest
0.1572 — a 0.84 drop the detector flags (the `backfill-dirty` fixture proves
this; the flagged commit is the *fix*, `4a95cfd`, which is counter-intuitive
but honest). The committed store keeps the full arc; its latest run per group
(v0.7.0, 0.5124) is healthy, so the committed store itself is **clean**.

Catching the *bug itself* would require a different check — a row-count /
vec0-population invariant, or an "implausible perfection" anomaly rule (recall
== 1.0 exactly, or latency falling as N rises). That is **deliberately out of
scope** for this degradation detector (it would expand past the HITL-locked
15%/0.03 thresholds and risk false positives on legitimately-perfect small-N
recall) and is logged here as **future work**.

**The v0.6.1 AC-012 @ N=1M run was a real RED verdict** (7× over the p50
budget) — preserved faithfully in the store, not smoothed away. It does not
flag as a *regression* only because it is the sole point in its group
(insufficient history); its RED-ness is a separate budget fact recorded in
`perf-canonical-runner-2026-MM.md`.

**Sparse history is the honest current state.** Because of the regime changes
above (K=64 → K=192, synthetic → real bge) and the once-per-release cadence,
most `(ac_id, n)` groups today hold a single comparable point and report
*insufficient-history*. The only group with a real multi-point comparison is
the K=64 AC-013b @ N=10000 arc. The store accrues comparability as new
canonical runs are appended on a stable regime; the detector is wired and
correct now, and gets more sensitive as history deepens.
