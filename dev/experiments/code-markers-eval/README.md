# code-markers-eval

Out-of-band experiment: **do in-source "markers" (structured comments that
cross-reference an external governance artifact — an ADR, an acceptance/REQ id,
a TC ledger entry, a design doc, a ledger seq) add value at code-review time,
and what lifecycle would they need to not rot into misinformation?**

This is an **experiment, not an implementation**. Nothing here adds markers to
product source, adds CI, or modifies engine code. It measures the marker-like
cross-references **that already exist** in this repo and how they have aged over
git history — all at **0 LLM at detection** (pure stdlib + regex + `git`).

Findings and verdict: `../../design/code-markers-evaluation-2026-07-09.md`.

## Reference classes measured (natural experiments)

The program already uses marker-like refs heavily; we mine those rather than
speculate:

| Class          | Where                         | Artifact side / registry                          |
|----------------|-------------------------------|---------------------------------------------------|
| ADR-PATH       | `src/**` comments             | `dev/adr/*.md` (filesystem)                        |
| DESIGN-PATH    | `src/**` comments             | `dev/design/*.md` (filesystem)                     |
| ADR-ID         | `src/**` comments             | `dev/adr/` filename prefix                         |
| AC / REQ       | `src/**` comments + tests     | ids defined in `dev/acceptance.md`                 |
| TC             | `src/**`, commit messages     | `dev/todos-and-considerations-ledger.jsonl` (+status) |
| F              | `src/**`, commit messages     | `dev/plans/*.md` mentions                          |
| ledger seq     | commit messages               | steward / todos ledger max seq                    |
| Claude-Session | commit trailers               | external URL (unresolvable offline)               |
| `[[wikilink]]` | auto-memory `*.md`            | sibling `*.md` in the memory dir                  |
| `@sha` pin     | `dev/design`, `dev/adr`       | git object + ancestor reachability                |

## Reproduce

```bash
python3 mine_incode_markers.py    # -> out/incode_markers.jsonl, out/incode_summary.json
python3 mine_drift_blame.py       # -> out/drift.jsonl,          out/drift_summary.json
python3 mine_natural_refs.py      # -> out/natural_refs.json, out/wikilinks.jsonl, out/commit_refs.jsonl
```

Deterministic; no network, no LLM, no writes outside `out/`. `git` is invoked
read-only (`log`, `blame`, `cat-file`, `merge-base`). The auto-memory path is
read-only from `/home/coreyt/.claude/projects/-home-coreyt-projects-fathomdb/memory`.

## What each miner computes

- **`mine_incode_markers.py`** — inventories every marker occurrence in `src/**`,
  classifies it, and **resolves it against its artifact-side registry** ->
  `resolved` / `dangling` / (for TC) `resolved-open` vs `resolved-terminal`.
  This is the **HEAD-state dangling rate**.
- **`mine_drift_blame.py`** — `git blame`s each marker line and its `±3` code
  lines; **drift** = surrounding code was last edited *after* the marker line
  (code moved on, marker did not). Reports a **lower bound** (a whitespace/format
  sweep touching the marker line resets its blame time and masks drift).
- **`mine_natural_refs.py`** — ages the non-code classes: memory wikilink
  dangling rate, commit-message ref resolvability (ledger seq / TC / F /
  Claude-Session), and doc `@sha` pin object-existence + reachability.

## Key caveats (carried into the report)

- **Drift ≠ wrongness.** Drift means the marker was not *re-validated* when
  adjacent code changed; it is the *risk surface* for false confidence (H5), not
  proof the marker is now false. Confirming falseness needs semantic reading and
  was not done at scale.
- **Young tree.** The 0.8.x source is recent (median marker age ~46d); drift is
  measured over a short horizon and would compound in a long-lived codebase.
- **H1 (does a missing/stale marker cost a reviewer?) is not directly measured.**
  The transcript corpus (`/home/coreyt/transcript-data`, ~1 GB) was deliberately
  **not** read; the counterfactual is under-powered here because the program
  already markers heavily. See the report's "smallest real trial" section.
