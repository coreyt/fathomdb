#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire Landes/Di Eugenio imperative to-do corpus + synthesize variants.

Source:  https://github.com/plandes/todo-task, MIT license.
         Resource file resources/todo-dataset.json (JSONL of 253 rows).
Pinned:  commit SHA 06bcd261fe09e767282c73bf59a480a71bd8d26f (2018-06-29).

The raw corpus has 253 rows — too small for the Version-B target of 500
todos. Per the research doc §1.5 / handoff Corpus-Pack 1, the gap is
closed by synthesising metadata (project / assignee / due date / priority)
deterministically from the real text. We emit:

  - 253 real-text todos with synthesised metadata fields,
  - 247 additional variants that remix the same real texts with different
    synthesised metadata,
  total 500 docs.

Synthesised content has provenance "github:plandes/todo-task@<sha>+synth".
"""

from __future__ import annotations

import hashlib
import json
import sys
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import CorpusDoc, corpus_dir, doc_id, write_jsonl  # noqa: E402

UPSTREAM_REPO = "plandes/todo-task"
UPSTREAM_SHA = "06bcd261fe09e767282c73bf59a480a71bd8d26f"
RAW_URL = (
    f"https://raw.githubusercontent.com/{UPSTREAM_REPO}/{UPSTREAM_SHA}"
    "/resources/todo-dataset.json"
)
PROVENANCE_REAL = f"github:{UPSTREAM_REPO}@{UPSTREAM_SHA}"
PROVENANCE_SYNTH = PROVENANCE_REAL + "+synth"
LICENSE_SPDX = "MIT"
TARGET_COUNT = 500

# Fixed synthesis vocabulary — drawn from the corpus card's locked
# source_type set + plausible knowledge-worker contexts. Anchors here are
# chosen so a downstream chain generator (Corpus-Pack 2) can link Landes
# todos to Enron senders / QMSum projects without per-source coupling.
PROJECTS = [
    "Q1-launch", "Q2-roadmap", "vendor-onboarding", "personal",
    "household", "research-reading", "hiring", "compliance-audit",
    "tax-prep", "anniversary",
]
ASSIGNEES = [
    "me", "alex.barker", "j.tanaka", "priya.iyer", "morgan.lee",
    "noor.haddad", "team",
]
PRIORITIES = ["P0", "P1", "P2", "P3"]

# Synthesised due-date window: relative to a fixed anchor so two runs
# produce identical timestamps.
ANCHOR = datetime(2026, 1, 1, tzinfo=timezone.utc)
WINDOW_DAYS = 180


def _hashbytes(*parts: str) -> bytes:
    h = hashlib.sha256()
    for p in parts:
        h.update(p.encode("utf-8"))
        h.update(b"\x00")
    return h.digest()


def _pick(seq, *parts: str):
    idx = int.from_bytes(_hashbytes(*parts)[:8], "big") % len(seq)
    return seq[idx]


def synth_metadata(native_id: str, variant: int) -> dict:
    """Deterministic synthesised metadata for a given row + variant."""
    salt = f"{native_id}|v{variant}"
    project = _pick(PROJECTS, salt, "project")
    assignee = _pick(ASSIGNEES, salt, "assignee")
    priority = _pick(PRIORITIES, salt, "priority")
    day_offset = int.from_bytes(_hashbytes(salt, "due")[:4], "big") % WINDOW_DAYS
    due = (ANCHOR + timedelta(days=day_offset)).date().isoformat()
    created_offset = int.from_bytes(_hashbytes(salt, "created")[:4], "big") % WINDOW_DAYS
    created = (ANCHOR + timedelta(days=created_offset, hours=9)).isoformat()
    return {
        "project": project,
        "assignee": assignee,
        "priority": priority,
        "due": due,
        "created_at": created,
    }


def download_raw() -> list[dict]:
    print(f"fetching {RAW_URL}")
    with urllib.request.urlopen(RAW_URL) as resp:
        text = resp.read().decode("utf-8")
    rows = [json.loads(line) for line in text.splitlines() if line.strip()]
    return rows


def build_real_doc(row: dict) -> CorpusDoc:
    native_id = str(row["id"])
    text = row["instance"]["panon"]["text"]
    label = row["class-label"]
    md = synth_metadata(native_id, variant=0)
    body = f"{text}\n\nProject: {md['project']}\nAssignee: {md['assignee']}\nDue: {md['due']}\nPriority: {md['priority']}"
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE_REAL, native_id),
        source_type="todo",
        title=text[:80],
        body=body,
        created_at=md["created_at"],
        modified_at=None,
        author_or_sender=md["assignee"],
        recipients=[],
        people_mentions=[md["assignee"]] if md["assignee"] != "team" else [],
        project_mentions=[md["project"]],
        tags=["intent:" + label, "priority:" + md["priority"], "set:" + row.get("set-type", "?")],
        url_or_external_id=f"landes:{native_id}",
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE_REAL + "+synth-metadata",
    )


def build_variant_doc(row: dict, variant: int) -> CorpusDoc:
    native_id = str(row["id"])
    text = row["instance"]["panon"]["text"]
    label = row["class-label"]
    md = synth_metadata(native_id, variant=variant)
    # Variant body remixes the same real text under different metadata —
    # exercises duplicate-detection + per-project filtering paths.
    body = f"{text}\n\nProject: {md['project']}\nAssignee: {md['assignee']}\nDue: {md['due']}\nPriority: {md['priority']}"
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE_SYNTH, f"{native_id}#v{variant}"),
        source_type="todo",
        title=text[:80],
        body=body,
        created_at=md["created_at"],
        modified_at=None,
        author_or_sender=md["assignee"],
        recipients=[],
        people_mentions=[md["assignee"]] if md["assignee"] != "team" else [],
        project_mentions=[md["project"]],
        tags=["intent:" + label, "priority:" + md["priority"], "variant:" + str(variant)],
        url_or_external_id=f"landes:{native_id}#v{variant}",
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE_SYNTH,
    )


def main() -> int:
    rows = download_raw()
    print(f"loaded {len(rows)} real Landes rows")
    out_path = corpus_dir() / "raw" / "landes_todos.jsonl"

    docs: list[CorpusDoc] = [build_real_doc(r) for r in rows]
    # Generate variants deterministically by walking rows in order; each
    # successive variant index produces fresh metadata for the same text.
    variant = 1
    while len(docs) < TARGET_COUNT:
        row = rows[(len(docs) - len(rows)) % len(rows)]
        if (len(docs) - len(rows)) > 0 and (len(docs) - len(rows)) % len(rows) == 0:
            variant += 1
        docs.append(build_variant_doc(row, variant))

    count, sha = write_jsonl(out_path, docs)
    print(f"wrote {count} docs ({len(rows)} real + {count - len(rows)} variants) to {out_path}")
    print(f"sha256 = {sha}")
    return 0 if count == TARGET_COUNT else 1


if __name__ == "__main__":
    raise SystemExit(main())
