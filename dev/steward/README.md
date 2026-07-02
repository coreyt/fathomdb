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

  ```bash
  python3 dev/agent-tools/ledgerwrite/ledgerwrite.py dev/steward/steward-ledger.jsonl \
    --kind decision \
    --summary "reconciled slice X into master §4" \
    --ref git:<sha> --ref plan:dev/plans/plan-0.8.z.md \
    --field decider=steward
  ```

- **`dev/agent-tools/ledgerwatch/ledgerwatch.py` — read deltas.** Reads only the
  entries appended since your saved cursor, then advances the cursor. This is the
  read you do at the top of a session — not a whole-file re-read.

  ```bash
  python3 dev/agent-tools/ledgerwatch/ledgerwatch.py dev/steward/steward-ledger.jsonl
  # --dry-run to peek without advancing; --reset to re-read from the top;
  # --validate for a whole-file JSONL integrity scan.
  ```

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

## Files here

- `steward-ledger.jsonl` — the append-only decision ledger (write via
  `ledgerwrite`, read via `ledgerwatch`).
- `tooling-port-plan.md` — plan of record for this tooling port.
- `tooling-port-reconciliation.md` — the convergence map (each new tooling file →
  the FathomDB doc/rule it encodes), proving this is not a parallel system.
