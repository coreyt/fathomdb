# `dev/steward/` — the Steward ledger discipline

This directory holds the **Steward's append-only decision ledger** and the
discipline for keeping it. The Program Steward (see
`dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`) is the program-scope keeper who
reconciles the schedule-of-record against git, places cross-cutting work, and
commissions release orchestrators. The ledger is how the Steward's decision trail
survives a context reset or a hand-off **without** re-reading whole files.

## Why a ledger (context is the scarce resource)

The Steward's job is judgment, and judgment burns context. Re-reading a growing
decision log on every session is O(file); it crowds out the reasoning the tokens
are actually for. The ledger fixes this by making reads **O(delta)**: you append
one structured line per decision, and you read only what changed since your last
cursor. State lives on disk (`orchestration.md` §12.1 — "if it must survive a
`/compact`, it goes on disk"), not in chat.

## The two tools (never hand-edit the ledger)

- **`dev/agent-tools/ledgerwrite/ledgerwrite.py` — append.** One structured JSONL
  record per call, with a monotonic `seq`. Never open the ledger in an editor and
  never hand-append; a malformed line breaks the delta reader for everyone.

  **`--summary` and `--body` MUST be single-quoted.** This is a hard rule, not a
  preference — see [Single-quote the summary](#single-quote-the-summary-tc-53)
  below for the two entries it has already cost.

  ```bash
  # RIGHT — single quotes. The shell hands the text through untouched.
  python3 dev/agent-tools/ledgerwrite/ledgerwrite.py dev/steward/steward-ledger.jsonl \
    --kind decision \
    --summary 'reconciled slice X into master §4; `design_refs` landed at d30ef52f' \
    --ref git:<sha> --ref plan:dev/plans/plan-0.8.z.md \
    --field decider=steward
  ```

  ```bash
  # WRONG — double quotes. The shell rewrites the text BEFORE ledgerwrite runs:
  #   `design_refs` is executed as a command and its (empty) output substituted,
  #   so the word and both backticks vanish and nothing warns.
  --summary "reconciled slice X; `design_refs` landed at d30ef52f"
  ```

- **`dev/agent-tools/ledgerwatch/ledgerwatch.py` — read deltas.** Reads only the
  entries appended since your saved cursor, then advances the cursor. This is the
  read you do at the top of a session — not a whole-file re-read.

  **Always pass an explicit `--state-dir`, and run from the repo root.** The
  default is *cwd-relative* (`$LEDGERWATCH_STATE`, else `./.ledgerwatch`), so a
  run from a worktree — or from any directory but the root — silently picks up a
  *different*, empty cursor, and the "delta" degrades into a whole-file re-read.
  A fixed repo-relative state dir makes it the same cursor every session.

  ```bash
  # From the repo root. This run ADVANCES the cursor.
  python3 dev/agent-tools/ledgerwatch/ledgerwatch.py \
    dev/steward/steward-ledger.jsonl --state-dir dev/steward/.ledgerwatch
  # --reset to re-read from the top; --validate for a whole-file JSONL
  # integrity scan.
  ```

  **`--dry-run` is the peek mode only: it does *not* advance the cursor** (the
  tool skips its state save entirely under that flag). It is never the normal
  read — a session that only ever peeks re-reads the whole ledger every time.

  This repo has **three** ledgers. Same form for each; give each its own fixed
  state dir (all three paths below are already git-ignored):

  | ledger                                                         | `--state-dir`                    |
  |----------------------------------------------------------------|----------------------------------|
  | `dev/steward/steward-ledger.jsonl`                             | `dev/steward/.ledgerwatch`       |
  | `dev/todos-and-considerations-ledger.jsonl`                    | `dev/.ledgerwatch-todos`         |
  | `dev/design/record-lifecycle-protocol/OPP-12-sub-ledger.jsonl` | `dev/steward/.ledgerwatch-opp12` |

Each tool has a full `README.md` next to it in `dev/agent-tools/`.

## Discipline

- **Append, never hand-edit.** All writes go through `ledgerwrite`; all reads of
  "what's new" go through `ledgerwatch`. This is the same rule the `steward` agent
  def encodes.
- **Record the decider.** Every entry says whether the *steward* or the *HITL*
  decided it (`--field decider=…`) — matches the STEWARD-HANDOFF §6 "name the
  decider" rule.
- **Trust git, not narration.** The ledger records what you verified from git; it
  is a decision trail, not a source of truth about the repo. When they disagree,
  git wins.

### Single-quote the summary (TC-53)

**The rule: `--summary` and `--body` are always in SINGLE quotes.** Double quotes
let the shell rewrite the text before `ledgerwrite` is even executed.

**It has already happened three times, all measured, all by the Steward:**

| entry | what the shell did | what survived |
|-------|--------------------|---------------|
| `seq-95/96` | a backtick substitution ran | one word silently dropped |
| `seq-108/109` | `$?` expanded to `0`; a `$(…)` expanded to empty | a wrong exit code, a gap, and `basename: missing operand` on stderr |
| quoted in `seq-147` | ``` `design_refs` ``` was executed | `"An optional  array on a ladder entry"` — the word and both backticks gone, only a doubled space left |

Each was caught only because a human re-read the written record. A mangled
summary is otherwise **indistinguishable from an intended one**.

**The tool cannot save you, and it does not pretend to.** The shell expands
first; `ledgerwrite` receives only what is left. It sees *residue*, never
*intent* — so it refuses a torn `$(` skeleton and control characters, and it
*warns* (never blocks) on surviving `$`/backticks and on the doubled space an
eaten word leaves. **None of the three incidents above is detectable in-tool.**
The refusal set is calibrated against every entry already in the three ledgers,
so it will not block a legitimate write; that is deliberate, and it is also the
reason the discipline above is the real fix.

**The cost of getting it wrong is permanent.** The ledger is append-only. A
mangled entry can only be **annotated by a follow-up entry** — never repaired in
place — so the corruption and its correction both stay in the record forever.

**If your prose genuinely needs a literal `$` or a backtick**, single quotes are
all you need — this arrives verbatim:

```bash
--summary 'costs ~$1.77 and extends the shipped `drain`'
```

A single-quoted string cannot itself contain a single quote: close, escape,
reopen — `'…isn'\''t…'` — or pass the text through a shell variable. If the
guard still refuses text you are certain of, re-run with
`--accept-shell-residue`; it writes the record and names the waived class on
stderr so the choice is auditable.

## Files here

- `steward-ledger.jsonl` — the append-only decision ledger (write via
  `ledgerwrite`, read via `ledgerwatch`).
- `tooling-port-plan.md` — plan of record for this tooling port.
- `tooling-port-reconciliation.md` — the convergence map (each new tooling file →
  the FathomDB doc/rule it encodes), proving this is not a parallel system.
