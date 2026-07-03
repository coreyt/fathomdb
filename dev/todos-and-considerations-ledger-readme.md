# Todos & Considerations Ledger — protocol

> A durable, append-only, **agent-readable** ledger for the cross-cutting items that
> otherwise die in chat: **todos, considerations, caveats, observations, and open
> questions** that don't belong to any single plan, slice, or review — but that a
> future session (human or agent) must not lose.
>
> **Ledger file:** `dev/todos-and-considerations-ledger.jsonl` (one JSON record per line).
> **This repo's id prefix:** `TC` (items are `TC-1`, `TC-2`, …).
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

- **Write** only with **`ledgerwrite`** (stamps `ts` + a monotonic `seq`; never opens the
  ledger body). Never `echo >>`, never an editor.
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

### 2.2 Field reference

| field | req? | set by | notes |
|---|---|---|---|
| `ts`, `seq` | — | tool | stamped by `ledgerwrite`; never pass them |
| `kind` | ✅ | `--kind` | the nature (table above); immutable per `id` |
| `summary` | ✅ | `--summary` | one line; for an update, describe the *change* |
| `id` | ✅ | `--field id=` | stable handle `TC-<n>`; **same across the item's whole life** |
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

### 3.1 Pick the next `id` (open only)

`id`s are stable and human-readable, so allocate the next integer once, at open time. This
is a mechanical shell op (not a context read of entries):

```bash
# highest existing TC id, or 0 if none yet:
grep -ho '"id": "TC-[0-9]\+"' dev/todos-and-considerations-ledger.jsonl 2>/dev/null \
  | grep -o '[0-9]\+' | sort -n | tail -1
# → your new id is that + 1 (e.g. TC-4)
```

(Alternative allowed convention: use the **opening entry's `seq`** as `<n>` — write the open
entry, then read the echoed `seq` and use `TC-<seq>` for all updates. Either is fine; pick one
per repo and be consistent.)

### 3.2 Open an item

Note `status` is a `--field` (there is no `--status` flag; the only tool flags are
`--kind`, `--summary`, `--field`, `--ref`, `--body`):

```bash
dev/agent-tools/ledgerwrite/ledgerwrite.py dev/todos-and-considerations-ledger.jsonl \
  --kind caveat \
  --summary "Commission B future-couples to OPP-12 only if it graduates to a live-pipeline value test" \
  --field id=TC-1 --field status=watching --field priority=p2 \
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
  --kind caveat --summary "B now going live-pipeline — coupling is now ACTIVE; gate on id-contract" \
  --field id=TC-1 --field status=blocked --field blocked-by=OPP-12 \
  --ref seq:1 --ref opp:OPP-12
```

### 3.4 Resolve (terminal)

```bash
dev/agent-tools/ledgerwrite/ledgerwrite.py dev/todos-and-considerations-ledger.jsonl \
  --kind todo --summary "transformers/ReFinED env conflict resolved: pinned 4.x in an isolated extra" \
  --field id=TC-3 --field status=done --field decider=hitl \
  --ref seq:3 --ref git:<sha>
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

**Deriving the live board (all open items):** fold to the latest entry per `id`, then drop
terminal statuses. A quick, dependency-free projection:

```bash
python3 - <<'PY'
import json, collections
latest = {}
for line in open("dev/todos-and-considerations-ledger.jsonl"):
    line = line.strip()
    if not line: continue
    r = json.loads(line)
    latest[r["id"]] = r            # last write wins (file is in append/seq order)
TERMINAL = {"done", "wont-do", "superseded"}
live = [r for r in latest.values() if r.get("status") not in TERMINAL]
live.sort(key=lambda r: (r.get("priority","p9"), r["id"]))
for r in live:
    print(f'{r["id"]:7} [{r["kind"]:13}] {r.get("status",""):11} {r["summary"]}')
PY
```

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
