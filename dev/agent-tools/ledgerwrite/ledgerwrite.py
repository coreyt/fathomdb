#!/usr/bin/env python3
"""ledgerwrite — append one well-formed JSON record to a JSONL ledger.

The write-side companion to ledgerwatch. Its whole reason to exist is the same
one ledgerwatch has on the read side: keep an agent's *context* small and its
attention un-drifted while it works a long-lived ledger. ledgerwatch lets you
read only the delta; ledgerwrite lets you append **without opening the file at
all** — so an agent never re-ingests old entries (the thing that chews context
and pulls attention back onto stale work) just to add a new one.

Appending a line is cheap on its own (`echo >> f.jsonl`). What this tool adds:

  * a stamped, structured record — UTC `ts` and a monotonic `seq` are filled in
    for you, so entries sort and cross-reference without you tracking them;
  * a validity guarantee — every record is emitted as exactly one line of valid
    JSON, so a downstream `ledgerwatch --select field=value` / `--json` reader
    can never choke on a hand-mangled line;
  * an atomic append (advisory-locked where the OS supports it), safe for the
    shared working tree;
  * it NEVER reads the ledger body — only a tiny sidecar counter — so the
    "don't re-read old work" discipline is enforced by the tool, not by hope.

It is deliberately generic (like ledgerwatch): it knows nothing about any
particular ledger's vocabulary. `--kind` and `--summary` are the two universal
fields of a ledger entry; everything else is `--field k=v`, `--ref R`, `--body`.
The meaning of the kinds is a convention of whoever owns the ledger.

Usage:
  ledgerwrite.py <ledger.jsonl> --kind decision --summary "..." \\
      [--field surface=roadmap] [--field decider=hitl] [--ref git:abc123] \\
      [--body "longer prose"] [--no-seq] [--dry-run] [--quiet]

Exit status:
  0  the record was appended (or, with --dry-run, would be) and echoed
  2  error — missing/invalid argument, bad --field, or an I/O failure
"""

import argparse
import json
import os
import sys
from datetime import datetime, timezone

try:
    import fcntl  # POSIX advisory locking; absent on Windows.
except ImportError:  # pragma: no cover - platform fallback
    fcntl = None


def utc_ts() -> str:
    """UTC ISO-8601 with millisecond precision and a trailing Z."""
    return (
        datetime.now(timezone.utc)
        .isoformat(timespec="milliseconds")
        .replace("+00:00", "Z")
    )


def parse_fields(items):
    """Turn repeated ``field=value`` items into a dict (last write wins).

    Splits on the first ``=`` only, so values may contain ``=``. A missing ``=``
    is a hard error — a silently-dropped field is worse than a loud one.
    """
    fields = {}
    for item in items or []:
        if "=" not in item:
            raise ValueError(f"--field must be key=value: {item!r}")
        key, value = item.split("=", 1)
        key = key.strip()
        if not key:
            raise ValueError(f"--field key must be non-empty: {item!r}")
        fields[key] = value
    return fields


def next_seq(seq_path: str) -> int:
    """Read → increment → write the sidecar counter. Never touches the ledger.

    A missing or corrupt counter starts at 1 (never crashes the write). The
    caller holds the ledger lock across this, so the read-modify-write is safe
    against a concurrent ledgerwrite on the same ledger.
    """
    try:
        with open(seq_path, "r", encoding="utf-8") as fh:
            current = int(fh.read().strip() or "0")
    except (FileNotFoundError, ValueError, OSError):
        current = 0
    # A missing/corrupt/nonsensical (e.g. negative) counter restarts at 1.
    if current < 0:
        current = 0
    nxt = current + 1
    tmp = seq_path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write(str(nxt))
    os.replace(tmp, seq_path)
    return nxt


def build_record(args, fields):
    """Assemble the record body (``kind`` onward) with a stable key order.

    ``ts`` and ``seq`` are prepended by the caller inside the flock — ``ts`` so
    it is ordered consistently with ``seq`` under concurrency, ``seq`` because it
    needs the counter file. Arbitrary --field keys follow the reserved head,
    sorted. Reserved keys win over a colliding --field (with a warning) so the
    record shape stays predictable. Returns (tail, clobbered_reserved_keys).
    """
    record = {"kind": args.kind, "summary": args.summary}
    reserved = {"ts", "seq", "kind", "summary", "refs", "body"}
    extra = {k: v for k, v in fields.items() if k not in reserved}
    for key in sorted(extra):
        record[key] = extra[key]
    if args.ref:
        record["refs"] = list(args.ref)
    if args.body is not None:
        record["body"] = args.body
    return record, (set(fields) & reserved)


def run(argv, out=sys.stdout, err=sys.stderr) -> int:
    parser = argparse.ArgumentParser(prog="ledgerwrite", add_help=True)
    parser.add_argument("file", nargs="?")
    parser.add_argument("--kind", help="entry kind (e.g. decision, drift, reconcile)")
    parser.add_argument("--summary", help="one-line human summary of the entry")
    parser.add_argument(
        "--field",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="arbitrary scalar field; repeatable (last write wins per key)",
    )
    parser.add_argument(
        "--ref",
        action="append",
        default=[],
        metavar="REF",
        help="a reference (git:sha, plan:path, seq:N); repeatable → refs[]",
    )
    parser.add_argument("--body", default=None, help="optional longer prose body")
    parser.add_argument(
        "--no-seq", action="store_true", help="do not assign a monotonic seq"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="validate and echo the record without writing it",
    )
    parser.add_argument(
        "--quiet", action="store_true", help="do not echo the record on success"
    )
    args = parser.parse_args(argv)

    if not args.file:
        print("ledgerwrite: ledger file argument required", file=err)
        return 2
    if not args.kind or not args.kind.strip():
        print("ledgerwrite: --kind is required and must be non-empty", file=err)
        return 2
    if not args.summary or not args.summary.strip():
        print("ledgerwrite: --summary is required and must be non-empty", file=err)
        return 2

    try:
        fields = parse_fields(args.field)
    except ValueError as exc:
        print(f"ledgerwrite: {exc}", file=err)
        return 2

    tail, clobbered = build_record(args, fields)
    for key in sorted(clobbered):
        print(
            f"ledgerwrite: --field {key}=... ignored (reserved key set by a flag)",
            file=err,
        )

    if args.dry_run:
        # Peek/validate: stamp ts and a placeholder seq so the echoed shape
        # matches a real write, but touch nothing on disk.
        record = {"ts": utc_ts()}
        if not args.no_seq:
            record["seq"] = None
        record.update(tail)
        line = json.dumps(record, ensure_ascii=False)
        if not args.quiet:
            out.write(line + "\n")
        return 0

    abspath = os.path.abspath(args.file)
    parent = os.path.dirname(abspath)
    if parent and not os.path.isdir(parent):
        print(f"ledgerwrite: no such directory: {parent}", file=err)
        return 2
    # The seq counter is intrinsic to the ledger, so it always lives beside it —
    # never a user-chosen dir, which would let the same ledger keep two
    # independent counters and reuse a seq.
    seq_path = abspath + ".seq"

    pre_existed = os.path.exists(abspath)
    fd = None
    line = None
    try:
        # O_APPEND: every write lands at EOF. Hold an exclusive advisory lock
        # across the (heal check + seq read-modify-write + append) so concurrent
        # writers can neither reuse a seq nor interleave a line.
        fd = os.open(abspath, os.O_RDWR | os.O_CREAT | os.O_APPEND, 0o644)
        if fcntl is not None:
            fcntl.flock(fd, fcntl.LOCK_EX)
        try:
            # Heal a torn last line: if the file has content whose final byte is
            # not a newline, some writer (a crash, a foreign appender, a hand
            # edit) left an unterminated line. Emit a leading newline so our
            # record lands on its own clean line instead of merging onto it.
            # Reading one byte is O(1) and never enters the agent's context, so
            # this does not compromise the token-efficiency contract.
            prefix = ""
            if hasattr(os, "pread"):
                size = os.fstat(fd).st_size
                if size > 0 and os.pread(fd, 1, size - 1) != b"\n":
                    prefix = "\n"
                    print(
                        "ledgerwrite: healed a torn trailing line "
                        "(a prior write left no newline)",
                        file=err,
                    )
            # Stamp ts inside the lock so it is ordered consistently with seq.
            record = {"ts": utc_ts()}
            if not args.no_seq:
                record["seq"] = next_seq(seq_path)
            record.update(tail)
            line = json.dumps(record, ensure_ascii=False)
            os.write(fd, (prefix + line + "\n").encode("utf-8"))
            os.fsync(fd)
        finally:
            if fcntl is not None:
                fcntl.flock(fd, fcntl.LOCK_UN)
    except OSError as exc:
        # If we created the ledger and failed before writing content, remove the
        # empty file so a rejected call leaves the ledger untouched.
        if not pre_existed and os.path.exists(abspath):
            try:
                if os.path.getsize(abspath) == 0:
                    os.remove(abspath)
            except OSError:
                pass
        print(f"ledgerwrite: write failed: {exc}", file=err)
        return 2
    finally:
        if fd is not None:
            try:
                os.close(fd)
            except OSError:
                pass

    if not args.quiet:
        out.write(line + "\n")
    return 0


if __name__ == "__main__":  # pragma: no cover
    sys.exit(run(sys.argv[1:]))
