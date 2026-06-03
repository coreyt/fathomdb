"""Shared helpers for FathomDB 0.7.0 corpus acquisition scripts.

Provides the canonical document schema, deterministic ID hashing, and
JSONL writers. Every acquisition script in this directory emits documents
through `write_jsonl` so the on-disk shape is identical across sources.

See tests/corpus/corpus-card.md for the authoritative schema + license
posture.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Iterable, Literal

SourceType = Literal["email", "meeting", "paper", "article", "note", "todo"]

SOURCE_TYPES: tuple[SourceType, ...] = (
    "email", "meeting", "paper", "article", "note", "todo",
)

RELATION_TYPES: tuple[str, ...] = (
    "replies_to", "follows_up_on", "summarizes", "action_from",
    "contradicts", "mentions", "cites",
)


@dataclass
class CorpusDoc:
    """Canonical document shape — see corpus-card.md §"Document schema"."""

    doc_id: str
    source_type: SourceType
    title: str | None
    body: str
    created_at: str           # ISO-8601 UTC
    modified_at: str | None
    author_or_sender: str | None
    recipients: list[str]
    people_mentions: list[str]
    project_mentions: list[str]
    tags: list[str]
    url_or_external_id: str | None
    thread_id: str | None
    parent_doc_id: str | None
    license: str              # SPDX identifier
    provenance: str           # short upstream tag

    def to_jsonl(self) -> str:
        return json.dumps(asdict(self), ensure_ascii=False, sort_keys=True)


def doc_id(provenance: str, source_native_id: str) -> str:
    """Deterministic doc_id = first 16 hex chars of SHA-256(provenance|id)."""
    h = hashlib.sha256(f"{provenance}|{source_native_id}".encode("utf-8"))
    return h.hexdigest()[:16]


def write_jsonl(path: Path, docs: Iterable[CorpusDoc]) -> tuple[int, str]:
    """Write docs to JSONL deterministically; return (count, sha256)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    count = 0
    with path.open("w", encoding="utf-8") as f:
        for d in docs:
            line = d.to_jsonl() + "\n"
            f.write(line)
            hasher.update(line.encode("utf-8"))
            count += 1
    return count, hasher.hexdigest()


QaAnswerType = Literal["span", "free_form", "yes_no_maybe", "summary", "abstain"]
EvalQaSource = Literal["qaconv", "qasper", "enronqa", "qmsum", "pmc_oa"]


@dataclass
class EvidenceSpan:
    doc_id: str
    start: int
    end: int
    text: str | None = None


@dataclass
class EvalQaRow:
    qa_id: str
    source: EvalQaSource
    source_type: SourceType
    question: str
    answers: list[str]
    answer_type: QaAnswerType
    evidence_doc_ids: list[str]
    evidence_spans: list[EvidenceSpan]
    negative_doc_ids: list[str]
    relation_type: str | None
    metadata: dict[str, Any]
    license: str
    provenance: str

    def to_jsonl(self) -> str:
        self.validate()
        return json.dumps(asdict(self), ensure_ascii=False, sort_keys=True)

    def validate(self) -> None:
        if not self.qa_id:
            raise ValueError("qa_id is required")
        if not self.question.strip():
            raise ValueError(f"{self.qa_id}: question is required")
        if not self.answers and self.answer_type != "abstain":
            raise ValueError(f"{self.qa_id}: answers are required unless answer_type=abstain")
        if self.source_type not in SOURCE_TYPES:
            raise ValueError(f"{self.qa_id}: unknown source_type {self.source_type}")
        if self.relation_type is not None and self.relation_type not in RELATION_TYPES:
            raise ValueError(f"{self.qa_id}: unknown relation_type {self.relation_type}")
        for span in self.evidence_spans:
            if span.start < 0 or span.end < span.start:
                raise ValueError(f"{self.qa_id}: invalid evidence span {span.start}:{span.end}")
            if span.doc_id not in self.evidence_doc_ids:
                raise ValueError(f"{self.qa_id}: evidence span doc_id is not listed")


def qa_id(source: str, *parts: str) -> str:
    h = hashlib.sha256("|".join((source, *parts)).encode("utf-8"))
    return h.hexdigest()[:24]


def assert_grounded_evidence(row: EvalQaRow, available_doc_ids: set[str]) -> None:
    missing = sorted(set(row.evidence_doc_ids) - available_doc_ids)
    if missing:
        raise ValueError(f"{row.qa_id}: evidence_doc_ids not present in corpus: {missing}")


def write_eval_jsonl(path: Path, rows: Iterable[EvalQaRow]) -> tuple[int, str]:
    path.parent.mkdir(parents=True, exist_ok=True)
    hasher = hashlib.sha256()
    count = 0
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            line = row.to_jsonl() + "\n"
            f.write(line)
            hasher.update(line.encode("utf-8"))
            count += 1
    return count, hasher.hexdigest()


def repo_root() -> Path:
    """Find repo root by walking up from this file looking for .git."""
    here = Path(__file__).resolve()
    for parent in here.parents:
        if (parent / ".git").exists():
            return parent
    raise RuntimeError("could not locate repo root from " + str(here))


def corpus_data_dir() -> Path:
    """Root of produced corpus data (raw downloads + per-source JSONL).

    Lives at <repo>/data/corpus-data/ and is .gitignored — acquisition
    scripts here are the reproducible source of truth, the data they
    produce is not tracked. CI fetches/restores into this directory.
    """
    return repo_root() / "data" / "corpus-data"


def corpus_doc_dir() -> Path:
    """Documentation / spec layout (corpus-card.md, chain definitions)."""
    return repo_root() / "tests" / "corpus"
