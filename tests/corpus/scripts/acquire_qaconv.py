#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire QAConv conversation docs and eval-only QA rows.

Source:  github.com/salesforce/QAConv.
Pinned:  commit b1f140c39580dd4dadb4ecd35e9a247a90016407.
License: BSD-3-Clause.

QAConv contains QA over informative conversations, including business
emails, panel discussions, and work channels. Conversation segments are
ingested as corpus documents; question/answer rows are written to the
eval artifact and are not ingested as documents.

Determinism: QA files are read in train, validation, test order, sorted
by split, inferred source, segment id, then QA id. The selected segment
set is balanced round-robin by inferred source_type before docs and QA
rows are emitted.
"""

from __future__ import annotations

import io
import json
import sys
import urllib.request
import zipfile
from collections import defaultdict
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import (  # noqa: E402
    CorpusDoc,
    EvalQaRow,
    corpus_data_dir,
    doc_id,
    qa_id,
    write_eval_jsonl,
    write_jsonl,
)

UPSTREAM_REPO = "salesforce/QAConv"
UPSTREAM_SHA = "b1f140c39580dd4dadb4ecd35e9a247a90016407"
ZIP_URL = (
    "https://raw.githubusercontent.com/"
    f"{UPSTREAM_REPO}/{UPSTREAM_SHA}/dataset/QAConv-V1.1.zip"
)
PROVENANCE = f"github:{UPSTREAM_REPO}@{UPSTREAM_SHA[:8]}"
LICENSE_SPDX = "BSD-3-Clause"
TARGET_DOCS = 1250
TARGET_QA_ROWS = 5000
SPLITS = (("train", "trn.json"), ("validation", "val.json"), ("test", "tst.json"))


def fetch_zip() -> bytes:
    print(f"fetching {ZIP_URL}", flush=True)
    with urllib.request.urlopen(ZIP_URL) as resp:
        return resp.read()


def _read_json(zf: zipfile.ZipFile, basename: str) -> Any:
    matches = [n for n in zf.namelist() if n.endswith("/" + basename) or n == basename]
    if not matches:
        raise FileNotFoundError(basename)
    with zf.open(matches[0]) as f:
        return json.loads(f.read().decode("utf-8"))


def infer_source_type(segment_id: str, full_ids: list[str]) -> str:
    joined = " ".join([segment_id, *full_ids]).lower()
    if "email" in joined or "mali" in joined:
        return "email"
    if "channel" in joined or "slack" in joined or "chat" in joined:
        return "note"
    return "meeting"


def render_turns(turns: list[dict[str, Any]]) -> tuple[str, list[str]]:
    lines: list[str] = []
    speakers: list[str] = []
    for turn in turns:
        speaker = str(turn.get("speaker") or "Speaker").strip()
        text = str(turn.get("text") or "").strip()
        if not text:
            continue
        speakers.append(speaker)
        lines.append(f"{speaker}: {text}")
    return "\n".join(lines), sorted(set(speakers))


def build_doc(segment_id: str, segment: dict[str, Any], source_type: str, split: str) -> CorpusDoc | None:
    prev = segment.get("prev_ctx") or []
    dialog = segment.get("seg_dialog") or []
    body, speakers = render_turns([*prev, *dialog])
    if not body:
        return None
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, segment_id),
        source_type=source_type,  # type: ignore[arg-type]
        title=f"QAConv conversation {segment_id}",
        body=body,
        created_at="2022-03-13T00:00:00+00:00",
        modified_at=None,
        author_or_sender=None,
        recipients=[],
        people_mentions=speakers,
        project_mentions=[],
        tags=["qaconv", "qaconv-split:" + split, "qaconv-source-type:" + source_type],
        url_or_external_id=f"qaconv:{segment_id}",
        thread_id=segment_id,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


def answer_type(qa: dict[str, Any]) -> str:
    answers = [str(a).strip().lower() for a in qa.get("answers", [])]
    if not answers or answers == ["unanswerable"] or answers == ["cannotanswer"]:
        return "abstain"
    return "free_form" if qa.get("QG") else "span"


def build_qa_row(qa: dict[str, Any], split: str, doc: CorpusDoc, source_type: str) -> EvalQaRow:
    answers = [str(a).strip() for a in qa.get("answers", []) if str(a).strip()]
    atype = answer_type(qa)
    if atype == "abstain":
        answers = []
    upstream_id = str(qa.get("id") or qa.get("question_id") or qa.get("question"))
    return EvalQaRow(
        qa_id=qa_id("qaconv", doc.thread_id or doc.doc_id, upstream_id),
        source="qaconv",
        source_type=source_type,  # type: ignore[arg-type]
        question=str(qa.get("question") or "").strip(),
        answers=answers,
        answer_type=atype,  # type: ignore[arg-type]
        evidence_doc_ids=[doc.doc_id],
        evidence_spans=[],
        negative_doc_ids=[],
        relation_type="mentions",
        metadata={
            "split": split,
            "thread_id": doc.thread_id,
            "upstream_id": upstream_id,
            "article_full_id": qa.get("article_full_id"),
            "qg": qa.get("QG"),
        },
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


def main() -> int:
    archive = fetch_zip()
    with zipfile.ZipFile(io.BytesIO(archive)) as zf:
        segments = _read_json(zf, "article_segment.json")
        candidates: dict[str, list[tuple[str, str, dict[str, Any]]]] = defaultdict(list)
        for split, filename in SPLITS:
            for qa in _read_json(zf, filename):
                segment_id = str(qa.get("article_segment_id") or "")
                if not segment_id or segment_id not in segments:
                    continue
                full_ids = [str(x) for x in (qa.get("article_full_id") or [])]
                source_type = infer_source_type(segment_id, full_ids)
                candidates[source_type].append((split, segment_id, qa))
        for source_type in candidates:
            candidates[source_type].sort(key=lambda x: (x[0], x[1], str(x[2].get("id") or "")))

        selected_segments: list[tuple[str, str, str]] = []
        seen_segments: set[str] = set()
        while len(selected_segments) < TARGET_DOCS:
            progressed = False
            for source_type in sorted(candidates):
                while candidates[source_type]:
                    split, segment_id, _qa = candidates[source_type].pop(0)
                    if segment_id in seen_segments:
                        continue
                    seen_segments.add(segment_id)
                    selected_segments.append((split, segment_id, source_type))
                    progressed = True
                    break
            if not progressed:
                break

        selected = {seg for _split, seg, _stype in selected_segments}
        doc_by_segment: dict[str, CorpusDoc] = {}
        docs: list[CorpusDoc] = []
        for split, segment_id, source_type in selected_segments:
            d = build_doc(segment_id, segments[segment_id], source_type, split)
            if d is None:
                continue
            docs.append(d)
            doc_by_segment[segment_id] = d

        qa_rows: list[EvalQaRow] = []
        for split, filename in SPLITS:
            rows = sorted(_read_json(zf, filename), key=lambda q: (str(q.get("article_segment_id") or ""), str(q.get("id") or "")))
            for qa in rows:
                if len(qa_rows) >= TARGET_QA_ROWS:
                    break
                segment_id = str(qa.get("article_segment_id") or "")
                if segment_id not in selected or segment_id not in doc_by_segment:
                    continue
                d = doc_by_segment[segment_id]
                qa_rows.append(build_qa_row(qa, split, d, d.source_type))

    raw_path = corpus_data_dir() / "raw" / "qaconv.jsonl"
    eval_path = corpus_data_dir() / "eval" / "qaconv_qa.jsonl"
    doc_count, doc_sha = write_jsonl(raw_path, docs)
    qa_count, qa_sha = write_eval_jsonl(eval_path, qa_rows)
    print(f"wrote {doc_count} docs to {raw_path}")
    print(f"sha256 = {doc_sha}")
    print(f"wrote {qa_count} eval QA rows to {eval_path}")
    print(f"eval sha256 = {qa_sha}")
    return 0 if doc_count > 0 and qa_count > 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
