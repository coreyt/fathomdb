# ledgerwatch

A single-file agent tool that emits **only what changed** in a monitored file
since the last check, so an agent watching a long-lived ledger pays context cost
proportional to the *delta*, not to the file's size or its own history.

---

## Overview

When an agent polls a growing file (a chat bus, a status doc, a roadmap, an event
log), the expensive thing is not the file on disk — it is that **the whole
conversation prefix is re-billed on every turn**, and naive re-reads keep
appending the entire file into that prefix. `ledgerwatch` runs at the shell,
persists a per-file cursor, and prints just the new/changed region. Only that
delta enters the agent's context.

It picks a delta strategy from the file extension, because the right notion of
"what's new" differs by format:

| Extension              | Strategy  | Notion of "new"                                  |
|------------------------|-----------|--------------------------------------------------|
| `.jsonl .ndjson .log`  | `tail`    | lines appended past a positional cursor          |
| `.md .markdown`        | `section` | heading sections whose content hash changed      |
| anything else          | `diff`    | unified-diff hunks vs. a shadow copy             |

Override with `--strategy`.

Every run also reports a **mode** — `incremental`, `cold`, or `resync` — so the
consumer knows whether the output is an exact delta or a re-read baseline (see
Design → *Run mode*).

---

## Agent how-to

If you are an agent polling a file, this is the contract:

```bash
ledgerwatch.py <file> --state-dir <persistent-dir>
```

> ### ⚠️ Read the exit code — `1` is NOT a failure
>
> ledgerwatch uses **grep-style exit codes by default**:
>
> | code | meaning                                   | what you do                |
> |------|-------------------------------------------|----------------------------|
> | `0`  | **changed** — a delta is on stdout        | act on the delta           |
> | `1`  | **no change** — a normal idle tick        | nothing; this is expected  |
> | `2`  | **error** — missing file/arg, bad select  | surface it                 |
>
> **`1` is the routine idle case, not an error.** If your harness reports any
> nonzero exit as "command failed", do not treat exit `1` as a problem — only
> `2` is a real failure. Either branch on the exact code, or pass `--no-status`
> to make every successful run exit `0` (reserving `2` for errors).

- **Empty stdout means nothing changed** (and the exit code is `1`). An idle tick
  costs you almost nothing. Empty stdout never means "broken" — a broken run
  exits `2`.
- **Non-empty stdout is the delta** (exit `0`) — the new lines / changed sections
  / diff hunks, and *only* those. Do not re-read the whole file.
- **Watch the mode (stderr in text mode, `"mode"` field in `--json`):**
  - `incremental` → these are exactly the new/changed items. Act on them.
  - `cold` → first run: the output is a **baseline snapshot**, not N new events.
  - `resync` → the cursor was invalidated (the file was rotated/truncated/
    rewritten). The output is a **re-read baseline**. Do **not** treat it as a
    burst of new events — it is the tool telling you it lost its place and
    recovered. In text mode this prints `ledgerwatch: mode=resync (...)` to
    stderr; in `--json` it is `"mode": "resync"`.
- **Recommended invocations:**
  - Branch on status without capturing stdout: `ledgerwatch f; case $? in 0) ...;; 1) ;; 2) alert;; esac`
  - Harness that dislikes nonzero exits: add `--no-status` (0 = ok, 2 = error)
  - Shared JSONL bus, only your component: `--select component=<name>`
  - Programmatic parsing: `--json` (envelope on change; silent + exit `1` on idle)
  - Peek without consuming: `--dry-run` (prints the delta, leaves the cursor)
  - Integrity scan a JSONL ledger: `--validate` (exit `0` clean / `3` corrupt / `2` error)
- **Validation (tail/JSONL).** *Delta-validate* is **on by default**: any non-blank
  line in the emitted delta that is not valid JSON is flagged on **stderr** (with
  its absolute line number), so a torn/corrupt line is surfaced even when
  `--select` would otherwise silently drop it (`match_select` does `json.loads`).
  The delta status/stdout/exit are unchanged — it is a diagnostic only, scoped to
  the delta, so it never re-spams corruption that lies before the cursor.
  **`--validate`** is the opt-in *full-file* scan: it reports every invalid-JSON
  line and an unterminated final line as a **bounded summary** (first 20 + a
  count), catching *interior* corruption a delta can't (e.g. a botched `sed -i`).
  It does not emit a delta or advance the cursor, and uses a distinct exit code
  (`0` clean / `3` corrupt / `2` read error) so it never muddies the delta status.
  Blank lines are ignored; add `--json` for a structured envelope.
- **State is yours to keep.** Point `--state-dir` at a directory that survives
  between polls (or set `$LEDGERWATCH_STATE`). It is safe to `rm -rf` that dir to
  fully reset, or run `--prune` to drop cursors for files that no longer exist.

`--json` envelope shape (emitted only on a change; idle runs are silent + exit `1`):

```jsonc
{ "file": "/abs/path", "strategy": "tail", "mode": "incremental", "changed": true,
  "lines": ["...", "..."] }                 // tail
{ ..., "sections": [{"key":"## H","kind":"new|changed|removed","body":"..."}] } // section
{ ..., "diff": "--- prev\n+++ cur\n@@ ..." }                                    // diff
```

---

## Purpose & Requirements

**Purpose.** Make "monitor a file that grows without bound" cost O(delta) in
context instead of O(file) × O(turns).

**Functional requirements**

1. Emit, on each run, only content new or changed since the previous run; emit
   nothing on stdout when nothing changed (text *and* `--json`). The run's
   status rides on the **exit code**, not on whether stdout is empty.
2. Choose strategy by extension; allow explicit override.
3. **Never silently miss an update.** This is the failure mode that matters —
   a monitor that drops an event is worse than one that is merely chatty. Each
   strategy is hardened against its characteristic miss:
   - *tail*: rotation, truncation, and same-length in-place rewrites must
     re-emit rather than report "no change" (detected via a first-line
     signature + size check, not byte offset alone).
   - *tail*: a partial trailing line (writer caught mid-append) is held back and
     emitted **exactly once** when completed — never as a half record, never
     lost.
   - *section*: an edit **anywhere** in the document (including the first
     section) is detected — the core reason a positional cursor is wrong for
     multi-section files.
   - *section*: a removed section is surfaced as an event.
4. **Avoid false positives** that would flood context: reordering identical
   sections is not a change.
5. **Report trust in the delta.** Each run announces its *mode* so the consumer
   can distinguish an exact incremental delta from a re-read baseline (cold /
   resync) and not misread a recovery as a burst of new events.
6. **Unambiguous status on the exit code.** Grep-style by default — `0` changed,
   `1` no change, `2` error — so a poller branches on `$?` without capturing
   stdout, *and* "empty stdout" can only mean no-change (errors are a distinct
   code). `--no-status` collapses `0`/`1` into `0` for harnesses that read any
   nonzero as failure.
7. Be robust: missing or corrupt state is a cold start, not a crash; a missing
   target file (or missing file argument) is a clean exit 2.
7a. **Surface corruption, never drop it silently.** For `tail`/JSONL, an
    invalid-JSON line in the delta is flagged on stderr by default (delta-validate);
    an opt-in `--validate` scans the whole file for invalid lines + an unterminated
    final line and reports a bounded summary with a distinct exit code (`0`/`3`/`2`).
    This is the read-side complement to `ledgerwrite`'s write-side torn-line heal.
8. Optional `--select field=value[,value]` filtering for `tail` (component-scoped
   views over a shared JSONL bus).
9. **Peek without consuming** (`--dry-run`): compute and print the delta without
   advancing the cursor, so a glance never drops a later real read's update.
10. **Structured output** (`--json`): an envelope on change (silent + exit `1` on
    idle), with bodies JSON-escaped so content can't break the stream.
11. **State hygiene** (`--prune`): drop cursor/shadow files whose source file is
    gone, without touching cursors it cannot identify.

**Non-requirements.** It does not watch/inotify (the caller schedules polls); it
does not parse semantics beyond JSON field matching; it does not transform or
summarize — it only extracts the delta. It does not seek within large files (it
reads the whole file each run; see Caveats).

**Constraints.** Python 3 standard library only (portable, no venv needed). One
script, one test file, this README.

---

## Design

Each strategy function is **pure**: it reads the file and computes a structured
*payload* + the next cursor state, and writes nothing. The driver renders the
payload (text or `--json`) and commits state (unless `--dry-run`). That split is
what makes `--json` and `--dry-run` fall out without per-strategy special-casing.
Strategies return `(payload, new_state, changed, mode, shadow_content)`.

### Strategy: `tail` (append-only, rotation-safe)

Reads the whole file, then:

- Commits only up to the **last newline**. A partial trailing line is excluded
  and picked up on the next run — emitted once, when complete.
- Stores `offset`, `size`, and `head_sig` = SHA-1 of the first line.
- **Cold-starts (re-emits from byte 0) when** `head_sig` changed (file replaced /
  first line rewritten) **or** the stored offset exceeds the current size
  (truncation/rotation). This is what defeats the "missed update" failure for
  same-length in-place rewrites and shorter rotations — an offset-only cursor
  would report no change.
- `--select` parses each candidate line as JSON and keeps it only if every
  `field=value[,value]` constraint matches (AND across fields, membership within
  a field). Non-JSON lines are dropped when a select is active.

### Strategy: `section` (intra-document, content-addressed)

Splits the document on Markdown ATX headings (`#`..`######`). Content before the
first heading is a synthetic `(preamble)` section; duplicate headings are
disambiguated (`## Notes`, `## Notes #2`).

Each section's identity is its heading key; its state is the SHA-1 of its
**stripped** body. Stripping is deliberate: the blank line separating sections
re-attaches to a different section when sections are reordered, so hashing raw
bodies would flag reorders as changes. On each run:

- new key → `new`; known key, hash differs → `changed`; key gone → `removed`;
  reorder of identical sections → nothing.

Because identity is the heading and content is the hash, an edit to the first
section is caught exactly like an edit to the last — the property a positional
cursor cannot provide on a multi-section file. The strategy is content-addressed,
so it can never lose its place: its mode is only ever `cold` (first run) or
`incremental`.

### Strategy: `diff` (fallback, any text)

Keeps a shadow copy keyed by the file's absolute path; emits
`difflib.unified_diff` hunks against it and advances the shadow. First run (no
shadow) yields the full file as additions. A wholesale replacement simply shows
as a large diff — correct without any rotation special-case — so diff is only
ever `cold` or `incremental`.

### Run mode

`mode` tells the consumer how much to trust the output:

| mode          | meaning                                              | strategies |
|---------------|------------------------------------------------------|------------|
| `incremental` | exact new/changed items since a valid cursor         | all        |
| `cold`        | first run / unreadable state → output is a baseline  | all        |
| `resync`      | cursor invalidated (rotation/truncation/rewrite)     | tail       |

In text mode, mode is printed to **stderr only when not `incremental`** (so clean
ticks stay silent on both streams). In `--json` mode it is always the `"mode"`
field. `resync` is the important one: it stops an agent reading a post-rotation
re-emit as a burst of fresh events.

### Exit codes (the status channel)

stdout carries the payload; **the exit code carries the status**, so the two
never have to be disambiguated from each other. Grep-style by default:

| code | meaning   | notes                                              |
|------|-----------|----------------------------------------------------|
| `0`  | changed   | a delta was written to stdout                      |
| `1`  | no change | a normal idle tick — silent stdout, **not** a fail |
| `2`  | error     | missing file/argument, bad `--select`              |

This is what lets an idle tick be cheap *and* unambiguous: because a no-change
run exits `1` and every error exits `2`, "empty stdout" can only mean no-change —
it can never silently mean "broken." `--no-status` collapses `0`/`1` into `0`
(reserving `2` for errors) for harnesses that treat any nonzero exit as failure.

The three channels each do one job: **exit code** = status, **stdout** = payload
(silent on idle in every mode), **stderr** = diagnostics (the mode note).

### `--dry-run`, `--json`, `--prune`

- **`--dry-run`** renders the delta but skips both state save and shadow write,
  so a peek can never consume an update a later real run needs. The exit code
  still reflects whether a change was detected (`0`/`1`).
- **`--json`** emits one envelope **when there is a change** (silent + exit `1`
  on idle, like text mode). Bodies are JSON-escaped, so quotes/newlines/unicode
  in content are safe.
- **`--prune`** (no file argument) scans the state dir and removes cursor +
  shadow files whose recorded source `path` no longer exists. State files it
  cannot parse, or that lack a recorded path, are left untouched — it never
  deletes something it cannot identify.

### State & selection

- State lives in `--state-dir` (default `$LEDGERWATCH_STATE` or `./.ledgerwatch`),
  one JSON file per watched path keyed by SHA-1 of its absolute path, written
  atomically (`*.tmp` + `os.replace`). Each state file records the source `path`
  (used by `--prune`).
- Corrupt/missing state → `{}` → cold start. `--reset` discards cursor + shadow.
- A strategy change between runs (extension default vs. `--strategy`)
  cold-starts under the new strategy.

### Caveats

- **One watcher per file.** State is a read-modify-write of the cursor file
  (writes are atomic, but the read+write pair is not locked). Two concurrent
  runs on the same file could both read the old cursor and double-emit. Fine for
  a single polling agent; do not fan out multiple watchers at the same file +
  state dir.
- **Whole-file read.** Every run reads the entire file (the delta is small in
  *context*, but disk I/O is O(file)). Not optimized for multi-GB logs; a
  seek-based tail read is a possible future addition.
- **Path identity.** Cursors are keyed by absolute path (not realpath). Renaming
  the file, or swapping a symlink, presents as a new file → a one-time cold
  start. Orphaned cursors are cleaned by `--prune`.
- **Duplicate headings.** Section identity uses occurrence order (`#2`, `#3`);
  inserting a new same-named section ahead of existing ones shifts those indices
  and can re-emit the shifted siblings once. Unique headings avoid it.

### Usage

```bash
# tail a JSONL bus, only the auth component (exit 0 = new lines, 1 = idle)
ledgerwatch.py bus.jsonl --select component=auth

# branch on status in a poll loop
ledgerwatch.py STATUS.md --state-dir .watch; case $? in
  0) echo "changed" ;; 1) ;; 2) echo "error" >&2 ;;
esac

# harness that dislikes nonzero exits: 0 on success, 2 only on error
ledgerwatch.py bus.jsonl --no-status

# structured output / peek without consuming / force a strategy / reset
ledgerwatch.py bus.jsonl --json
ledgerwatch.py bus.jsonl --dry-run
ledgerwatch.py notes.conf --strategy diff
ledgerwatch.py bus.jsonl --reset

# housekeeping: drop cursors for files that no longer exist
ledgerwatch.py --prune --state-dir .watch
```

---

## Tests

`test_ledgerwatch.py` (run: `python3 -m pytest -q` in this directory). Each
strategy and feature has happy-path and miss-mode coverage; the failure-mode
tests are the point of the suite.

**tail / JSONL** — cold start emits all; append emits only new; no-op emits
nothing; empty file is clean. Miss-modes: rotation/truncation re-emits;
same-length in-place rewrite is detected; partial trailing line is held then
emitted exactly once. Plus `--select` single-value and multi-value membership.

**section / Markdown** (status, roadmap, plan, design, ledger docs) — cold start
emits all; append within a section and edit of the **first** section are caught
while untouched sections stay silent; no-op silent; new section `[new]`; removed
section `[removed]`; duplicate headings disambiguated; preamble tracked.
False-positive guard: reordering identical sections emits nothing.

**diff / fallback** — cold start emits full add; edit emits only the hunk; no-op
silent; unknown extension routes to diff.

**run mode** — cold announced on first run; incremental is silent; rotation
announces `resync`; section never resyncs even after a sweeping edit; section
cold-starts on corrupt state.

**exit codes** — default grep-style: changed → `0`, no-change → `1`, error → `2`;
`--no-status` collapses `0`/`1` into `0` but still reports errors as `2`
(negative control). Every no-change test pins `out == "" AND rc == 1`, so silence
can never green a test on its own — errors are a provably distinct code.

**`--dry-run`** — does not advance the tail cursor; idempotent across repeats;
does not write the diff shadow (so the next real run still sees the change);
does not persist section hashes; exit code still reflects the change.

**`--json`** — tail/section/diff envelopes carry strategy/mode/changed + payload;
a no-op is silent on stdout and exits `1` (no envelope); quotes/backslashes/
newlines/unicode are escaped and round-trip; mode reports `resync`; `--json`
composes with `--dry-run`.

**`--prune` & argument handling** — removes a stale cursor (and its orphan
shadow); keeps a live cursor; ignores/keeps unparseable state files; is a no-op
on a missing state dir; a missing file argument without `--prune` exits 2.

**state & robustness** — corrupt state cold-starts (no crash); `--reset`
re-emits; missing file exits 2; `--strategy` override works and a strategy change
between runs cold-starts; `--select` warns on non-tail strategy.

**validate** — *delta-validate*: an invalid-JSON delta line is flagged on stderr
with its absolute line number and still emitted (status unchanged); it is flagged
even when `--select` drops it (the silent-miss guard); a clean delta is silent; no
warnings for non-tail strategies. *`--validate`*: clean file exits `0`; interior
corruption exits `3` with the line number; an unterminated final line exits `3`;
blank lines are ignored; output is bounded (cap + "…N more"); `--json` envelope
carries `valid`/`invalid_count`/`invalid_lines`; a missing file exits `2`.

Result: **65 passing.**

---

## Changelog

### 0.2.0 — 2026-07-01

- **Validation.** *Delta-validate* (tail, on by default): flags an invalid-JSON
  line in the emitted delta on stderr with its absolute line number — surfacing a
  torn/corrupt line even when `--select` would silently drop it — without touching
  the delta status/stdout/exit. *`--validate`* (opt-in): a full-file JSONL
  integrity scan reporting invalid lines + an unterminated final line as a bounded
  summary, with a distinct exit code (`0` clean / `3` corrupt / `2` error), no
  delta, no cursor advance. The read-side complement to `ledgerwrite`'s torn-line
  heal. 11 new tests (65 total).

### 0.1.0 — 2026-06-30

- Initial tool: extension-routed delta extraction with `tail` (jsonl/log,
  rotation-safe), `section` (markdown, intra-document content hashing), and
  `diff` (fallback) strategies; per-file persistent cursor.
- Run **mode** signal (`incremental` / `cold` / `resync`) so a consumer never
  misreads a re-synced baseline as a burst of new events.
- Exit code as the status channel — grep-style `0` changed / `1` no change / `2`
  error, with `--no-status` to collapse `0`/`1` → `0`; stdout is payload-only
  (silent on idle in text and `--json`).
- Flags: `--select`, `--json`, `--dry-run`, `--reset`, `--strategy`,
  `--no-status`, `--prune`.
- Hardened the characteristic miss-modes (rotation/in-place rewrite and
  partial-line exactly-once for tail; intra-document edit for section) and the
  reorder false-positive for section.
- 54-test suite covering happy paths and per-format edge/failure modes; exit
  codes are pinned alongside stdout so silence can't pass a test on its own.
