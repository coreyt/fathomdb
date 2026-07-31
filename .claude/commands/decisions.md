---
description: Enumerate the open HITL decisions for the live FathomDB release — situation, options, recommendation
argument-hint: [optional scope — e.g. "before we land Slice 20" or "just the publish gate"]
---

# Open HITL decisions

Enumerate the decisions that are genuinely mine (coreyt, the HITL) to make on the
live release line. Work from the repo, the release state file, the steward ledger,
and anything in flight.

## Start from the machine-readable open set — do NOT assemble one by hand

`dev/plans/release-state-0.8.20.json` is the **single-writer** state file for the
current release. Its `decisions.unruled` and `decisions.ruled` arrays are the
answer to "what is actually open", and every entry carries a `source`. Read it
first; treat it as the spine of your report and the prose docs as its evidence.

Cross-check with the ledger projection, which is read-only and advances no cursor:

```
python3 dev/agent-tools/ledgerwatch/ledgerwatch.py --project rulings dev/steward/steward-ledger.jsonl
```

Two caveats it will not tell you itself. It keys by `seq-N` (the steward ledger
carries no ids and must not be made to), and it classifies as *ruled* only on
`kind == "decision"` plus the literal status `open` — so it currently reports
`unruled: 0` with a large `unclassified` bucket. **That is a decision *record*,
not the open set.** The open set is `release-state-0.8.20.json`.

Then read, in this order, for the framing behind each item:

- `dev/plans/plan-0.8.20.md` §11 — the live open HITL decision queue, including
  the ✅ **HITL RULINGS 2026-07-25** block that closed several of its own items.
- `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` §6 — the master. Findings
  **F-1 … F-34** are the program's decision record; **F-34** is the current run
  authorization.
- `dev/plans/runs/STATUS-0.8.20.md` §4 — ⚠ **bannered as a HISTORICAL queue.** Its
  rows 1–7 are retained as decision record and are explicitly *not* open; the
  banner points at the generated live count instead. Never lift a row out of §4
  and present it as an open decision.

## Then harvest the SESSION — the repo does not know about these

The sources above only know what has been written down. **A question raised in
this session and never recorded is invisible to every one of them**, and it is
usually the one actually blocking me. Sweep the conversation for:

- anything an orchestrator or subagent **escalated** and you triaged but did not
  yet place (these arrive as "for you / your call, not mine" in a hand-back);
- anything you deferred with "I'll flag this" or "worth your attention" and no
  durable id was minted;
- assumptions you stated and acted on that I never actually confirmed — say so
  plainly, and give me the chance to overturn them while it is still cheap;
- work you scoped or re-sequenced yourself that arguably crossed into the §5
  **always-HITL** row.

Mark each one **`[session]`** so I can see at a glance which items have no repo
home yet. If I close a `[session]` item, it gets a durable id at close time (see
the closing section) — that is the moment it stops being transcript-only.

## What qualifies

The authority is `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md` **§5** — the
decision-rights table. Do not invent a different test.

- **Observe / verify**, **analyze / triage**, and **propose** are **autonomous,
  always**. Never bring me one of these. Proposing ≠ applying.
- **Mutate the repo / external state** is HITL-gated *by default* but **autonomous
  under a standing mandate** for a defined unit of work. If a live mandate already
  covers it, it is not a decision — act, and say so in one line.
- **Change program direction or the record** — a release slot, moving an item
  between releases, altering an I-edge, re-sequencing — is **ALWAYS explicit
  HITL**, never self-widened and **never inside an implied mandate**.

The mandate rule is the heart of it (§5): act *within* the authorized unit of
work, never expand its boundary yourself. When in doubt whether something is
in-mandate, treat it as out and ask.

## Ruled decisions are CITED, never re-opened

Before you list anything, check it against `decisions.ruled` in
`release-state-0.8.20.json`. If it is there, write **"already ruled at
`<source>`"** and move on — do not re-surface it, do not schedule a confirming
check, do not ask me to reaffirm it. Closing a decision and then scheduling a
confirmation is a failure mode this repo has already ruled against.

Currently ruled and explicitly **not open**:

- **AC-079 governed-surface sign-off — PRE-SIGNED** (master **F-34**;
  `plan-0.8.20.md` §11 item 1 + its 2026-07-25 rulings block). Pinned to the
  content of `src/conformance/governed-surface-allowlist.json`; any diff re-opens
  the gate, enforced mechanically by the DOC-HYGIENE-2 T1e pin gate. Minting still
  happens at Slice 40 — that is not a re-open.
- **`plan-0.8.20.md` §7 prerequisite 5 — Memex co-land readiness — CLOSED BY
  DECISION** (master F-34). Memex adapts to 0.8.20's surface and **no confirmation
  is to be sought**. Do not re-introduce a "verify with Memex" step. Push scope
  stays fathomdb-only.
- **`plan-0.8.20.md` §11 item 8 — the Hermes consult — CLOSED**, no input
  received; it gated nothing.

Also settled per §11's own preamble: Finding-1 = (A), TC-46/TC-47 RESOLVED,
TC-11 + TC-32 CLOSED, and (master F-28) **R-20-EU7 closed by decision — zero eu7
runs on any backend at any N**.

## Do not pad the list

**Zero open decisions is a valid and useful answer** — say so plainly rather than
manufacturing one to fill the template.

Here a padded list is *actively harmful*. This program has a documented failure of
**re-deciding settled calls because four documents disagreed** — the very fan-out
`STATUS-0.8.20.md` §4 flags in its own banner ("a hand-maintained duplicate of
state that lives in three other files"). Every item you surface that is already
ruled costs me the decision again. Rank by what is blocking the most, not by what
is easiest to describe.

## Every load-bearing claim needs a witness

`file:line`, a sha, or a command **and its real exit code**. A decision framed on
a premise you did not check is worse than no decision.

**Capture exit codes immediately.** Put `rc=$?` on the very next line, before any
command substitution:

```bash
bash scripts/tests/test_actionlint_fixture.sh; rc=$?   # correct
printf '%s %s\n' "$(basename "$t")" "$rc"              # only AFTER rc is captured
```

A loop that interpolates `$?` *after* a `$(basename …)` reports **basename's**
status, not the test's. That exact mistake invalidated a verification in this repo
this week — `dev/steward/steward-ledger.jsonl` seq-108, corrected at seq-109, and
it was the *second* occurrence of the class (seq-95/96 was the first).

**`scripts/agent-test.sh`'s aggregate exit code is a VACUOUS signal.** It runs
under `set -euo pipefail` (line 3) and aborts at line 63 on a known-red stale
assertion in `scripts/tests/test_actionlint_fixture.sh` — **TC-16 / F-30**, red
since 0.8.14, pre-existing, already HITL-placed at Slice 40
(`dev/plans/runs/STATUS-0.8.20.md:391`). It therefore **never reaches the Rust
(line 147) or Python (line 170) steps**, and its exit code says nothing about
them. Run the suites individually and quote each one's own exit code.

## Per decision

- **Situation** — what is true now, in 2–4 sentences, each load-bearing claim
  witnessed as above.
- **The question** — one sentence, answerable as posed.
- **Options** — 2–4, each materially different, each with its cost and what it
  forecloses. If one is "do nothing" or "defer", say what that costs too. Options
  that differ only in wording are one option.
- **Recommendation** — one option, and *why this one over the runner-up
  specifically*. Not a summary of its merits.
- **What would change it** — the fact that would flip your recommendation, and
  whether it is cheaply checkable. If nothing would change it, this was probably
  not a decision for me.
- **Blocked / reversible** — what stops until I answer, and whether anything
  irreversible waits behind the gate. Say plainly whether the item **halts the
  run** (`halts_run` in the state file) or is merely scheduled at a boundary.

## Then stop — enumeration ends here

Do not begin work on any of it — not the analysis it implies, not the "quick
check" that would settle it. If some decisions are independent and others depend
on an earlier answer, say which, so I can answer out of order.

---

## Closing a decision — the second half of this command

When I answer, **record it.** A decision answered only in chat does not exist:
the transcript is not a durable home, and losing it is how the same call gets
re-litigated a week later. Closing is three writes, in this order.

**1. The ledger — the append-only trail, and the authority.**

```bash
python3 dev/agent-tools/ledgerwrite/ledgerwrite.py dev/steward/steward-ledger.jsonl \
  --kind decision --field decider=hitl \
  --summary '<the ruling, in MY words, and what it forecloses>' \
  --ref plan:dev/plans/plan-0.8.20.md
```

**Single-quote the summary.** Shell substitution has silently corrupted this
ledger **twice** — seq-95/96 and seq-108/109 (`TC-53`). A mangled entry is
indistinguishable from an intended one, and the ledger is append-only, so it can
only ever be corrected by a follow-up entry, never repaired.

**2. The state file — the single writer.** Move the entry from
`decisions.unruled` to `decisions.ruled` in `dev/plans/release-state-0.8.20.json`,
with `source` set to the new `seq-N`.

**3. Regenerate and verify.** `scripts/check-release-state-views.sh` must exit 0.

⚠ **Know exactly what is and is not generated.** The §4 live-open **count**, the
master's ladder-progress row, the board's `Unblocks` cell, the plan's
immediate-next pointer and the hand-off next-step **do** render from the state
file — **never hand-edit inside a `GENERATED` marker.**

⛔ **The board's §1 cell does NOT render from anything.** Neither does its §4
enumeration, the session hand-off's §4, or master F-34's prose. **Closing a
decision therefore does NOT update every view, and
`check-release-state-views.sh` exiting 0 does NOT mean the documents agree** —
the fence covers the *count*, not the *list*.

This file asserted the opposite until 2026-07-31 ("no two documents can disagree
about what is open"), and that false claim is why nobody re-checked when
`axis-e-version` was registered at `c73c367a`: the generated numeral followed
TWO → THREE automatically while **four** hand-written copies did not, and the
check stayed rc=0 the whole time. **Read the open set out of
`dev/plans/release-state-<version>.json` and correct every prose copy by hand**
until the list itself is fenced — owed immediately after the 0.8.20 publish
(master F-34).

**For a session question with no repo home yet** — something raised in
conversation that was never written down — mint a durable id *first*, then close
it the same way:

```bash
python3 dev/agent-tools/ledgerwrite/ledgerwrite.py dev/todos-and-considerations-ledger.jsonl \
  --kind todo --field decider=hitl --field id=TC-<n> --field status=open --summary '<...>'
```

Check the id is unused before minting it — `max(TC-n) + 1` is not safe on its own
(AC ids already collided once this way, master **F-29**).

### What closing must never do

- **Never close a decision by making it yourself.** `decider=hitl` means the
  human answered. If I did not answer it, it stays open — an agent-supplied
  answer recorded as mine is the worst possible outcome of this command.
- **Record my answer, not your reading of it.** If I pick the option you did not
  recommend, record that plainly, with no editorial and no re-argument.
- **Cite, never restate.** Prose homes reference `seq-N`; they do not paraphrase
  the ruling. Restating is precisely what put one fact in four documents and let
  them drift apart.
- **Never schedule a confirming check** for something just closed. Closing a
  decision and then pricing a run to confirm it is a failure mode this program
  has already ruled against.
- A close that changes program direction or the record is the §5 **always-HITL**
  row. That is satisfied *only* because I am the one answering — so attribute
  honestly, every time.

### Report back after closing

One line per decision: what was decided, the `seq-N` it landed at, and what is
now unblocked. Then re-run the enumeration if anything remains, so the open set
is current before you resume work.

$ARGUMENTS
