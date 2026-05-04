# Whitepaper currency check — `performance-whitepaper-notes.md`

This prompt audits `dev/notes/performance-whitepaper-notes.md`
against the source-of-truth artifacts in `dev/plan/runs/`,
`dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` §12,
and the engine source. The goal: catch stale paragraphs,
misaligned numbers, removed code references, and obsolete
hypothesis ladders BEFORE the next packet starts using the
whitepaper as its evidence base.

Run this after Pack 5 close (which has happened — last commit
`46f693a` 2026-05-04) and before each new packet's first
implementer spawn.

## Model + effort

Sonnet 4.6, intent: medium. Main thread invokes directly OR
spawns via the resume §4 anti-chaining defenses. Pure read-and-
edit work; no code change, no test change.

```bash
PHASE=03-whitepaper-currency-check
TS=$(date -u +%Y%m%dT%H%M%SZ)
LOG=/home/coreyt/projects/fathomdb/dev/plan/runs/${PHASE}-${TS}.log
WT=/tmp/fdb-${PHASE}-${TS}
git -C /home/coreyt/projects/fathomdb worktree add "$WT" -b "${PHASE}-${TS}" 0.6.0-rewrite

PREAMBLE='YOU ARE THE IMPLEMENTER. Not the orchestrator. Do the
work yourself; do not spawn agents. Disallowed tools: Task, Agent.
This phase is doc-audit only — no src/ or tests/ changes.'

( cd "$WT" && \
  ( echo "$PREAMBLE"; cat /home/coreyt/projects/fathomdb/dev/plan/prompts/03-whitepaper-currency-check.md ) \
  | claude -p --model claude-sonnet-4-6 --effort medium \
      --add-dir "$WT" \
      --allowedTools Read Edit Write Bash Grep Glob \
      --disallowedTools Task Agent \
      --permission-mode bypassPermissions \
      --output-format stream-json --include-partial-messages --verbose \
  > "$LOG" 2>&1 )
```

## Log destination

- stdout/stderr: `dev/plan/runs/03-whitepaper-currency-check-<ts>.log`
- structured: `dev/plan/runs/03-whitepaper-currency-check-output.json`
- The whitepaper itself (`dev/notes/performance-whitepaper-notes.md`)
  is the artifact this phase mutates.

## Required reading

- **Read `AGENTS.md` §1** — "Stale > missing — keep evidence
  files current or delete them." This phase's mandate.
- **Read `MEMORY.md` + the `feedback_*.md` it indexes.** The
  reliability + release-verification principles bind doc work
  too: deprecation shims and stale evidence are first-class
  code paths; they need their own currency.
- `dev/notes/performance-whitepaper-notes.md` — the file to
  audit. Read every section.
- `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md`
  §12 — append-only experiment log; whitepaper §11 narrative
  must agree with it line-for-line on which experiments
  ran + their decisions + commit SHAs.
- `dev/plan/runs/STATUS.md` — current packet state.
- `dev/plan/runs/final-synthesis-output.json` — Pack 5 close.
- All `dev/plan/runs/*-output.json` for individual phases.
- `dev/plan/runs/*-review-*.md` for codex reviewer verdicts.
- `src/rust/crates/fathomdb-engine/src/lib.rs` — for line-number
  references in the whitepaper (e.g. `lib.rs:154` ReaderPool;
  `lib.rs:740` open_locked; `lib.rs:1824`
  register_sqlite_vec_extension; `lib.rs:1285` removed
  `_for_test` accessors after E.1 revert).
- `src/rust/crates/fathomdb-engine/Cargo.toml` — confirm no
  `LIBSQLITE3_FLAGS` or `crossbeam-queue` deps exist if Pack 6
  hasn't landed yet.

## Mandate

For each numbered section of the whitepaper, perform these
five checks in order. Open issues are recorded in the output
JSON; fixes are applied directly to the whitepaper.

### Check 1 — Number agreement

Every numeric claim in the whitepaper §4 / §5 / §10 / §11 must
match the corresponding output JSON. Spot-check at least:

- A.1 baseline (seq=182, conc=115, speedup=1.58, n=5) sourced
  from `dev/plan/runs/A1-perf-capture-output.json`.
- A.2 classification table — share percentages match
  `dev/plan/runs/A2-symbol-focus-output.json`.
- B.1 #2 numbers (seq median 184, conc median 120.6) match
  `dev/plan/runs/B1-multithread-wiring-output.json`.
- C.1 numbers (conc 121.5 ms, speedup 1.509×) match
  `dev/plan/runs/C1-threadsafe2-rebuild-output.json`.
- E.1 numbers (seq 157, conc 125, speedup 1.266×) match the
  values archived in commit `91c69e9`'s tree (E.1 output JSON
  was reverted; the figures live in §5/§11 narrative + the
  reviewer log at `dev/plan/runs/E1-review-20260504T031424Z.md`).
- Final synthesis numbers (seq 184.7, conc 124.0, speedup
  1.487×) match `dev/plan/runs/final-synthesis-output.json`.

If any number drifts, fix the whitepaper to match the JSON.
The JSON is canonical (it was written by the implementer at
the time of the experiment); the whitepaper is narrative.

### Check 2 — Commit SHA agreement

Every commit SHA referenced in the whitepaper must exist in
`git log` and must match the experiment described. Run:

```bash
for sha in $(grep -oE '\b[0-9a-f]{7}\b' dev/notes/performance-whitepaper-notes.md | sort -u); do
  git log --oneline | grep -q "^$sha" || echo "STALE: $sha"
done
```

Any STALE shas: replace with the correct sha or remove the
reference (with a noted reason in the same paragraph).

### Check 3 — File-line reference agreement

Every `<file>:<line>` reference (e.g. `lib.rs:154`,
`lib.rs:1824`, `sqlite3.h:249-252`,
`libsqlite3-sys-0.30.1/build.rs:136`) must point at the symbol
the whitepaper claims it points at. Run:

```bash
for ref in $(grep -oE '[a-zA-Z0-9_/.-]+\.(rs|h|toml|md):[0-9]+' dev/notes/performance-whitepaper-notes.md | sort -u); do
  file="${ref%:*}"; line="${ref##*:}"
  test -f "$file" || { echo "MISSING FILE: $ref"; continue; }
  # Print the line and immediate context for human eyeball later
  awk -v l="$line" 'NR==l-1||NR==l||NR==l+1{print FILENAME":"NR": "$0}' "$file"
done
```

Any MISSING FILE or relocated symbol: update the line number
or remove the reference.

### Check 4 — Hypothesis-ladder currency

Whitepaper §6 ("Hypothesis hierarchy for the remaining gap")
predates Pack 5. Pack 5 falsified the §6 primary suspect
(SQLite global allocator mutex). §11 contains the corrected
hypothesis ladder.

This audit must reconcile §6 with §11 in one of two ways:

- (A) Append a 2026-05-04 update to §6 that points readers to
  §11's revised hierarchy, leaving the original §6 text intact
  as historical record. Use this if Pack 6 hasn't run yet.
- (B) After Pack 6 closes (KEEP or REVERT), rewrite §6 in place
  with the post-Pack-6 hierarchy, citing §11 paragraphs as
  evidence. Use this only if the human explicitly authorizes
  a §6 rewrite (it's a non-trivial editorial change).

Default: use (A).

### Check 5 — Open-question currency

§8 ("Open questions for the whitepaper") was updated at Pack 5
close with q1=ANSWERED, q2=STILL-OPEN-with-evidence, q3=MOOT,
q4=NEW. Confirm:

- q1's answer paragraph still matches B.1 / C.1 / E.1 evidence
  in §5/§11.
- q2's "new evidence" line names the rusqlite-side / ReaderPool
  Mutex / WAL atomics candidates. If Pack 6 has run and
  classified, update q2 with the Pack 6 verdict.
- q3 stays MOOT until/unless a new packet revisits THREADSAFE
  for non-AC-020 reasons.
- q4's text matches the §11 closing-paragraph hypothesis triple.

If a Pack 6 packet has run after this audit was last run,
add new questions for any new open issue surfaced.

### Check 6 — Section drift / orphaning

Look for:

- §7.7 (SWMR + per-reader `OPEN_NOMUTEX` stack) — its trigger
  ("B.1 KEEP/INCONCLUSIVE") never fired. Mark the section
  "trigger never fired; archived as conditional plan record"
  if Pack 5 is the most recent packet.
- §7.8 (E synthesis track) — activation table was used (C.1
  REVERT → E.1 ran → E.1 REVERT → rest skipped). Reference
  the §11 narrative paragraphs that closed each branch.
- §4 (kept) — should still be empty for Pack 5 work; no KEPT
  production-code experiments. If a future packet adds KEPT
  entries, ensure they include hypothesis + before/after
  numbers + reviewer link + commit SHA per the resume §3
  decision-loop step 6.
- §5 (reverted) — Pack 5 added 3 entries (B.1, C.1, E.1).
  Verify each entry has hypothesis + why-it-didn't-work +
  do-not-retry rationale per resume §3 decision-loop step 7.

## Acceptance criteria

- Every numeric claim in the whitepaper matches the source JSON
  or the source code.
- Every commit SHA resolves in `git log`.
- Every `file:line` reference exists at the named line (±1
  line of drift acceptable; >1 means update or remove).
- §6 hypothesis ladder either appended-with-pointer-to-§11
  (option A) or rewritten with explicit human authorization
  (option B).
- §8 open questions match the Pack 5 close + any newer evidence.
- §7.7 / §7.8 conditional sections reflect actual trigger /
  activation outcomes.
- Output JSON written to
  `dev/plan/runs/03-whitepaper-currency-check-output.json`
  enumerating: stale numbers (with file:line + before/after),
  stale shas, missing/relocated file refs, sections with no
  drift, sections fixed in place.

## Files allowed to touch

- `dev/notes/performance-whitepaper-notes.md` (the audit target).
- `dev/plan/runs/03-whitepaper-currency-check-output.json` and
  `.log`.

## Files NOT to touch

- All `src/`, all `tests/` (audit is doc-only).
- Other prompt files (their work is complete; if a prompt
  references a stale whitepaper paragraph, file an issue,
  don't fix the prompt here).
- `STATUS.md`, plan §12, progress board (out of scope; their
  currency is the orchestrator's job at packet-close, not
  this phase's).
- Cargo.toml / build files.

## Verification commands

```bash
# 1. Number agreement spot-check
jq -r '
  "\(input_filename | sub(".*/"; "")): seq=\(.before.sequential_ms // .ac020_summary_a1_baseline.sequential_ms.median // "n/a"), conc=\(.before.concurrent_ms // .ac020_summary_a1_baseline.concurrent_ms.median // "n/a")"
' dev/plan/runs/A1-perf-capture-output.json \
  dev/plan/runs/B1-multithread-wiring-output.json \
  dev/plan/runs/C1-threadsafe2-rebuild-output.json \
  dev/plan/runs/final-synthesis-output.json \
  2>/dev/null

# 2. Commit SHA agreement
for sha in $(grep -oE '\b[0-9a-f]{7}\b' dev/notes/performance-whitepaper-notes.md | sort -u); do
  git log --oneline | grep -q "^$sha" || echo "STALE: $sha"
done

# 3. File-line reference agreement
for ref in $(grep -oE '[a-zA-Z0-9_./-]+\.(rs|h|toml|md):[0-9]+' dev/notes/performance-whitepaper-notes.md | sort -u); do
  file="${ref%:*}"
  test -f "$file" || echo "MISSING FILE: $ref"
done

# 4. agent-verify (final sanity)
./scripts/agent-verify.sh
```

## Required output to orchestrator

`dev/plan/runs/03-whitepaper-currency-check-output.json`:

```json
{
  "phase": "03-whitepaper-currency-check",
  "decision": "CLEAN|FIXED_IN_PLACE|ESCALATE",
  "decision_summary": "<one line>",
  "checks": {
    "number_agreement": {
      "issues_found": <n>,
      "issues_fixed": <n>,
      "issues_escalated": ["<text>", ...]
    },
    "commit_sha_agreement": {
      "stale_shas_found": ["<sha>", ...],
      "stale_shas_fixed": <n>
    },
    "file_line_agreement": {
      "missing_files": ["<ref>", ...],
      "relocated_symbols": [{"ref": "<ref>", "now_at": "<file:line>"}, ...]
    },
    "hypothesis_ladder_currency": {
      "option_chosen": "A|B",
      "rationale": "<text>"
    },
    "open_questions_currency": {
      "questions_updated": <n>,
      "new_questions_added": <n>
    },
    "section_drift": {
      "sections_marked_archived": ["<section ref>", ...],
      "sections_in_sync": ["<section ref>", ...]
    }
  },
  "files_changed": ["dev/notes/performance-whitepaper-notes.md"],
  "loc_added": <n>,
  "loc_removed": <n>,
  "loc_net_negative": true|false,
  "next_audit_due": "<after which packet's close>",
  "unexpected_observations": "<free text>"
}
```

## When to invoke this audit

- **After every Pack-N close** — before Pack-(N+1)'s first
  implementer spawn reads the whitepaper as evidence.
- **Before promoting any whitepaper paragraph** to a
  publication-quality artifact (paper, blog, ADR).
- **When a new contributor reports** that a whitepaper claim
  doesn't match what they see in the code or the runs.
- **On any AGENTS.md §1 "Stale > missing" smell-test failure**
  during a regular review.

## Update log

- 2026-05-04 — Initial handoff written. The whitepaper is
  current as of commit `46f693a` (Pack 5 final synthesis); this
  prompt is the on-demand audit harness for keeping it that
  way. First scheduled invocation: before Pack 6 spawns its
  first implementer.
