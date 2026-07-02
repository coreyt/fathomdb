# ledgerwrite

The **write-side companion to [`ledgerwatch`](../ledgerwatch/README.md)**. It
appends one well-formed JSON record to a JSONL ledger **without ever opening the
ledger body** — so an agent adds an entry at a context cost proportional to the
*entry*, never to the file's size or its own history.

---

## Overview

`ledgerwatch` solved the read side: watch a growing file and pay O(delta), not
O(file). `ledgerwrite` is the symmetric write side. Appending a line is itself
cheap (`echo >> f.jsonl`), so the tool is **not** about the append — it is about
three things `echo` cannot give you:

1. **A stamped, structured record.** UTC `ts` (millisecond, `Z`) and a monotonic
   `seq` are filled in for you, so entries sort and cross-reference (`--ref
   seq:7`) without you tracking either. `seq` comes from a tiny sidecar counter —
   **the ledger body is never read.**
2. **A validity guarantee.** Every record is emitted as exactly one line of valid
   JSON (embedded newlines/quotes/unicode are escaped), so a downstream
   `ledgerwatch --select field=value` / `--json` reader can never choke on a
   hand-mangled line.
3. **An enforced anti-drift discipline.** Because the tool writes without reading,
   an agent working a long-lived decision ledger never re-ingests old entries —
   the thing that chews context *and* pulls attention back onto stale work. The
   pairing is the point: **write with `ledgerwrite`, read deltas with
   `ledgerwatch`, and never open the ledger by hand.**

Like `ledgerwatch`, it is deliberately **generic** — it knows nothing about any
particular ledger's vocabulary. `--kind` and `--summary` are the two universal
fields of a ledger entry; everything else is `--field k=v`, `--ref R`, `--body`.
What the kinds *mean* is a convention of whoever owns the ledger (for the PAS
steward ledger, that vocabulary lives in `dev/steward/DESIGN.md`).

---

## Agent how-to

```bash
ledgerwrite.py <ledger.jsonl> --kind <kind> --summary <one-line> \
    [--field key=value ...] [--ref ref ...] [--body <prose>] \
    [--no-seq] [--dry-run] [--quiet]
```

- **Exit `0`** = the record was appended (and echoed on stdout so you can see /
  capture the assigned `seq`). **Exit `2`** = an error (missing/invalid argument,
  bad `--field`, or an I/O failure) — nothing was written. There is no "no-op"
  code: a write either lands or errors.
- **`--field key=value`** adds an arbitrary scalar field; repeatable; split on
  the first `=` only (values may contain `=`); last write wins per key. A field
  named the same as a reserved key (`ts`/`seq`/`kind`/`summary`/`refs`/`body`) is
  **ignored with a warning** — the flag wins, so the record shape stays stable.
- **`--ref`** is repeatable and collects into a `refs` array (use for
  `git:<sha>`, `plan:<path>`, `seq:<n>` back-references).
- **`--dry-run`** validates and echoes the record (with a `null` seq placeholder)
  but writes nothing and does not advance the counter — a safe peek.
- **`--quiet`** suppresses the stdout echo on success (the record still lands).
- **`--no-seq`** omits the `seq` field (and does not touch the counter).

The `seq` counter is a `<ledger>.seq` sidecar that always lives **beside the
ledger** — it is intrinsic to the ledger's identity, not a configurable location,
so the same ledger can never end up with two independent counters (which would
reuse a `seq`).

**Torn-line healing.** Before appending, ledgerwrite reads the file's **last
byte** (O(1), never into context). If the file is non-empty and that byte is not a
newline, a prior writer (a crash, a foreign appender, a hand edit) left an
unterminated line; ledgerwrite emits a leading newline and a stderr warning
(`healed a torn trailing line`) so its record lands on its own clean, valid line
instead of merging onto the fragment. This heals only the *trailing* boundary —
an interior torn line is out of scope (it would require an O(file) read; that job
belongs to a reader/validator, not this append path).

Record shape (keys in a stable, human-readable order):

```jsonc
{ "ts": "2026-07-01T16:47:55.071Z", "seq": 2, "kind": "drift",
  "summary": "roadmap stale vs git",
  "surface": "roadmap",            // arbitrary --field keys, sorted
  "refs": ["git:abc123"],          // present only if --ref given
  "body": "..." }                  // present only if --body given
```

This composes directly with the reader:

```bash
ledgerwrite.py steward-ledger.jsonl --kind drift --summary "roadmap stale" \
    --field surface=roadmap
ledgerwatch.py  steward-ledger.jsonl --select kind=drift        # sees only drift entries
```

---

## Purpose & Requirements

**Purpose.** Make "append to a long-lived ledger" cost O(entry) in context
instead of O(file), while guaranteeing the ledger stays machine-readable for
`ledgerwatch`.

**Functional requirements**

1. Append exactly one line of valid JSON per invocation; never corrupt an
   existing line; content with newlines/quotes/unicode is escaped, never split
   across lines.
2. Stamp `ts` (UTC, millisecond, trailing `Z`) automatically.
3. Assign a monotonic `seq` from a **sidecar counter beside the ledger**
   (`<ledger>.seq`), never by reading the ledger body; `--no-seq` opts out. A
   missing/corrupt/negative counter starts at `1` rather than crashing. `ts` is
   stamped *inside the lock* so it is ordered consistently with `seq`.
3a. Heal a torn **trailing** line (last byte not a newline) by emitting a leading
   newline + a warning, so a record never merges onto an unterminated predecessor.
4. Require a non-empty `--kind` and `--summary` (the two universal fields);
   accept arbitrary scalar `--field`s, a `--ref` list, and a `--body`.
5. Never let a `--field` clobber a reserved key — the named flag wins, with a
   warning.
6. Append **atomically under an exclusive advisory lock** (POSIX `flock`) so a
   concurrent writer can neither reuse a `seq` nor interleave a line; degrade to
   a plain `O_APPEND` write where `fcntl` is unavailable.
7. Unambiguous status on the exit code: `0` written, `2` error. A rejected call
   leaves the ledger untouched — a validation error is reported before any open,
   and a write that fails *after* the file was created removes the empty file.
8. `--dry-run` validates and echoes without writing or advancing the counter.

**Non-requirements.** It does not read, tail, query, or summarize the ledger —
that is `ledgerwatch`'s job. It heals only the *trailing* boundary; detecting an
**interior** torn/corrupt line (from a bad `sed`, truncation, or editor rewrite)
would require an O(file) parse and belongs to a reader/validator, not this append
path. It does not enforce a `kind` vocabulary or a field schema beyond "kind and
summary are non-empty" (the owning ledger's convention does that). It does not
rotate or compact the ledger.

**Constraints.** Python 3 standard library only (portable, no venv). One script,
one test file, this README — the same shape as `ledgerwatch`.

---

## Design

`run()` is a thin driver: validate args → build the record tail (stable key
order) → under an exclusive `flock` on the ledger, heal a torn trailing line if
needed, stamp `ts`, read-increment-write the sidecar `seq` counter, and append the
JSON line → echo the record. The lock spans the heal check, the counter update,
*and* the append, so they are one critical section: concurrent writers serialize,
`ts` orders with `seq`, and no two records can share a `seq` or interleave bytes.

- **`seq` counter.** `<ledger>.seq` (always beside the ledger) holds a single integer,
  written atomically (`*.tmp` + `os.replace`). It is the *only* file read on the
  write path — the ledger itself is opened append-only and never read. This is
  what preserves the "don't re-ingest old work" property.
- **Reserved-key protection.** `ts`/`seq`/`kind`/`summary`/`refs`/`body` are set
  only by their flags; a colliding `--field` is dropped with a stderr warning, so
  the record's core shape is never deformed by user input.
- **Durability.** After the write the file is `flush()`ed and `fsync()`ed inside
  the lock, so a crash cannot leave a torn trailing line for `ledgerwatch`'s
  partial-line handling to have to recover.

### Caveats

- **Sub-`PIPE_BUF` atomicity aside, the `flock` is the real guarantee.** On a
  filesystem without working `flock` (some network mounts) concurrency falls back
  to `O_APPEND` best-effort. For the single-steward use case this never matters.
- **`seq` is per-ledger, not global**, and its counter is not relocatable — it is
  always `<ledger>.seq` beside the ledger. (An earlier `--state-dir` option was
  removed precisely because pointing the same ledger at two different counter
  dirs could reuse a `seq`.) It is also **not gap-free**: a write that fails after
  the counter advanced leaves a gap, which is fine — the guarantee is uniqueness
  and monotonicity, not density.
- **It heals a torn *trailing* line, not an interior one.** A single last-byte
  read cannot see a corrupt line in the middle of the file; that needs a full
  parse (a reader/validator's job). See Non-requirements.
- **It trusts your `--field` values as strings.** Numbers/booleans are written as
  JSON strings (`"3"`, not `3`). `ledgerwatch --select` compares stringified
  values, so this is consistent — but do not expect typed numerics.

### Usage

```bash
# a program-steward decision (echoes the record + its seq on stdout)
ledgerwrite.py steward-ledger.jsonl --kind decision \
    --summary "keep the retrieval cascade OFF (gap 0.03 < 0.08 bar)" \
    --field decider=hitl --field surface=ledger --ref git:6ca5905

# a drift finding, quietly
ledgerwrite.py steward-ledger.jsonl --kind drift \
    --summary "STATUS-current says PR#101 open; git shows it merged at 4ccd879" \
    --field surface=status --field sev=med --quiet

# validate the shape without writing
ledgerwrite.py steward-ledger.jsonl --kind note --summary "x" --dry-run
```

---

## Tests

`test_ledgerwrite.py` (run: `python3 -m pytest -q` in this directory). Two layers,
mirroring `ledgerwatch`'s split of happy-path vs. characteristic-failure-mode:

**Functional surface.** Single-line valid JSON that round-trips (embedded
newlines/quotes/unicode in summary, body, *and* fields); `ts` shape; monotonic
`seq` and corrupt/negative-counter recovery; `--no-seq`; arbitrary fields (incl.
values containing `=`, empty values); `refs` list; `body`; `--dry-run` writes
nothing and does not advance the counter; `--quiet`; every error path (missing
file arg, missing/empty kind or summary, bad `--field`, empty field key, missing
parent dir) exits `2` with nothing written; the counter always lives beside the
ledger (and `--state-dir` no longer exists).

**Failure modes — the silent-corruption risks this tool exists to prevent.**
*Concurrent writers* (24 processes appending at once) must serialize under the
`flock` with no lost line, no interleaved bytes, no two records sharing a `seq`,
and `ts` non-decreasing in `seq` order (the headline test — the reason the lock
exists); append *preserves foreign prior content* byte-for-byte; a written record
is always *newline-terminated*; `ts` *parses* as tz-aware ISO-8601. **Torn-line
heal**: a foreign torn trailing line is isolated (not merged onto) and our record
lands valid + selectable with a warning, no spurious blank line on a clean ledger,
and no heal on an empty file. **Failed write leaves the ledger untouched**: a
fault-injected lock failure removes the 0-byte file it created, and never deletes
a pre-existing ledger's content. *Every* reserved key
(`ts`/`seq`/`kind`/`summary`/`refs`/`body`) is protected from a same-named
`--field` (parametrized), and a real `--ref`/`--body` value *wins* over a clobber
attempt; a `--dry-run` does not consume a `seq`. **42 passing.**

---

## Changelog

### 0.1.0 — 2026-07-01

- Initial tool: append one stamped, validated JSON record to a JSONL ledger
  without reading the ledger body; auto `ts` (stamped inside the lock, so ordered
  with `seq`) + monotonic sidecar `seq` (always `<ledger>.seq` beside the ledger);
  arbitrary `--field`, `--ref` list, `--body`; reserved-key protection; atomic
  `flock`-guarded append with `fsync`; **torn-trailing-line heal** via a one-byte
  `pread`; failed writes leave the ledger untouched (empty file removed);
  `--dry-run` / `--quiet` / `--no-seq`. Exit `0` written / `2` error. 42-test suite
  (functional surface + failure modes: concurrent-writer flock guarantee incl.
  `ts`/`seq` ordering, foreign-content preservation, torn-line heal, write-failure
  cleanup, full reserved-key protection incl. real-value precedence). Symmetric
  companion to `ledgerwatch` 0.1.0.
- Post-review fixes (Sonnet 5): removed `--state-dir` (could reuse a `seq` across
  one ledger); negative counter now restarts at 1; `ts` moved inside the lock.
