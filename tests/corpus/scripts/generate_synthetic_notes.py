#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Generate 1200 synthetic personal-knowledge notes.

Eight styles x 150 notes per style. Notes are plain markdown; each is
deterministic from a fixed seed + index. Entities (people, projects,
papers, technologies) are drawn from a fixed vocabulary chosen so the
chain generator (Corpus-Pack 2) can layer cross-modal chains over
these notes by referencing the same names that appear in
acquire_landes_todos.py (projects/assignees) and the Enron user dirs
(senders).

Provenance: synthetic:fathomdb-corpus-v1.

Styles (per research doc §1.6):
  - fleeting        short stream-of-consciousness capture
  - project         status / next steps for an ongoing initiative
  - reading         paper or article notes + key claim + open question
  - idea            sketch of a hypothesis or product idea
  - decision-log    "decided X over Y because Z" with reversal hooks
  - someday-maybe   loosely-scoped aspirations
  - personal-crm    notes on a person, last contact, follow-ups
  - meeting-follow  recap + action items derived from a meeting

Determinism: a re-run with the same SEED produces a bit-identical
JSONL.
"""

from __future__ import annotations

import hashlib
import random
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import CorpusDoc, corpus_data_dir, doc_id, write_jsonl  # noqa: E402

PROVENANCE = "synthetic:fathomdb-corpus-v1"
LICENSE_SPDX = "Apache-2.0"  # project license; same as the FathomDB repo
SEED = 0x53EEDFA7C012B0F1  # locked
PER_STYLE = 150
TARGET_COUNT = PER_STYLE * 8

# Vocabularies — must be stable across runs.
# People: union of synthetic names + a sample of real Enron-user-style
# handles, so Corpus-Pack 2 chains can link Enron threads to synthetic
# notes by name.
PEOPLE_SYN = [
    "alex.barker", "j.tanaka", "priya.iyer", "morgan.lee", "noor.haddad",
    "sam.becker", "kira.osei", "luca.romano", "ines.duarte", "mei.chen",
]
PEOPLE_ENRON = [
    "phillip.allen", "jeff.dasovich", "kay.mann", "sara.shackleton",
    "vince.kaminski", "kenneth.lay",
]
PEOPLE = PEOPLE_SYN + PEOPLE_ENRON

PROJECTS = [
    "Q1-launch", "Q2-roadmap", "vendor-onboarding", "compliance-audit",
    "hiring", "tax-prep", "anniversary", "research-reading", "personal",
    "household",
]

TECHNOLOGIES = [
    "sqlite-vec", "FathomDB", "Rust async", "tokio", "Arrow",
    "duckdb", "Parquet", "FAISS", "HNSW", "OpenSearch",
]

PAPERS = [
    ("ColBERT: Efficient Passage Search via Late Interaction", "Khattab & Zaharia, 2020"),
    ("Dense Passage Retrieval for Open-Domain QA", "Karpukhin et al., 2020"),
    ("RAGGED: Towards Informed Design of RAG Systems", "Anonymous, 2024"),
    ("Lost in the Middle", "Liu et al., 2023"),
    ("Atlas: Few-shot Learning with Retrieval-Augmented LMs", "Izacard et al., 2022"),
]


def _seeded(idx: int, style: str) -> random.Random:
    h = hashlib.sha256(f"{SEED}|{style}|{idx}".encode()).digest()
    return random.Random(int.from_bytes(h[:8], "big"))


def _date(rng: random.Random, anchor: datetime, span_days: int) -> str:
    offset = rng.randint(0, span_days)
    hour = rng.randint(7, 22)
    minute = rng.randint(0, 59)
    return (anchor + timedelta(days=offset, hours=hour, minutes=minute)).isoformat()


# Each style returns (title, body, tags, people_mentions, project_mentions).
def style_fleeting(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    proj = rng.choice(PROJECTS)
    tech = rng.choice(TECHNOLOGIES)
    person = rng.choice(PEOPLE)
    body = rng.choice([
        f"quick thought — what if we tried {tech} for {proj}? would simplify the ingest path.",
        f"reminder: ping {person} about {proj} sync before EOD.",
        f"observation: {tech} latency spikes correlate with {proj} backfill window — investigate.",
        f"idea park: blend {tech} with our existing pipeline for {proj}. low-risk experiment.",
    ])
    return f"fleeting — {proj}", body, ["style:fleeting", f"project:{proj}", f"tech:{tech}"], [person], [proj]


def style_project(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    proj = rng.choice(PROJECTS)
    ppl = rng.sample(PEOPLE, 2)
    tech = rng.choice(TECHNOLOGIES)
    body = (
        f"# {proj} status\n\n"
        f"Owner: {ppl[0]}. Backup: {ppl[1]}.\n\n"
        f"## Done this week\n- migrated {proj} backend to {tech}\n- closed three blocker bugs\n\n"
        f"## Next\n- finalize the rollout plan with {ppl[1]}\n- write the {proj} runbook\n- decide on the {tech} pinned version\n"
    )
    return f"{proj} — weekly status", body, ["style:project", f"project:{proj}", f"tech:{tech}"], ppl, [proj]


def style_reading(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    paper, cite = rng.choice(PAPERS)
    tech = rng.choice(TECHNOLOGIES)
    proj = rng.choice(PROJECTS)
    body = (
        f"# reading: {paper}\n\n"
        f"_{cite}_\n\n"
        f"**Key claim.** The authors argue that late-interaction retrieval gives most of the gain of cross-encoders at a fraction of the cost.\n\n"
        f"**Why I care.** Possible win for {proj} if we can swap the dense retriever for a {tech}-friendly variant.\n\n"
        f"**Open question.** How does this hold up when the corpus is multi-modal (notes + emails + meeting transcripts)?\n"
    )
    return f"reading: {paper[:50]}", body, ["style:reading", f"project:{proj}", f"tech:{tech}"], [], [proj]


def style_idea(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    tech = rng.choice(TECHNOLOGIES)
    proj = rng.choice(PROJECTS)
    body = (
        f"# idea sketch — {tech} + {proj}\n\n"
        f"**Hypothesis.** Combining {tech} with our current architecture would cut p99 latency by ~30%.\n\n"
        f"**Smallest possible experiment.** Single-day prototype. Reuse the {proj} fixture.\n\n"
        f"**Way it could be wrong.** {tech} may not handle our partition_key cardinality well; fall back if so.\n"
    )
    return f"idea: {tech} for {proj}", body, ["style:idea", f"project:{proj}", f"tech:{tech}"], [], [proj]


def style_decision(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    proj = rng.choice(PROJECTS)
    chosen, rejected = rng.sample(TECHNOLOGIES, 2)
    person = rng.choice(PEOPLE)
    body = (
        f"# decision log — {proj} retrieval backend\n\n"
        f"**Decision.** Going with {chosen} for {proj}.\n\n"
        f"**Considered.** {rejected} was the close runner-up.\n\n"
        f"**Why {chosen}.** Better fit for our existing toolchain; smaller dependency surface; {person} has prior production experience.\n\n"
        f"**Reversal condition.** If p99 on the canonical benchmark regresses by >20% over a release cycle, revisit.\n"
    )
    return f"decision: {chosen} for {proj}", body, ["style:decision-log", f"project:{proj}", f"tech:{chosen}", f"tech-rejected:{rejected}"], [person], [proj]


def style_someday(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    tech = rng.choice(TECHNOLOGIES)
    body = rng.choice([
        f"someday: rewrite the ingest path on top of {tech}. probably not this year.",
        f"someday/maybe: a talk on {tech} at a local meetup. defer until after the {rng.choice(PROJECTS)} cutover.",
        f"someday: write up the {tech} migration as a blog post. low priority, no deadline.",
    ])
    return f"someday — {tech}", body, ["style:someday-maybe", f"tech:{tech}"], [], []


def style_crm(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    person = rng.choice(PEOPLE)
    proj = rng.choice(PROJECTS)
    body = (
        f"# CRM: {person}\n\n"
        f"**Last contact.** Email about {proj} — they followed up the same day.\n\n"
        f"**Context.** Owns the upstream side of {proj}; good first call for any escalations.\n\n"
        f"**Next.** Schedule a sync about the {proj} cutover before the freeze.\n"
    )
    return f"CRM — {person}", body, ["style:personal-crm", f"person:{person}", f"project:{proj}"], [person], [proj]


def style_meeting_follow(rng: random.Random) -> tuple[str, str, list[str], list[str], list[str]]:
    proj = rng.choice(PROJECTS)
    attendees = rng.sample(PEOPLE, 3)
    body = (
        f"# {proj} sync — follow-ups\n\n"
        f"Attendees: {', '.join(attendees)}.\n\n"
        f"## Decisions\n- ship the {proj} migration behind a feature flag\n- defer the schema change to next release\n\n"
        f"## Action items\n- [ ] {attendees[0]}: draft the rollback plan\n- [ ] {attendees[1]}: confirm the on-call rotation\n- [ ] {attendees[2]}: post the runbook PR\n"
    )
    return f"{proj} sync — follow-ups", body, ["style:meeting-follow", f"project:{proj}"], attendees, [proj]


STYLES = [
    ("fleeting", style_fleeting),
    ("project", style_project),
    ("reading", style_reading),
    ("idea", style_idea),
    ("decision-log", style_decision),
    ("someday-maybe", style_someday),
    ("personal-crm", style_crm),
    ("meeting-follow", style_meeting_follow),
]

ANCHOR = datetime(2025, 1, 1, tzinfo=timezone.utc)
SPAN_DAYS = 540  # ~18 months


def main() -> int:
    docs: list[CorpusDoc] = []
    for style_name, fn in STYLES:
        for i in range(PER_STYLE):
            rng = _seeded(i, style_name)
            title, body, tags, ppl, projs = fn(rng)
            native_id = f"{style_name}#{i:04d}"
            docs.append(CorpusDoc(
                doc_id=doc_id(PROVENANCE, native_id),
                source_type="note",
                title=title,
                body=body,
                created_at=_date(rng, ANCHOR, SPAN_DAYS),
                modified_at=None,
                author_or_sender="me",
                recipients=[],
                people_mentions=ppl,
                project_mentions=projs,
                tags=sorted(set(tags + ["synthetic"])),
                url_or_external_id=f"synthetic:{native_id}",
                thread_id=None,
                parent_doc_id=None,
                license=LICENSE_SPDX,
                provenance=PROVENANCE,
            ))

    out_path = corpus_data_dir() / "raw" / "synthetic_notes.jsonl"
    count, sha = write_jsonl(out_path, docs)
    print(f"wrote {count} docs to {out_path}")
    print(f"sha256 = {sha}")
    return 0 if count == TARGET_COUNT else 1


if __name__ == "__main__":
    raise SystemExit(main())
