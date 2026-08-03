# Todos & Considerations Ledger — protocol

> A durable, append-only, **agent-readable** ledger for the cross-cutting items that
> otherwise die in chat: **todos, considerations, caveats, observations, and open
> questions** that don't belong to any single plan, slice, or review — but that a
> future session (human or agent) must not lose.
>
> **Ledger file:** `dev/todos-and-considerations-ledger.jsonl` (one JSON record per line).
> **This repo's id prefix:** `TC`. New items use tool-allocated `TC-<uuid>` identities;
> historical `TC-N` identities remain valid.
>
> It is **generic and portable** — the same two files (this README + the JSONL) drop
> into any repo that has the `ledgerwrite`/`ledgerwatch` tools. See [Porting](#porting-to-another-repo).

---

## 1. What it is (and is not)

It is **event-sourced**: the ledger is **append-only** and **never hand-edited**. You do
not open the file and change a line. Instead, every fact — opening an item, advancing
its status, resolving it — is a **new appended record**. An item's *current* state is
**derived** by folding all records that share its `id` (the newest one wins). This is the
same discipline the steward and enum-discussion ledgers use, enforced by the tools:

- **Write** only with **`ledgerwrite --profile todos`** (stamps `ts` + a monotonic
  `seq`; folds this ledger under its append lock for identity/update validation).
  Never `echo >>`, never an editor.
- **Read** only with **`ledgerwatch`** (delta or filtered reads; O(delta), not O(file)).
- **Never** open the `.jsonl` by hand. A stray editor save can tear a line; `--validate`
  is the integrity check.

**Use this ledger for** cross-cutting items with no natural home:

- a **caveat** a future change must respect ("X future-couples to Y only if it does Z");
- a **consideration** to weigh at a decision point ("prefer A over B because …, revisit when …");
- an **observation** worth durable capture (a measured fact, a gotcha, a root cause);
- a **todo** that spans initiatives or has no owning plan yet;
- an **open question** whose answer will steer later work.

**Do NOT use it for** (each has a better home):

- a specific initiative's slice ladder → the plan (`dev/plans/<x.y.z>-plan.md`).
- the steward's own decision/drift/reconcile trail → `dev/steward/steward-ledger.jsonl`.
- a live cross-repo negotiation → its own discussion ledger (e.g. `enum-discussion-ledger.jsonl`).
- a code-review finding on a diff → the review verdict under `dev/reviews/`.

A good test: *"is this a durable cross-cutting thing to remember/act-on later, that would
otherwise be lost?"* → here. *"is this a step in one plan, or a record of what I just did?"*
→ its plan/steward ledger.

---

## 2. Record shape

Two fields are stamped by the tool and two are universal; the rest are this ledger's
convention (all values are strings — `ledgerwrite` stringifies everything).

```jsonc
{ "ts": "2026-07-03T17:02:11.481Z",   // tool-stamped (UTC, ms, Z)
  "seq": 3,                            // tool-stamped monotonic entry number
  "kind": "caveat",                    // REQUIRED — the item's NATURE (immutable per id)
  "summary": "Commission B future-couples to OPP-12 only if it goes live-pipeline",
  "id": "TC-1",                        // REQUIRED — stable item handle (join key)
  "status": "watching",               // REQUIRED — lifecycle state AS OF this entry
  "priority": "p2",                    // optional
  "owner": "pas",                      // optional
  "area": "eval/crosssource",          // optional — subsystem / path
  "blocked-by": "steward:env-decision",// optional — a dependency (id or external ref)
  "epistemic": "verified",             // optional — verified | proposed | assumed
  "refs": ["git:1a73717", "opp:OPP-12", "file:eval/crosssource/linker.py"],
  "body": "…full prose: the caveat, the options, the rationale…" }
```

### 2.1 The two axes — `kind` (nature) and `status` (lifecycle)

`kind` answers **"what kind of thing is this?"** and is **immutable** across an item's life
(every entry sharing an `id` uses the same `kind`). `status` answers **"where is it now?"**
and **changes** entry to entry. Keep them orthogonal.

**`kind` vocabulary (the item's nature):**

| `kind` | meaning |
|---|---|
| `todo` | actionable work to be done |
| `consideration` | a design/sequencing/trade-off judgment to weigh at a decision point |
| `caveat` | a constraint / gotcha / "must respect" that a future change could trip over |
| `observation` | a durable fact worth keeping (a measurement, a root cause, an inventory) |
| `question` | an open unknown whose answer steers later work |

**`status` vocabulary (the lifecycle; the fold picks the latest):**

| `status` | meaning | terminal? |
|---|---|---|
| `open` | captured, not yet started / unresolved | no |
| `in-progress` | actively being worked | no |
| `blocked` | can't proceed; see `blocked-by` | no |
| `watching` | passively tracked; no action pending, but re-check on a named trigger | no |
| `done` | completed / resolved | **yes** |
| `wont-do` | consciously declined (keep the record + reason so it isn't re-litigated) | **yes** |
| `superseded` | replaced by another item; see `supersedes`/`refs` | **yes** |

An item is **live** iff its latest entry's `status` is non-terminal.

The profile permits normal forward movement among active states, permits a
terminal resolution from an active state, and refuses transitions out of a
terminal state or a return from `in-progress` to `open`.

### 2.2 Field reference

| field | req? | set by | notes |
|---|---|---|---|
| `ts`, `seq` | — | tool | stamped by `ledgerwrite`; never pass them |
| `kind` | ✅ | `--kind` | the nature (table above); immutable per `id` |
| `summary` | ✅ | `--summary` | one line; for an update, describe the *change* |
| `id` | ✅ | `--open` or `--field id=` | new opens receive `TC-<uuid>`; legacy `TC-N` updates remain valid; **same across the item's whole life** |
| `status` | ✅ | `--field status=` | lifecycle as of this entry |
| `priority` | ◻ | `--field priority=` | `p0`..`p3` |
| `owner` | ◻ | `--field owner=` | `pas` / `hitl` / `orchestrator` / a repo name / a person |
| `area` | ◻ | `--field area=` | subsystem or path the item lives in |
| `blocked-by` | ◻ | `--field blocked-by=` | an `id` or external ref this waits on |
| `blocks` | ◻ | `--field blocks=` | an `id` this is a prerequisite of |
| `supersedes` | ◻ | `--field supersedes=` | an `id` this entry replaces (pair with `status=superseded` on the old one) |
| `decider` | ◻ | `--field decider=` | when a status change is a HITL/authority call: `hitl`/`pas`/… |
| `epistemic` | ◻ | `--field epistemic=` | `verified` / `proposed` / `assumed` (borrowed from the steward ledger) |
| `refs` | ◻ | `--ref` (repeatable) | **precise pointers** — see §2.3 |
| `body` | ◻ | `--body` | the full prose (caveat text, options, rationale) |

### 2.3 `refs` — cite precisely or it didn't happen

`refs` is where the ledger earns "precise references." Use typed prefixes so a reader (or a
tool) can resolve them:

- `git:<sha>` — a commit.
- `file:<path>` or `file:<path>:<line>` — a code/doc anchor.
- `plan:<path>` — an initiative plan.
- `seq:<n>` — **another entry in THIS ledger** (link an update to the item's prior entry, or
  to a related item's entry). This is how you thread history and relationships.
- `id:<PREFIX>-<n>` — another *item* (relationship without a specific entry).
- `opp:<id>`, `url:<…>`, or any `scheme:value` your repo needs.

Always link an **update** back to the item's previous entry with `--ref seq:<prev>` so the
chain is walkable from either end.

---

## 3. The workflow (open → update → resolve)

### 3.1 Open safely (the tool allocates `id`)

Never allocate a `TC-N` identity by grep, a sequence sidecar, or any local
counter. Independently cloned worktrees can allocate the same number, so no
local sequential scheme is globally safe. The todos profile allocates a
collision-resistant immutable `TC-<uuid>` while holding the append lock.

### 3.2 Open an item

Note `status` is a `--field` (there is no `--status` flag; the only tool flags are
`--kind`, `--summary`, `--field`, `--ref`, `--body`):

```bash
dev/agent-tools/ledgerwrite/ledgerwrite.py dev/todos-and-considerations-ledger.jsonl \
  --profile todos --open --kind caveat \
  --summary "Commission B future-couples to OPP-12 only if it graduates to a live-pipeline value test" \
  --field status=watching --field priority=p2 \
  --field owner=pas --field area=eval/crosssource --field epistemic=verified \
  --ref opp:OPP-12 --ref file:eval/crosssource/linker.py \
  --body "Today eval/crosssource imports nothing from src/memex, so it's independent of the OPP-12 id-contract. IF the bench later becomes an OPP-11 live-pipeline value test, it would consume SearchHit.logical_id and become downstream of Cause-A. Re-check when B moves from offline QID-join to live pipeline."
```

The command echoes the written record (including its `seq`) on stdout — capture it if you'll
reference this entry later.

### 3.3 Update / advance an item

Same `id`, same `kind`, a **new** `status`/`summary`, and a back-ref to the prior entry:

```bash
dev/agent-tools/ledgerwrite/ledgerwrite.py dev/todos-and-considerations-ledger.jsonl \
  --profile todos --kind caveat \
  --summary "B now going live-pipeline — coupling is now ACTIVE; gate on id-contract" \
  --field id=TC-<uuid> --field status=blocked --field blocked-by=OPP-12 \
  --expected-prior-seq 1 --ref seq:1 --ref opp:OPP-12
```

### 3.4 Resolve (terminal)

```bash
dev/agent-tools/ledgerwrite/ledgerwrite.py dev/todos-and-considerations-ledger.jsonl \
  --profile todos --kind todo \
  --summary "transformers/ReFinED env conflict resolved: pinned 4.x in an isolated extra" \
  --field id=TC-<uuid> --field status=done --field decider=hitl \
  --expected-prior-seq 3 --ref seq:3 --ref git:<sha>
```

`wont-do` and `superseded` are resolutions too — always leave a `--body`/`--ref` saying **why**.

---

## 4. Reading it back (derive state)

```bash
LW=dev/agent-tools/ledgerwatch/ledgerwatch.py
LEDGER=dev/todos-and-considerations-ledger.jsonl

# One item's full history, in order (the LAST line is its current state):
$LW $LEDGER --select id=TC-1 --state-dir dev/.ledgerwatch-todos-TC1

# Everything of one nature:
$LW $LEDGER --select kind=caveat --state-dir dev/.ledgerwatch-todos-caveat

# New activity since you last looked (delta):
$LW $LEDGER --state-dir dev/.ledgerwatch-todos

# Integrity check (run on resume, or if anything may have hand-edited the file):
$LW $LEDGER --validate
```

**Deriving the current state:** fold the ledger to its latest entry per `id` with
`--project`. The file is an event log; the fold is the state.

```bash
# Latest entry per id, one compact JSON object per line, sorted by id:
$LW $LEDGER --project

# Same, as a single envelope (adds the entries / folded_ids / unfoldable_no_id counts):
$LW $LEDGER --project --json
```

`--project` is read-only: it never writes the ledger and never advances the watch cursor,
so a projection cannot swallow the delta your next `$LW $LEDGER` run is waiting for.

Every run prints an `unfoldable (no id): N` bucket. Entries with no `id` are **normal**,
not an error — `dev/steward/steward-ledger.jsonl` is a decision trail in which *no* entry
has one (107/107), and 16 of this ledger's 76 entries have none either. A previous recipe
here folded with `latest[r["id"]] = r` and died with `KeyError: 'id'` on the first such
entry; the bucket is what replaces that failure with a number you can see.

**Asking "was this ruled on?"** — `--project rulings` (T2b) projects any of the three ledgers
to a **ruling registry**: which items carry a recorded ruling, which are still open, and which
the ledger does not say.

```bash
# The ruled/unruled registry for this ledger:
$LW $LEDGER --project rulings

# Works on the steward ledger and the OPP-12 sub-ledger too:
$LW dev/steward/steward-ledger.jsonl --project rulings
```

Read it knowing exactly what it can and cannot see:

| bucket | predicate | note |
|---|---|---|
| `ruled` | **any** entry under the key has `kind: decision` | a ruling stays recorded; a later `observation` does not un-rule it. `ruling_seqs` names the entries |
| `unruled` | no ruling entry **and** the folded entry's `status` is `open` | `open` is this README's own non-terminal status (§2.1), read case-insensitively because the file holds both `open` and `OPEN` |
| `unclassified` | everything else | **not a defect — the ledger genuinely does not say.** Emitted in full with its literal `kind`/`status`, never dropped |

The key is `id` when the entry has one, otherwise **`seq-N`**. That is deliberate: the steward
ledger holds most of this repo's rulings and *none* of its entries carry an `id`, and adding
one is out of scope for a decision trail. Because `seq` is unique **per file**, this mode reads
one ledger at a time — do not merge its keys across files without a file qualifier.

No other status is interpreted. `resolved`, `converged-pending-hitl`, `build-authorized`,
`placed`, `ratified-both-sides`… are echoed verbatim and land in `unclassified`, because
inventing a status vocabulary is a refused move here — and note that the **terminal** vocabulary
this README declares in §2.1 (`done` / `wont-do` / `superseded`) currently matches **zero**
entries in any of the three ledgers. Two consequences worth knowing before you trust a count:
an item ruled in prose but never entered as `kind: decision` reads as unruled or unclassified
(**TC-7** is the live example — it walked `open → in_progress → converged-pending-hitl →
resolved` without one), and the registry reports what the ledger *records*, never whether a
ruling is *right*.

Filter the projection with whatever you like (`jq 'select(.status=="open")'`) — the tool
deliberately imposes no status vocabulary, because the statuses actually in use here are
open-ended (`open`, `OPEN`, `resolved`, `closed`, `watching`, `RATIFIED`, `accepted`,
`placed`, `in_progress`, `build-authorized`, `converged-pending-hitl`, and entries with
no status at all).

> The `.ledgerwatch*` cursor dirs are **gitignored** (watcher state). Commit only the
> `.jsonl` and its `.seq` sidecar.

---

## 5. Rules (the short list)

1. **Append-only. Never hand-edit the `.jsonl`.** Write with `ledgerwrite`, read with `ledgerwatch`.
2. **Every entry carries `id` + `status` + a meaningful `kind`.** `kind` is immutable per `id`.
3. **State is derived, not stored** — the latest entry per `id` wins. Don't "correct" an old
   entry; append a new one that supersedes it.
4. **Cite precisely** in `refs` (`git:`/`file:path:line`/`seq:`/`plan:`/`id:`…). Link each update
   back to its prior entry with `--ref seq:<prev>`.
5. **Leave a reason on every terminal transition** (`done`/`wont-do`/`superseded`) in `--body`.
6. **Commit** the `.jsonl` + `.jsonl.seq`; the `.ledgerwatch*` cursors stay gitignored.
7. **`--validate` on resume**; if it ever exits `3` (corruption), stop and escalate — don't
   append onto a corrupt ledger.

---

## Porting to another repo

The protocol is repo-agnostic; only three things are local:

1. **The tools** — copy `dev/agent-tools/ledgerwrite/` + `dev/agent-tools/ledgerwatch/`
   (Python-3-stdlib-only, no venv). Adjust the invocation paths.
2. **The id prefix** — pick a short prefix (`TC`, `TODO`, `NOTE`, …) and use it consistently.
   State it at the top of your copy of this README.
3. **Gitignore the cursor** — add `<dir>/.ledgerwatch-*` (or your chosen `--state-dir`) so
   watcher state isn't committed; keep the `.jsonl` + `.seq` tracked.

The `kind`/`status` vocabularies, the field set, the event-sourced fold, and the citing
discipline are general — keep them. Extend `kind`/`status`/fields only additively, and
document any addition in your README copy so downstream readers stay in sync.

---

## Changelog

### 0.1.0 — 2026-07-03 (ported to FathomDB)

- Adopted from the memex `dev/todos-and-considerations-ledger` protocol (same event-sourced
  two-axis model over `ledgerwrite`/`ledgerwatch`). **FathomDB id prefix `TC`**; the ledger was
  **started empty** (memex's own entries were not carried over); cursor state gitignored at
  `dev/.ledgerwatch-todos*`. The Steward and orchestrator role contracts (`.claude/agents/`)
  were updated to use it for cross-cutting items another agent/session must not lose.

### 0.1.0 — 2026-07-03

- Initial protocol. Append-only, event-sourced todos/considerations/caveats/observations/
  questions ledger over `ledgerwrite`/`ledgerwatch`. Two-axis model (`kind`=nature immutable,
  `status`=lifecycle derived-by-fold), stable `id` join key, typed `refs`, live-board projection,
  portability guide. Memex id prefix `TC`.
