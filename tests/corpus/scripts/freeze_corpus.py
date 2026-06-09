#!/usr/bin/env python3
"""COR-2 corpus freeze / verify / reconcile tool.

Turns the manual COR-2 freeze checklist (corpus-card.md "Determinism" lock +
scaffolds/5-COR-2-corpus-freeze.md) into one deterministic command. It operates
on the on-disk corpus under data/corpus-data/raw/ (gitignored — restored from CI
cache or rebuilt by the acquire_*.py scripts), so it MUST run where the data
actually lives, not in an ephemeral checkout with an empty data dir.

Modes
-----
  (default)        VERIFY  — hash every raw/*.jsonl, compare to manifest.json,
                            report MATCH / MISMATCH / MISSING / UNMANIFESTED.
                            Exit non-zero if anything is off.
  --reconcile      Rewrite manifest.json sha256 + doc_count for MISMATCH sources
                   to the values recomputed from the real bytes (prints a diff
                   first). Fixes a stale pin (e.g. the qmsum checksum GA-1 found
                   never matched on disk) — only ever from real bytes, never
                   fabricated.
  --freeze         After a clean verify, compute the snapshot (per-source
                   sha256 + total_docs + combined corpus_hash + snapshot_id) and
                   write tests/corpus/snapshot.json — the pinned basis IR-B/IR-C
                   consume. Prints the corpus_hash to paste into the gold set.
  --reproduce P    Re-hash the current corpus and assert it is bit-identical to a
                   prior snapshot record P (the COR-2 step-4 determinism gate).
                   On PASS, stamps reproduced_bit_identical=true in P.

A snapshot that will not reproduce bit-identically is NOT frozen — `--reproduce`
is the gate that proves it.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import json
import sys
from pathlib import Path

# Reuse the corpus lib's repo-root / data-dir resolution so paths match the
# acquisition scripts exactly.
from _corpus_lib import corpus_data_dir, corpus_doc_dir, repo_root

# raw/*.jsonl files that are produced deterministically but NOT pinned in
# manifest.json (synthetic generators, not upstream acquisitions). They are
# expected members of the frozen corpus, just not manifest contracts.
UNMANIFESTED_EXPECTED = {"chain_connectives"}

SNAPSHOT_PATH = corpus_doc_dir() / "snapshot.json"
MANIFEST_PATH = corpus_doc_dir() / "scripts" / "manifest.json"


def _sha256_and_count(path: Path) -> tuple[str, int]:
    """Return (sha256 of the file bytes, line count). Matches write_jsonl,
    which hashes exactly the bytes it writes line by line."""
    hasher = hashlib.sha256()
    count = 0
    with path.open("rb") as f:
        for line in f:
            hasher.update(line)
            if line.strip():
                count += 1
    return hasher.hexdigest(), count


def _raw_dir() -> Path:
    return corpus_data_dir() / "raw"


def _load_manifest() -> dict:
    return json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))


def _manifest_sources(manifest: dict) -> dict[str, dict]:
    return manifest.get("sources", {})


class Row:
    """One source's verify result."""

    def __init__(self, name: str, path: Path) -> None:
        self.name = name
        self.path = path
        self.status = ""          # MATCH / MISMATCH / MISSING / UNMANIFESTED / UNKNOWN
        self.disk_sha: str | None = None
        self.disk_count: int | None = None
        self.manifest_sha: str | None = None
        self.manifest_count: int | None = None


def verify(manifest: dict) -> list[Row]:
    """Hash on-disk raw/*.jsonl and reconcile against the manifest."""
    sources = _manifest_sources(manifest)
    raw = _raw_dir()
    rows: list[Row] = []
    seen_files: set[Path] = set()

    # 1) Every manifest source must exist on disk and match its pin.
    for name, entry in sources.items():
        out = repo_root() / entry["output"]
        row = Row(name, out)
        row.manifest_sha = entry.get("sha256")
        row.manifest_count = entry.get("doc_count")
        if not out.exists():
            row.status = "MISSING"
        else:
            seen_files.add(out.resolve())
            row.disk_sha, row.disk_count = _sha256_and_count(out)
            row.status = "MATCH" if row.disk_sha == row.manifest_sha else "MISMATCH"
        rows.append(row)

    # 2) Any raw/*.jsonl not in the manifest (e.g. chain_connectives).
    if raw.exists():
        for f in sorted(raw.glob("*.jsonl")):
            if f.resolve() in seen_files:
                continue
            stem = f.stem
            row = Row(stem, f)
            row.disk_sha, row.disk_count = _sha256_and_count(f)
            row.status = "UNMANIFESTED" if stem in UNMANIFESTED_EXPECTED else "UNKNOWN"
            rows.append(row)

    return rows


def print_report(rows: list[Row]) -> None:
    width = max((len(r.name) for r in rows), default=4)
    print(f"\n{'SOURCE':<{width}}  {'STATUS':<13} {'DOCS':>7}  SHA256")
    print("-" * (width + 13 + 9 + 66))
    for r in rows:
        sha = (r.disk_sha or r.manifest_sha or "—")[:16]
        docs = r.disk_count if r.disk_count is not None else (r.manifest_count or 0)
        print(f"{r.name:<{width}}  {r.status:<13} {docs:>7}  {sha}…")
        if r.status == "MISMATCH":
            print(f"{'':<{width}}    manifest: {r.manifest_sha[:16]}…  ({r.manifest_count} docs)")
            print(f"{'':<{width}}    on-disk : {r.disk_sha[:16]}…  ({r.disk_count} docs)")
    total = sum(r.disk_count or 0 for r in rows if r.status not in ("MISSING",))
    print("-" * (width + 13 + 9 + 66))
    print(f"{'TOTAL':<{width}}  {'':<13} {total:>7}  docs on disk\n")


def reconcile(manifest: dict, rows: list[Row]) -> bool:
    """Rewrite manifest sha256/doc_count for MISMATCH sources from real bytes."""
    sources = _manifest_sources(manifest)
    changed = False
    for r in rows:
        if r.status != "MISMATCH":
            continue
        entry = sources[r.name]
        print(f"reconcile {r.name}: sha256 {entry.get('sha256','—')[:16]}… -> {r.disk_sha[:16]}…"
              f"  doc_count {entry.get('doc_count')} -> {r.disk_count}")
        entry["sha256"] = r.disk_sha
        entry["doc_count"] = r.disk_count
        entry.pop("sha256_reconcile", None)  # clear any stale-pin marker
        changed = True
    if changed:
        MANIFEST_PATH.write_text(
            json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
        )
        print(f"\nwrote {MANIFEST_PATH.relative_to(repo_root())}")
    else:
        print("nothing to reconcile — all manifest pins already match disk.")
    return changed


def _snapshot_members(rows: list[Row]) -> list[Row]:
    return [r for r in rows if r.status in ("MATCH", "UNMANIFESTED")]


def _corpus_hash(members: list[Row]) -> str:
    """Order-independent combined hash over 'name:sha256' lines."""
    body = "\n".join(f"{r.name}:{r.disk_sha}" for r in sorted(members, key=lambda r: r.name))
    return hashlib.sha256(body.encode("utf-8")).hexdigest()


def freeze(rows: list[Row], corpus_version: str, allow_unknown: bool) -> int:
    blockers = [r for r in rows if r.status in ("MISSING", "MISMATCH")]
    unknown = [r for r in rows if r.status == "UNKNOWN"]
    if blockers:
        print("REFUSING TO FREEZE — resolve these first "
              "(--reconcile fixes stale pins, re-acquire fixes MISSING):", file=sys.stderr)
        for r in blockers:
            print(f"  {r.status}: {r.name}", file=sys.stderr)
        return 2
    if unknown and not allow_unknown:
        print("REFUSING TO FREEZE — unrecognized raw files (pass --allow-unknown to "
              "include, or add them to manifest.json / UNMANIFESTED_EXPECTED):", file=sys.stderr)
        for r in unknown:
            print(f"  UNKNOWN: {r.name}", file=sys.stderr)
        return 2

    members = _snapshot_members(rows)
    if allow_unknown:
        members += [r for r in rows if r.status == "UNKNOWN"]
    chash = _corpus_hash(members)
    snapshot_id = f"{corpus_version}-{chash[:12]}"
    record = {
        "snapshot_id": snapshot_id,
        "corpus_version": corpus_version,
        "corpus_hash": chash,
        "created_at": _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "total_docs": sum(r.disk_count or 0 for r in members),
        "source_count": len(members),
        "per_source_sha256": [
            {"source": r.name, "sha256": r.disk_sha, "doc_count": r.disk_count}
            for r in sorted(members, key=lambda r: r.name)
        ],
        "reproduced_bit_identical": None,
        "generator": "tests/corpus/scripts/freeze_corpus.py",
    }
    SNAPSHOT_PATH.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(f"\nFROZE snapshot {snapshot_id}")
    print(f"  total_docs   : {record['total_docs']} across {record['source_count']} sources")
    print(f"  corpus_hash  : {chash}")
    print(f"  wrote        : {SNAPSHOT_PATH.relative_to(repo_root())}")
    print("\nNEXT: confirm determinism with")
    print(f"      python tests/corpus/scripts/freeze_corpus.py --reproduce {SNAPSHOT_PATH.relative_to(repo_root())}")
    print("THEN: set the gold set's corpus_hash to the value above "
          "(replaces the TODO(COR-2-freeze) placeholder).")
    return 0


def reproduce(rows: list[Row], snapshot_file: Path) -> int:
    prior = json.loads(snapshot_file.read_text(encoding="utf-8"))
    members = _snapshot_members(rows) + [r for r in rows if r.status == "UNKNOWN"]
    now_hash = _corpus_hash([r for r in members if r.disk_sha])
    expected = prior["corpus_hash"]
    by_name = {r.name: r for r in members}

    ok = now_hash == expected
    print(f"\nreproduce {prior['snapshot_id']}: corpus_hash "
          f"{'MATCH' if ok else 'MISMATCH'}")
    print(f"  expected: {expected}")
    print(f"  current : {now_hash}")
    if not ok:
        for entry in prior["per_source_sha256"]:
            r = by_name.get(entry["source"])
            if r is None:
                print(f"  DROPPED  {entry['source']}")
            elif r.disk_sha != entry["sha256"]:
                print(f"  CHANGED  {entry['source']}: {entry['sha256'][:16]}… -> {r.disk_sha[:16]}…")
        live = {r.name for r in members if r.disk_sha}
        for extra in sorted(live - {e["source"] for e in prior["per_source_sha256"]}):
            print(f"  ADDED    {extra}")
        print("\nNOT REPRODUCIBLE — this corpus is NOT frozen. Re-assemble from the "
              "manifest pins and retry; if it still differs, HALT + escalate.", file=sys.stderr)
        return 1

    prior["reproduced_bit_identical"] = True
    snapshot_file.write_text(json.dumps(prior, indent=2) + "\n", encoding="utf-8")
    print("  reproduced_bit_identical = true (stamped). Corpus is FROZEN.")
    return 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--reconcile", action="store_true",
                   help="rewrite manifest sha256/doc_count for MISMATCH sources from real bytes")
    g.add_argument("--freeze", action="store_true",
                   help="write tests/corpus/snapshot.json after a clean verify")
    g.add_argument("--reproduce", metavar="SNAPSHOT", help="assert bit-identical to a prior snapshot record")
    ap.add_argument("--corpus-version", default="0.8.x-B",
                    help="corpus version label baked into the snapshot id (default: 0.8.x-B)")
    ap.add_argument("--allow-unknown", action="store_true",
                    help="include UNKNOWN (unrecognized) raw files in the freeze")
    args = ap.parse_args(argv)

    raw = _raw_dir()
    if not raw.exists():
        print(f"ERROR: {raw} does not exist. The corpus data is gitignored — run the "
              "acquire_*.py scripts or restore the CI cache first, and run this where "
              "the data actually lives.", file=sys.stderr)
        return 2

    manifest = _load_manifest()
    rows = verify(manifest)
    print_report(rows)

    if args.reconcile:
        reconcile(manifest, rows)
        return 0
    if args.freeze:
        return freeze(rows, args.corpus_version, args.allow_unknown)
    if args.reproduce:
        return reproduce(rows, Path(args.reproduce))

    # default: verify — non-zero if anything needs attention
    bad = [r for r in rows if r.status in ("MISMATCH", "MISSING", "UNKNOWN")]
    if bad:
        print(f"VERIFY FAILED — {len(bad)} source(s) need attention "
              "(--reconcile for stale pins, re-acquire for MISSING).")
        return 1
    print("VERIFY OK — every raw source matches its manifest pin.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
