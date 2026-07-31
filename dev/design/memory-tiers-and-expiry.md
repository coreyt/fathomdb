---
status: PROPOSED
---

# Memory tiers and expiry — capture + hypothesis

**Status: HYPOTHESIS. Nothing here is implemented, ratified, or scheduled.** Part 1 is a verbatim
capture of a working-memory mechanism from another repo. Part 2 is my hypothesis about how it composes
with the memory this repo already has.

---

## Part 1 — the source mechanism, captured precisely

From `~/projects/local/unifi-openwrt`: `STEWARD-HANDOFF-NOTE.md` (66 lines) and its enforcing test
`test_steward_note.py`. Quoted, not paraphrased.

### 1.1 The note's own header

> **Covers** · Two things the record has no home for: **what is still running**, and **what bit us and
> is not a filed defect yet**.
>
> **Does NOT cover** · What just moved, or the *sequence* — those have owners (a tracker row, a witness
> cell, a sha), so a copy here is a duplicate and eventually a lie. ⚠️ **A pointer to in-flight or
> halted work is not a duplicate and belongs here** — that is what "still running" means. Point at the
> row; never restate its status.
>
> **Read when** · Starting cold, after `CLAUDE.md`. It does not tell you what to work on;
> `docs/0.2.0-plan.md`'s tracker does.
>
> **Every line here is temporary**, and is deleted the moment its fact reaches the record.
> **Target ≤ 50 lines; at > 80 the test suite fails** (`test_steward_note.py`) — the remedy is a prune
> with the operator, never a raised number. A predecessor, `HANDOFF.md`, died of exactly this and was
> retired in `c2432a7`.
>
> **Update ritual:** after every orchestrator or implementer returns, and before you hand off, add what
> a fresh steward would be *wrong* without, then delete every line that has since landed in the record.
> Date each line; cite a sha where one exists.

### 1.2 The four things the test actually enforces

`TARGET_LINES = 50`, `CEILING_LINES = 80`. The target is **deliberately not enforced** — "it is the
number you aim at, and leaving room between the target and the failure is what keeps the guard from
firing on an honest working note."

1. the note exists and is non-empty;
2. **`test_under_the_hard_ceiling`** — > 80 lines fails;
3. **`test_the_note_states_the_same_thresholds_this_test_enforces`** — the note's prose must contain
   the literal thresholds the test enforces. Doc and mechanism cannot drift apart;
4. **`test_it_does_not_become_a_second_board`** — asserts the *absence* of `| [` (a tracker row) and of
   `[ ]` / `[x]` checkboxes. On the checkbox: *"an UNCHECKED box is a plan, a CHECKED box is a
   done-state claim, and done-ness requires a witness in the tracker."*

### 1.3 Three lines from the test's docstring that carry the whole idea

- "A file whose whole value is being SHORT needs a mechanism, not an instruction, or it becomes the
  thing it was written to avoid."
- "The remedy is a prune with the operator, never a raised ceiling — a threshold that moves when it is
  inconvenient is not a threshold."
- "This paragraph lives here rather than in the note because it is durable, and the note's own rule is
  that durable text belongs in the record."

That last one is the mechanism applying itself: the *reasoning* is durable, so it lives in the test,
not in the working note. The note holds only what expires.

### 1.4 The limit the author states honestly, and does not paper over

> "⚠️ Length is a PROXY. The failure mode is duplication, and a 49-line note that is mostly duplicated
> passes every check below. … No mechanical check for duplication is attempted here; the honest
> position is that the update ritual carries that weight, and it is discipline."

Part 2 argues this repo can do better than discipline on exactly that point, because it has a single
writer.

### 1.5 How the mechanism is actually used, from the agent that uses it

Supplied by the operator; the agent's own account of applying the rule in one session:

- "Terseness isn't achieved by writing less about everything; it's achieved by the note refusing to
  hold anything that has an owner elsewhere."
- "I added a line because an orchestrator going in-flight is 'still running' and a fresh steward would
  be wrong without it, and in the same commit I pruned the README-vs-`CLAUDE.md` contradiction because
  `4d0af1c` had just put that fact into the record. Net 66 lines — over the 50 target, under the 80
  gate."
- "One caveat worth naming: 66 is above the stated target, and the correct response to that is a prune
  with the operator, not a quiet acceptance. The lines I judged not yet prunable are the ones whose
  facts genuinely haven't reached the record."

Three things this establishes that the artifact alone does not.

**The ceiling is a duplication detector, not a length budget.** That reframes §1.4's honest caveat. The
author says length is a proxy for duplication and no mechanical duplication check is attempted — but
the *ritual* is the check. Length is what duplication produces, so a note pruned by ownership stays
short as a consequence, never as a goal. Editing for brevity would be the wrong move and would leave
the duplicates in.

**Add and prune are one transaction.** The line went in and a line came out in the same commit,
because `4d0af1c` had just given that fact an owner. Not a periodic cleanup — the promotion of a fact
into the record *is* the deletion trigger, observed at the moment it happens. A batched prune would
mean carrying known-duplicated lines in the interim, which is exactly the window in which a stale line
gets read and believed.

**Exceeding the target is a signal to escalate, not to absorb.** 66 against a 50 target is over, and
the stated response is a prune with the operator rather than quiet acceptance. The gap between target
and ceiling is not slack to be spent; it is the region in which you are supposed to be talking to
someone. This repo has the opposite reflex — a defect gets named at the end of a report and carried,
which is how a 4,194-byte `steward-orient.sh` sat 98 bytes over its own cap, exiting 1, for long
enough that a cold-start agent tripped over it.

---

## Part 2 — hypothesis

### 2.1 The tier is set by the claim's TENSE

Not by the document, the directory, or the audience. By what kind of fact the sentence asserts:

| Claim tense | Example | Tier | Stays true? |
|---|---|---|---|
| **Past** | "Slice 39 landed at `91db34d8`" · "`seq-219` ruled fix-everything" | **deep** | forever |
| **Present** | "SCHEMA is 24" · "the live board is X" · "the remaining ladder is 40" | **middle** | until the world moves |
| **Imminent** | "worktree X is still running" · "Y bit us, not filed yet" | **working** | hours–days |

This predicts the failure data. **Every rot found in this repo during the 2026-07-30/31 audits was a
present-tense claim written by hand**: the plan §9 landed-pointer (stale three consecutive
commissions), the SCHEMA evidence cell, `release-state.json`'s `_comment`-is-`HELD` ruling, the
decision index missing 17 ADRs, `DOC-INDEX.md` at 223 rows against 877 files, `AGENTS.md` §11's
permission model, its 7-of-9 crate list. Not one deep-memory entry rotted. Not one *generated* view
rotted.

### 2.2 The mechanism is set by whether the claim is DERIVABLE

Tense says which tier. Derivability says what protects it.

- **Derivable present-tense claim → generate it.** This repo already has the machine:
  `release-state-<version>.json` as single writer, `generated_views`, `check-release-state-views.sh`
  hard-failing on drift. **6** views exist. **Zero have ever rotted.** The `plan-landed-roll-up` added
  this session replaced a pointer that had rotted three times, and its incremental phrasing —
  "landed *since* the Slice-20 narration" — is why it rotted by *omission*, the quietest failure: the
  list was never wrong, only short.
- **Underivable present-tense claim → give it an expiry.** Prose reasoning ("why the cap is tight",
  "why this fix shape is wrong") cannot be generated. It needs a `verified_at: <sha>` and a gate that
  fails when HEAD has moved too far without re-verification. That is the middle-memory analogue of the
  80-line ceiling: a mechanism, not an instruction.
- **Past-tense claim → append-only + id uniqueness.** Already in place: the JSONL ledgers with
  monotonic `seq`, ADR supersession, `lint-findings.sh`'s uniqueness check. Deep memory's failure mode
  is not staleness, it is **ambiguity** — two entries under one id — and that is already gated.

### 2.3 Middle memory does not expire by deletion. It expires by FREEZING

This is the part the source mechanism does not cover, because that repo's middle layer is one tracker.

A closed release's board is *still true* — as a record of that release. It must not be deleted, and it
must not stay on the live reading path. The expiry **event** is the release closing; the expiry
**action** is demotion from middle to deep.

The repo already has both predicates and has already used them once:

- `scripts/lib/board-closed.sh`'s `board_is_closed()` — 18 of 19 boards CLOSED;
- `status:` frontmatter on ladders — `ACTIVE` / `PROPOSED` / `COMPLETE` / `SUPERSEDED`.

Phase 1 of the cold-start budget work (`seq-226`) applied exactly this and cut a steward cold start
from ~374,000 to ~153,000 tokens without moving or rewriting a single file. **That is the middle-memory
expiry mechanism, already proven, applied once.** The hypothesis is that it generalises: everything in
`dev/plans/`, `dev/design/` and `dev/adr/` should carry a liveness predicate, and the live reading path
should be *computed* from those predicates rather than enumerated by hand.

So the three expiries are structurally different, and conflating them is the error:

| Tier | Expiry event | Expiry action | Enforcement |
|---|---|---|---|
| working | its fact reaches the record | **delete the line** | hard line ceiling |
| middle | the release closes / the plan completes | **freeze and demote** — never delete | liveness predicate + freshness stamp |
| deep | never | supersede in place | append-only + id uniqueness |

### 2.4 `steward-orient.sh` is the pattern, not an exception

Orient is a **projection**: a read-only derived view over middle memory, computed at read time, with a
byte budget and a hard-fail on any empty section. 955 tokens standing in for what the §3 reading list
would otherwise cost.

The hypothesis is that projections, not shorter documents, are the answer for middle memory. You do not
make the master terser; you compute the 40 lines of it that are live. The generated-view machinery and
orient are the same idea at two scales, and the second scale is barely used.

### 2.5 Where this repo can beat the source mechanism

The source names its own limit: length is a proxy for duplication, and no mechanical duplication check
is attempted — *"the update ritual carries that weight, and it is discipline."*

**This repo does not have to settle for discipline**, because it has a single writer. §1.5 reframes
what that check is FOR: the ritual prunes by ownership, and shortness is the by-product. So the
middle-memory equivalent is not a length gate on the boards — it is an ownership gate. A duplication
check is mechanically available: for any present-tense claim in a middle-memory document, ask whether
`release-state-<version>.json` already owns that fact. If it does, the prose is a duplicate and
eventually a lie — the source note's own words, now checkable rather than merely asserted.

That check does not exist. It is the strongest single thing this hypothesis suggests building, and it
is what would have caught the `_comment`-is-`HELD` ruling: a present-tense claim, inside the single
writer itself, contradicted by the board and the master and by `grep`.

### 2.6 What I am least sure of

- **Where prose reasoning lives.** The source puts durable reasoning in the *test*. That works for one
  note. This repo's boards carry a great deal of load-bearing reasoning — the `🕮 CORRECTED` blocks
  exist because a false claim was once acted on — and I do not know the right home for it. If it stays
  in middle memory it needs freshness stamps; if it moves to deep memory the boards get thin and the
  ledger gets fat. Untested either way.
- **Whether a freshness stamp is honest.** `verified_at: <sha>` is only as good as the verification.
  A stamp refreshed without re-reading is worse than no stamp, because it launders. This is the same
  hazard as a green gate nobody exercised, and I have no mechanism for it beyond the one the source
  admits to: discipline.
- **Whether a working-memory note is even needed here** given `steward-orient.sh` plus the dated
  hand-off series. §1.5 narrows this: the note's live value was a pointer to an in-flight
  orchestrator — which `orient` reports structurally (worktrees, dirty count, open PRs) but cannot
  annotate with *do not tidy this one, it holds unlanded work*. That annotation is the gap, and it
  is small enough that a note may be the wrong shape for it. The source repo's note covers "what is still running" and "what bit us and is not
  filed" — orient covers the first (worktrees, dirty count, open PRs) and the todos ledger covers the
  second. The gap may be smaller than it looks, or it may be exactly the 3,014 lines the dated series
  has accumulated across 17 files with no ceiling.
