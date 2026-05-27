#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Corpus-Pack 2: synthetic cross-document chain generator.

Produces ~200 multi-document chains that exercise FathomDB's
cross-modal retrieval story. Each chain weaves together a small set
of real-data anchor documents (from Corpus-Pack 1's JSONLs) with
synthetic connective documents (notes / emails / todos) that
reference the anchors by doc_id and surface a known ground-truth
retrieval expectation.

Outputs:

  tests/corpus/chains/<chain_id>.json   (one file per chain, committed)
  data/corpus-data/raw/chain_connectives.jsonl (synthetic connectives, gitignored)

Chain shapes (deterministic rotation across 6 shapes for 200 chains
=> ~33 per shape):

  EMAIL -> NOTE -> TODO                  (Enron anchor)
  ARTICLE -> NOTE -> EMAIL               (CNN/DM anchor)
  MEETING -> TODO -> NOTE                (QMSum anchor)
  EMAIL -> MEETING -> TODO               (Enron + QMSum anchors)
  ARTICLE -> NOTE -> TODO                (CNN/DM anchor)
  TODO -> NOTE -> EMAIL                  (Landes anchor)

Relation vocabulary (locked in corpus-card.md):
  replies_to, follows_up_on, summarizes, action_from,
  contradicts, mentions, cites.

Determinism: every random choice is keyed off SEED + chain_id, so a
re-run produces a bit-identical chain_connectives.jsonl and the same
set of <chain_id>.json files.

Volume cap (handoff §"Out of scope"): synthetic content must not
exceed 20% of total corpus by doc count. With Pack 1 at ~7,300 real
docs the cap is ~1,460 synthetic-chain docs; 200 chains * ~3
connectives ~ 600 docs sits well under it.
"""

from __future__ import annotations

import hashlib
import json
import random
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Iterable

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import (  # noqa: E402
    CorpusDoc,
    corpus_data_dir,
    corpus_doc_dir,
    doc_id,
    write_jsonl,
)

PROVENANCE = "synthetic-chain:fathomdb-corpus-v1"
LICENSE_SPDX = "Apache-2.0"  # project license
SEED = 0xC4A1_C0A0_C4A1_AB1E
TARGET_CHAINS = 200
CONNECTIVES_OUT = "chain_connectives.jsonl"

# Locked relation vocabulary (mirror of corpus-card.md).
RELATION_TYPES = {
    "replies_to", "follows_up_on", "summarizes", "action_from",
    "contradicts", "mentions", "cites",
}

# Anchor-time window used when synthetic docs need an explicit date.
ANCHOR = datetime(2025, 6, 1, tzinfo=timezone.utc)


# ---------------------------------------------------------------------------
# Real-doc index: load existing Pack-1 JSONLs once, grouped by source_type.
# ---------------------------------------------------------------------------

SOURCES = (
    ("enron",            "email"),
    ("enronqa",          "email"),
    ("cnn_dailymail",    "article"),
    ("bahmutov_dailylogs", "note"),
    ("synthetic_notes",  "note"),
    ("landes_todos",     "todo"),
    ("qmsum",            "meeting"),
)


def load_anchors() -> dict[str, list[dict]]:
    """Load Pack-1 JSONLs, group by source_type, sort by doc_id for determinism."""
    raw_dir = corpus_data_dir() / "raw"
    by_type: dict[str, list[dict]] = {}
    for fname, expected_type in SOURCES:
        path = raw_dir / f"{fname}.jsonl"
        if not path.exists():
            print(f"WARN: {path} missing — chains needing {expected_type} may be skipped",
                  file=sys.stderr)
            continue
        with path.open() as f:
            for line in f:
                d = json.loads(line)
                if d.get("source_type") != expected_type:
                    continue
                by_type.setdefault(expected_type, []).append(d)
    for t, lst in by_type.items():
        lst.sort(key=lambda d: d["doc_id"])
        print(f"  {t}: {len(lst)} anchors")
    return by_type


def chain_rng(chain_id: str) -> random.Random:
    h = hashlib.sha256(f"{SEED}|{chain_id}".encode()).digest()
    return random.Random(int.from_bytes(h[:8], "big"))


def pick(rng: random.Random, anchors: list[dict]) -> dict:
    return anchors[rng.randrange(len(anchors))]


def chain_date(rng: random.Random, offset_days: int) -> str:
    drift_hours = rng.randint(0, 23)
    drift_minutes = rng.randint(0, 59)
    return (ANCHOR + timedelta(days=offset_days, hours=drift_hours, minutes=drift_minutes)).isoformat()


def synth_doc(
    *,
    chain_id: str,
    role: str,                # "note" / "email" / "todo" inside the chain
    source_type: str,
    title: str,
    body: str,
    created_at: str,
    parent_doc_id: str | None,
    thread_id: str | None,
    people: list[str],
    projects: list[str],
    extra_tags: Iterable[str] = (),
) -> CorpusDoc:
    native_id = f"{chain_id}:{role}"
    tags = sorted({"synthetic-chain", f"chain:{chain_id}", f"role:{role}"} | set(extra_tags))
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, native_id),
        source_type=source_type,  # type: ignore[arg-type]
        title=title,
        body=body,
        created_at=created_at,
        modified_at=None,
        author_or_sender="me" if source_type == "note" else None,
        recipients=[],
        people_mentions=people,
        project_mentions=projects,
        tags=tags,
        url_or_external_id=f"synthetic-chain:{native_id}",
        thread_id=thread_id,
        parent_doc_id=parent_doc_id,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


# ---------------------------------------------------------------------------
# Salient-bit extractors per anchor source_type.
# ---------------------------------------------------------------------------

def anchor_topic(anchor: dict) -> str:
    """One short topic string per anchor, used in synthetic prose."""
    title = anchor.get("title") or ""
    if title:
        return title.strip().split("\n", 1)[0][:120]
    # Fallback: first 80 chars of body, single-line.
    body = (anchor.get("body") or "").strip().replace("\n", " ")
    return body[:120] or "the matter at hand"


def anchor_person(anchor: dict) -> str | None:
    if anchor.get("author_or_sender"):
        return anchor["author_or_sender"]
    if anchor.get("people_mentions"):
        return anchor["people_mentions"][0]
    return None


def anchor_project(anchor: dict, fallback: str = "general") -> str:
    if anchor.get("project_mentions"):
        return anchor["project_mentions"][0]
    return fallback


# ---------------------------------------------------------------------------
# Chain builders. Each returns (synthetic_docs, ground_truth_queries,
# all_chain_doc_ids, chain_shape).
# ---------------------------------------------------------------------------

def chain_email_note_todo(chain_id: str, rng: random.Random, anchors: dict) -> dict | None:
    if not anchors.get("email") or not anchors.get("note"):
        return None
    email = pick(rng, anchors["email"])
    topic = anchor_topic(email)
    sender = anchor_person(email) or "the sender"
    project = anchor_project(email, fallback="inbox")

    note_body = (
        f"# follow-up on \"{topic}\"\n\n"
        f"{sender} sent a message about this. The relevant points:\n\n"
        f"- worth a closer look this week\n"
        f"- linked to project: {project}\n\n"
        f"**Decision.** We'll act on the proposal as stated; revisit if blockers surface.\n"
    )
    note = synth_doc(
        chain_id=chain_id, role="note", source_type="note",
        title=f"follow-up: {topic[:60]}",
        body=note_body,
        created_at=chain_date(rng, 1),
        parent_doc_id=email["doc_id"],
        thread_id=email.get("thread_id"),
        people=[sender] if sender != "the sender" else [],
        projects=[project],
        extra_tags=("relation:summarizes",),
    )
    todo_body = (
        f"action from {topic}: follow up with {sender}.\n\n"
        f"Project: {project}\nPriority: P1\nDue: within the week.\n"
    )
    todo = synth_doc(
        chain_id=chain_id, role="todo", source_type="todo",
        title=f"follow up on {topic[:60]}",
        body=todo_body,
        created_at=chain_date(rng, 2),
        parent_doc_id=note.doc_id,
        thread_id=email.get("thread_id"),
        people=[sender] if sender != "the sender" else [],
        projects=[project],
        extra_tags=("relation:action_from",),
    )
    chain_ids = [email["doc_id"], note.doc_id, todo.doc_id]
    queries = [
        {
            "query": f"what was decided after the email about \"{topic[:80]}\"?",
            "expected_top_k_doc_ids": chain_ids,
            "relation_type": "summarizes",
        },
        {
            "query": f"what's the follow-up action from {sender}?",
            "expected_top_k_doc_ids": [todo.doc_id, note.doc_id],
            "relation_type": "action_from",
        },
    ]
    return {
        "shape": "EMAIL->NOTE->TODO",
        "anchors": [email["doc_id"]],
        "synthetic": [note, todo],
        "chain_ids": chain_ids,
        "queries": queries,
    }


def chain_article_note_email(chain_id: str, rng: random.Random, anchors: dict) -> dict | None:
    if not anchors.get("article") or not anchors.get("note"):
        return None
    article = pick(rng, anchors["article"])
    body0 = (article.get("body") or "").strip().split(". ")
    headline = body0[0][:120] if body0 else "(headline)"
    project = "research-reading"
    teammate = "alex.barker"

    note_body = (
        f"# reading: {headline}\n\n"
        f"saved this article — worth circulating to the {project} group.\n\n"
        f"- punchline: {body0[0][:200] if body0 else ''}\n"
        f"- ties into our ongoing thread on {project}\n"
    )
    note = synth_doc(
        chain_id=chain_id, role="note", source_type="note",
        title=f"reading: {headline[:60]}",
        body=note_body,
        created_at=chain_date(rng, 0),
        parent_doc_id=article["doc_id"],
        thread_id=None,
        people=[],
        projects=[project],
        extra_tags=("relation:summarizes",),
    )
    email_body = (
        f"Subject: FYI — {headline[:80]}\nFrom: me\nTo: {teammate}\n\n"
        f"Saw this article and thought of the {project} thread.\n\n"
        f"My notes are pinned; key bit: {body0[0][:200] if body0 else ''}\n"
    )
    email = synth_doc(
        chain_id=chain_id, role="email", source_type="email",
        title=f"FYI — {headline[:60]}",
        body=email_body,
        created_at=chain_date(rng, 1),
        parent_doc_id=note.doc_id,
        thread_id=None,
        people=[teammate],
        projects=[project],
        extra_tags=("relation:mentions",),
    )
    chain_ids = [article["doc_id"], note.doc_id, email.doc_id]
    queries = [
        {
            "query": f"what's the context around \"{headline[:80]}\"?",
            "expected_top_k_doc_ids": chain_ids,
            "relation_type": "mentions",
        },
        {
            "query": f"what did I send {teammate} about {project}?",
            "expected_top_k_doc_ids": [email.doc_id, note.doc_id],
            "relation_type": "mentions",
        },
    ]
    return {
        "shape": "ARTICLE->NOTE->EMAIL",
        "anchors": [article["doc_id"]],
        "synthetic": [note, email],
        "chain_ids": chain_ids,
        "queries": queries,
    }


def chain_meeting_todo_note(chain_id: str, rng: random.Random, anchors: dict) -> dict | None:
    if not anchors.get("meeting") or not anchors.get("note"):
        return None
    meeting = pick(rng, anchors["meeting"])
    topic = anchor_topic(meeting)
    project = anchor_project(meeting, fallback="meeting-followups")

    todo_body = (
        f"action item from \"{topic[:80]}\": draft the rollback plan.\n\n"
        f"Project: {project}\nPriority: P1\nDue: end of next week.\n"
    )
    todo = synth_doc(
        chain_id=chain_id, role="todo", source_type="todo",
        title=f"draft rollback plan ({topic[:50]})",
        body=todo_body,
        created_at=chain_date(rng, 1),
        parent_doc_id=meeting["doc_id"],
        thread_id=meeting.get("thread_id"),
        people=[],
        projects=[project],
        extra_tags=("relation:action_from",),
    )
    note_body = (
        f"# {project} — follow-up\n\n"
        f"Re: meeting on \"{topic[:120]}\".\n\n"
        f"Action item logged. **Update:** on second thought we should pursue option B instead — the meeting's preferred option carries more rollback risk than I initially weighed.\n"
    )
    note = synth_doc(
        chain_id=chain_id, role="note", source_type="note",
        title=f"{project} — meeting follow-up",
        body=note_body,
        created_at=chain_date(rng, 3),
        parent_doc_id=todo.doc_id,
        thread_id=meeting.get("thread_id"),
        people=[],
        projects=[project],
        extra_tags=("relation:contradicts",),
    )
    chain_ids = [meeting["doc_id"], todo.doc_id, note.doc_id]
    queries = [
        {
            "query": f"what did we decide in the meeting about \"{topic[:80]}\"?",
            "expected_top_k_doc_ids": chain_ids,
            "relation_type": "summarizes",
        },
        {
            "query": f"did we end up reversing the {project} decision?",
            "expected_top_k_doc_ids": [note.doc_id, meeting["doc_id"]],
            "relation_type": "contradicts",
        },
    ]
    return {
        "shape": "MEETING->TODO->NOTE(contradicts)",
        "anchors": [meeting["doc_id"]],
        "synthetic": [todo, note],
        "chain_ids": chain_ids,
        "queries": queries,
    }


def chain_email_meeting_todo(chain_id: str, rng: random.Random, anchors: dict) -> dict | None:
    if not anchors.get("email") or not anchors.get("meeting"):
        return None
    email = pick(rng, anchors["email"])
    meeting = pick(rng, anchors["meeting"])
    sender = anchor_person(email) or "the sender"
    project = anchor_project(meeting, fallback="cross-thread")

    todo_body = (
        f"From the meeting that {sender}'s email led to: \"{anchor_topic(meeting)[:80]}\".\n\n"
        f"Action: close the loop with {sender} on the {project} decision.\n"
        f"Project: {project}\nPriority: P1\n"
    )
    todo = synth_doc(
        chain_id=chain_id, role="todo", source_type="todo",
        title=f"close loop with {sender}",
        body=todo_body,
        created_at=chain_date(rng, 2),
        parent_doc_id=meeting["doc_id"],
        thread_id=email.get("thread_id"),
        people=[sender] if sender != "the sender" else [],
        projects=[project],
        extra_tags=("relation:follows_up_on",),
    )
    chain_ids = [email["doc_id"], meeting["doc_id"], todo.doc_id]
    queries = [
        {
            "query": f"what's the action item from {sender}'s thread that ended in a meeting?",
            "expected_top_k_doc_ids": chain_ids,
            "relation_type": "follows_up_on",
        },
    ]
    return {
        "shape": "EMAIL->MEETING->TODO",
        "anchors": [email["doc_id"], meeting["doc_id"]],
        "synthetic": [todo],
        "chain_ids": chain_ids,
        "queries": queries,
    }


def chain_article_note_todo(chain_id: str, rng: random.Random, anchors: dict) -> dict | None:
    if not anchors.get("article") or not anchors.get("note"):
        return None
    article = pick(rng, anchors["article"])
    body0 = (article.get("body") or "").strip().split(". ")
    headline = body0[0][:120] if body0 else "(headline)"
    project = "research-reading"

    note_body = (
        f"# reading: {headline}\n\n"
        f"Key takeaway worth following up on. Punchline: {body0[0][:200] if body0 else ''}\n"
    )
    note = synth_doc(
        chain_id=chain_id, role="note", source_type="note",
        title=f"reading: {headline[:60]}",
        body=note_body,
        created_at=chain_date(rng, 0),
        parent_doc_id=article["doc_id"],
        thread_id=None,
        people=[],
        projects=[project],
        extra_tags=("relation:summarizes",),
    )
    todo_body = (
        f"follow up on the article \"{headline[:80]}\" — verify the claim in the second paragraph.\n\n"
        f"Project: {project}\nPriority: P2\n"
    )
    todo = synth_doc(
        chain_id=chain_id, role="todo", source_type="todo",
        title=f"verify claim from {headline[:50]}",
        body=todo_body,
        created_at=chain_date(rng, 2),
        parent_doc_id=note.doc_id,
        thread_id=None,
        people=[],
        projects=[project],
        extra_tags=("relation:action_from",),
    )
    chain_ids = [article["doc_id"], note.doc_id, todo.doc_id]
    queries = [
        {
            "query": f"what's on my to-do list from the article about \"{headline[:80]}\"?",
            "expected_top_k_doc_ids": chain_ids,
            "relation_type": "action_from",
        },
    ]
    return {
        "shape": "ARTICLE->NOTE->TODO",
        "anchors": [article["doc_id"]],
        "synthetic": [note, todo],
        "chain_ids": chain_ids,
        "queries": queries,
    }


def chain_todo_note_email(chain_id: str, rng: random.Random, anchors: dict) -> dict | None:
    if not anchors.get("todo") or not anchors.get("note"):
        return None
    todo = pick(rng, anchors["todo"])
    topic = anchor_topic(todo)
    project = anchor_project(todo, fallback="personal")
    teammate = anchor_person(todo) or "the assignee"

    note_body = (
        f"# {project} status\n\n"
        f"Working through the to-do: {topic[:120]}. Some blockers cropping up; need input from {teammate}.\n"
    )
    note = synth_doc(
        chain_id=chain_id, role="note", source_type="note",
        title=f"{project} — status on {topic[:50]}",
        body=note_body,
        created_at=chain_date(rng, 1),
        parent_doc_id=todo["doc_id"],
        thread_id=None,
        people=[teammate] if teammate != "the assignee" else [],
        projects=[project],
        extra_tags=("relation:follows_up_on",),
    )
    email_body = (
        f"Subject: status check — {topic[:80]}\nFrom: me\nTo: {teammate}\n\n"
        f"Hey {teammate}, quick status check on \"{topic[:120]}\". Bumping into a couple blockers — when's a good time to sync?\n"
    )
    email = synth_doc(
        chain_id=chain_id, role="email", source_type="email",
        title=f"status check — {topic[:50]}",
        body=email_body,
        created_at=chain_date(rng, 2),
        parent_doc_id=note.doc_id,
        thread_id=None,
        people=[teammate] if teammate != "the assignee" else [],
        projects=[project],
        extra_tags=("relation:mentions",),
    )
    chain_ids = [todo["doc_id"], note.doc_id, email.doc_id]
    queries = [
        {
            "query": f"what's the status of the {project} to-do about {topic[:80]}?",
            "expected_top_k_doc_ids": chain_ids,
            "relation_type": "follows_up_on",
        },
    ]
    return {
        "shape": "TODO->NOTE->EMAIL",
        "anchors": [todo["doc_id"]],
        "synthetic": [note, email],
        "chain_ids": chain_ids,
        "queries": queries,
    }


CHAIN_BUILDERS = [
    chain_email_note_todo,
    chain_article_note_email,
    chain_meeting_todo_note,
    chain_email_meeting_todo,
    chain_article_note_todo,
    chain_todo_note_email,
]


# ---------------------------------------------------------------------------
# Main.
# ---------------------------------------------------------------------------

def main() -> int:
    print("loading Pack-1 anchors...")
    anchors = load_anchors()
    if not anchors:
        print("ERROR: no anchors available; run Pack-1 acquisition scripts first",
              file=sys.stderr)
        return 2

    chains_dir = corpus_doc_dir() / "chains"
    chains_dir.mkdir(parents=True, exist_ok=True)

    connectives: list[CorpusDoc] = []
    written_chain_files = 0
    skipped = 0
    for i in range(TARGET_CHAINS):
        builder = CHAIN_BUILDERS[i % len(CHAIN_BUILDERS)]
        shape_short = builder.__name__.replace("chain_", "")
        chain_id = f"chain-{shape_short}-{i:04d}"
        rng = chain_rng(chain_id)
        result = builder(chain_id, rng, anchors)
        if result is None:
            skipped += 1
            continue
        # Validate relation types referenced in queries.
        for q in result["queries"]:
            if q["relation_type"] not in RELATION_TYPES:
                raise AssertionError(
                    f"chain {chain_id} uses unknown relation_type {q['relation_type']}"
                )
        chain_record = {
            "chain_id": chain_id,
            "chain_shape": result["shape"],
            "doc_ids": result["chain_ids"],
            "anchor_doc_ids": result["anchors"],
            "synthetic_doc_ids": [d.doc_id for d in result["synthetic"]],
            "ground_truth_queries": result["queries"],
        }
        with (chains_dir / f"{chain_id}.json").open("w") as f:
            json.dump(chain_record, f, indent=2, sort_keys=True)
            f.write("\n")
        written_chain_files += 1
        connectives.extend(result["synthetic"])

    print(f"wrote {written_chain_files} chain files to {chains_dir}")
    print(f"skipped {skipped} chains (missing source anchors)")

    out_path = corpus_data_dir() / "raw" / CONNECTIVES_OUT
    # write_jsonl is deterministic across runs but ordering of connectives
    # follows the chain iteration order (already deterministic via the
    # range loop above).
    count, sha = write_jsonl(out_path, connectives)
    print(f"wrote {count} synthetic-chain docs to {out_path}")
    print(f"sha256 = {sha}")

    # Sanity: synthetic content cap.
    print(f"synthetic-chain doc count: {count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
