"""S1 — gold basis: verify the gold set before anything measures against it.

Pure: no SDK, no database, no network. One question -- "may this gold be used,
and under what identity?" -- answered with either a typed identity or a typed
blocker, never a partial result or a silent default.

Because EARP never gates FathomDB, nothing downstream will catch a wrong gold
basis. These refusals are the only thing between a stale cache and a confident
false number.

Design of record: `dev/design/earp-slice-1-design.md`.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Mapping

from eval.earp.schema.models import Blocker, BlockerCode, CorpusIdentity, GoldIdentity

#: Closed vocabularies, mirroring the Rust reference. An unknown value is a
#: hard error there (`ir_eval.rs:95-105,292-296,301-311`) and is refused here.
QUERY_CLASSES = frozenset(
    {"commitment", "action", "exact_fact", "preference", "exploratory", "negative"}
)
NECESSITIES = frozenset({"required", "supporting"})
QUERY_ORIGINS = frozenset({"human_dataset", "llm_generated", "templated"})

#: The reference defaults this when the field is absent (`ir_eval.rs:292-296`).
DEFAULT_QUERY_ORIGIN = "human_dataset"

#: Keys consumed into typed fields; everything else lands in `extra`.
_TYPED_QUERY_KEYS = frozenset(
    {
        "query",
        "query_id",
        "query_class",
        "required_evidence",
        "expected_top_k_doc_ids",
        "relation_type",
        "chain_shape",
        "source",
        "answer_type",
        "query_origin",
        "_source",
        "_answer_type",
    }
)


class _Malformed(Exception):
    """Internal: a gold document that does not conform. Carries the specific
    defect so the blocker names it rather than saying 'invalid'."""


@dataclass(frozen=True)
class EvidenceUnit:
    evidence_id: str
    doc_id: str
    necessity: str
    #: Optional in the reference (`ir_eval.rs:170`); today every real unit is
    #: `{"kind": "whole_body"}` and none carries spans.
    locator: Mapping[str, Any] | None = None


@dataclass(frozen=True)
class GoldQuery:
    query_id: str
    query: str
    query_class: str
    required_evidence: tuple[EvidenceUnit, ...]
    expected_top_k_doc_ids: tuple[str, ...] = ()
    relation_type: str | None = None
    chain_shape: str | None = None
    source: str | None = None
    answer_type: str | None = None
    query_origin: str = DEFAULT_QUERY_ORIGIN
    #: Genuinely unknown keys, retained rather than refused: IR-B defines the
    #: gold schema as an additive superset owned upstream. Frozen dataclasses
    #: have fixed fields, so retention needs this explicit slot.
    extra: Mapping[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class GoldSet:
    corpus_hash: str
    qrels_version: str
    queries: tuple[GoldQuery, ...]
    #: Names the generator and the reuse tier -- provenance this slice exists
    #: to preserve (`ir_eval.rs:213`).
    note: str | None = None


@dataclass(frozen=True)
class GoldVerification:
    """Either `blocker` is set, or the three identities are. Never both, never
    neither."""

    blocker: Blocker | None = None
    corpus_identity: CorpusIdentity | None = None
    gold_identity: GoldIdentity | None = None
    gold_set: GoldSet | None = None


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _blocked(code: BlockerCode, message: str, stage: str, **detail: Any) -> GoldVerification:
    return GoldVerification(
        blocker=Blocker(code=code, message=message, stage=stage, detail=dict(detail))
    )


def _str_or_none(raw: Mapping[str, Any], *names: str) -> str | None:
    """First present string among `names` -- the v2 name, then the `_`-prefixed
    legacy fallback the reference implements (`ir_eval.rs:284-291`)."""
    for name in names:
        value = raw.get(name)
        if isinstance(value, str):
            return value
    return None


def _parse_evidence(raw: Any, query_id: str) -> EvidenceUnit:
    if not isinstance(raw, dict):
        raise _Malformed(f"query {query_id}: evidence unit is not an object")
    for key in ("evidence_id", "doc_id", "necessity"):
        if not isinstance(raw.get(key), str):
            raise _Malformed(f"query {query_id}: evidence unit missing `{key}`")
    necessity = raw["necessity"]
    if necessity not in NECESSITIES:
        raise _Malformed(f"query {query_id}: unknown necessity `{necessity}`")
    locator = raw.get("locator")
    if locator is not None and not isinstance(locator, dict):
        raise _Malformed(f"query {query_id}: locator is not an object")
    return EvidenceUnit(
        evidence_id=raw["evidence_id"],
        doc_id=raw["doc_id"],
        necessity=necessity,
        locator=locator,
    )


def _parse_query(raw: Any, index: int) -> GoldQuery:
    if not isinstance(raw, dict):
        raise _Malformed(f"query #{index} is not an object")

    # Optional in the reference, but the pairing key for comparisons -- refused
    # here rather than letting S8's pairing degrade silently.
    query_id = raw.get("query_id")
    if not isinstance(query_id, str) or not query_id:
        raise _Malformed(f"query #{index} missing `query_id`")

    query = raw.get("query")
    if not isinstance(query, str):
        raise _Malformed(f"query {query_id}: missing `query`")

    query_class = raw.get("query_class")
    if not isinstance(query_class, str):
        raise _Malformed(f"query {query_id}: missing `query_class`")
    if query_class not in QUERY_CLASSES:
        raise _Malformed(f"query {query_id}: unknown query_class `{query_class}`")

    query_origin = raw.get("query_origin", DEFAULT_QUERY_ORIGIN)
    if not isinstance(query_origin, str) or query_origin not in QUERY_ORIGINS:
        raise _Malformed(f"query {query_id}: unknown query_origin `{query_origin}`")

    evidence_raw = raw.get("required_evidence", [])
    if not isinstance(evidence_raw, list):
        raise _Malformed(f"query {query_id}: `required_evidence` is not an array")

    doc_ids = raw.get("expected_top_k_doc_ids", [])
    if not isinstance(doc_ids, list):
        raise _Malformed(f"query {query_id}: `expected_top_k_doc_ids` is not an array")

    return GoldQuery(
        query_id=query_id,
        query=query,
        query_class=query_class,
        required_evidence=tuple(_parse_evidence(u, query_id) for u in evidence_raw),
        expected_top_k_doc_ids=tuple(str(d) for d in doc_ids),
        relation_type=_str_or_none(raw, "relation_type"),
        chain_shape=_str_or_none(raw, "chain_shape"),
        source=_str_or_none(raw, "source", "_source"),
        answer_type=_str_or_none(raw, "answer_type", "_answer_type"),
        query_origin=query_origin,
        extra={k: v for k, v in raw.items() if k not in _TYPED_QUERY_KEYS},
    )


def _parse_gold(document: Any) -> GoldSet:
    if not isinstance(document, dict):
        raise _Malformed("gold document is not an object")
    for key in ("corpus_hash", "qrels_version"):
        if not isinstance(document.get(key), str):
            raise _Malformed(f"gold document missing `{key}`")
    queries = document.get("queries")
    if not isinstance(queries, list):
        raise _Malformed("gold document missing `queries` array")
    note = document.get("note")
    return GoldSet(
        corpus_hash=document["corpus_hash"],
        qrels_version=document["qrels_version"],
        queries=tuple(_parse_query(q, i) for i, q in enumerate(queries)),
        note=note if isinstance(note, str) else None,
    )


def verify_gold(
    *,
    gold_path: Path,
    snapshot_path: Path,
    expected_sha256: str,
    expected_corpus_hash: str,
    expected_qrels_version: str,
    data_root: Path | None = None,
    manifest_path: Path | None = None,
) -> GoldVerification:
    """Verify the gold basis. Checks run in a load-bearing order; the first
    failure returns.

    The content pin precedes every semantic check: a file whose bytes are not
    the pinned bytes cannot have its fields trusted at all, so reading
    `corpus_hash` out of an unverified file and reporting a mismatch would name
    the wrong defect.
    """
    # 1 -- the gitignored data tree is absent (the normal worktree case).
    if data_root is not None and not Path(data_root).is_dir():
        return _blocked(
            BlockerCode.CORPUS_ROOT_ABSENT,
            f"configured corpus data_root does not exist: {data_root}",
            stage="gold.resolve",
            data_root=str(data_root),
        )

    gold_path = Path(gold_path)
    if not gold_path.is_file():
        return _blocked(
            BlockerCode.GOLD_MISSING,
            f"gold file does not exist: {gold_path}",
            stage="gold.resolve",
            path=str(gold_path),
        )

    # 3 -- the content pin, BEFORE anything reads a field out of the file.
    actual_sha = _sha256_file(gold_path)
    if actual_sha != expected_sha256:
        return _blocked(
            BlockerCode.GOLD_HASH_MISMATCH,
            f"gold sha256 {actual_sha} does not match the pin {expected_sha256}",
            stage="gold.pin",
            path=str(gold_path),
            expected=expected_sha256,
            actual=actual_sha,
        )

    try:
        gold_set = _parse_gold(json.loads(gold_path.read_text(encoding="utf-8")))
    except json.JSONDecodeError as exc:
        return _blocked(
            BlockerCode.GOLD_MALFORMED,
            f"gold file is not valid JSON: {exc}",
            stage="gold.parse",
            path=str(gold_path),
        )
    except _Malformed as exc:
        return _blocked(
            BlockerCode.GOLD_MALFORMED,
            f"gold file does not conform: {exc}",
            stage="gold.parse",
            path=str(gold_path),
        )

    # 5 -- the snapshot must be readable before its field can be compared.
    snapshot_path = Path(snapshot_path)
    snapshot_corpus_hash: str | None = None
    if snapshot_path.is_file():
        try:
            snapshot_doc = json.loads(snapshot_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            snapshot_doc = None
        if isinstance(snapshot_doc, dict):
            candidate = snapshot_doc.get("corpus_hash")
            snapshot_corpus_hash = candidate if isinstance(candidate, str) else None
    if snapshot_corpus_hash is None:
        return _blocked(
            BlockerCode.SNAPSHOT_UNREADABLE,
            f"corpus snapshot is absent, unreadable, or carries no `corpus_hash`: {snapshot_path}",
            stage="gold.snapshot",
            path=str(snapshot_path),
        )

    # 6 -- three-way: the declared pin, the gold's own field, the snapshot's.
    if len({gold_set.corpus_hash, snapshot_corpus_hash, expected_corpus_hash}) != 1:
        return _blocked(
            BlockerCode.GOLD_CORPUS_MISMATCH,
            "corpus hash disagreement between the declared pin, the gold set, and the snapshot",
            stage="gold.corpus",
            declared=expected_corpus_hash,
            gold=gold_set.corpus_hash,
            snapshot=snapshot_corpus_hash,
        )

    # 7 -- exact match against the declared version. A denylist would fail open
    # at v3; this fails closed and needs no maintenance. Provenance hygiene, not
    # a metric trap: v2 is semantically identical to v1 for this corpus.
    if gold_set.qrels_version != expected_qrels_version:
        return _blocked(
            BlockerCode.GOLD_STALE_QRELS_VERSION,
            (
                f"gold declares qrels_version `{gold_set.qrels_version}` but the configuration "
                f"pins `{expected_qrels_version}`; regenerate with "
                f"tests/corpus/scripts/build_ir_gold.py"
            ),
            stage="gold.version",
            found=gold_set.qrels_version,
            expected=expected_qrels_version,
        )

    manifest = Path(manifest_path) if manifest_path is not None else None
    corpus_identity = CorpusIdentity(
        snapshot_path=str(snapshot_path),
        # The snapshot FILE's hash -- distinct from the `corpus_hash` FIELD
        # read out of its body, which is the gold's pin.
        snapshot_sha256=_sha256_file(snapshot_path),
        manifest_path=str(manifest) if manifest else None,
        manifest_sha256=_sha256_file(manifest) if manifest and manifest.is_file() else None,
        data_root=str(data_root) if data_root else None,
    )
    gold_identity = GoldIdentity(
        path=str(gold_path),
        sha256=actual_sha,
        corpus_hash=gold_set.corpus_hash,
        qrels_version=gold_set.qrels_version,
        #: Total, including negatives.
        query_count=len(gold_set.queries),
    )
    return GoldVerification(
        corpus_identity=corpus_identity,
        gold_identity=gold_identity,
        gold_set=gold_set,
    )


__all__ = [
    "DEFAULT_QUERY_ORIGIN",
    "NECESSITIES",
    "QUERY_CLASSES",
    "QUERY_ORIGINS",
    "EvidenceUnit",
    "GoldQuery",
    "GoldSet",
    "GoldVerification",
    "verify_gold",
]
