"""S4 — the durable writer.

Makes a partial run impossible to mistake for a complete one, by owning the
ordering (stage and validate sidecars, materialize the shared record, append
the index last) and the run identity that ordering depends on.

`_lib.write_record` materializes and appends in ONE call with no hook between
them, so the ordering is only achievable by pre-deriving the identity, staging
into the derived directory, then calling `write_record` with a byte-identical
config document and the same `ts` so it recomputes the same id.

Design of record: `dev/design/earp-slice-4-design.md`.
"""

from __future__ import annotations

import copy
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping, Sequence

from eval.earp._experiments import lib as _lib
from eval.earp.schema import PER_QUERY_SCHEMA_PATH, RESULT_SCHEMA_PATH
from eval.earp.schema.models import (
    SCHEMA_VERSION_PER_QUERY,
    SCHEMA_VERSION_RESULT,
    Blocker,
    BlockerCode,
    RunVerdict,
)
from eval.earp.schema.validate import validate

_RESULT_SCHEMA: dict[str, Any] = json.loads(RESULT_SCHEMA_PATH.read_text(encoding="utf-8"))
_PER_QUERY_SCHEMA: dict[str, Any] = json.loads(
    PER_QUERY_SCHEMA_PATH.read_text(encoding="utf-8")
)

SIDECAR_NAME = "earp.result.v1.json"
PER_QUERY_NAME = "earp.per-query.v1.jsonl"


@dataclass(frozen=True)
class WriteOutcome:
    run_id: str
    run_dir: Path | None = None
    blocker: Blocker | None = None


def _canonical(document: Mapping[str, Any]) -> str:
    return json.dumps(document, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def write_run(
    *,
    experiment: str,
    ts: datetime,
    config_doc: Mapping[str, Any],
    experiments_root: Path,
    verdict: RunVerdict,
    read: str,
    metrics: Mapping[str, Any],
    code: Mapping[str, Any],
    env: Mapping[str, Any],
    corpus: Mapping[str, Any],
    seeds: Mapping[str, Any],
    cost_usd: float,
    per_query: Sequence[Mapping[str, Any]] = (),
    sidecar: Mapping[str, Any] | None = None,
    sidecar_blockers: Sequence[Mapping[str, Any]] = (),
    tdd_evidence: Mapping[str, Any] | None = None,
    n: int | None = None,
    headline: Mapping[str, Any] | None = None,
) -> WriteOutcome:
    """Stage, validate, claim, write, materialize, append.

    `config_doc` must be the RAW config document -- the same object S3 hashed.
    Passing a `ResolvedScenario` would hash differently and then fail
    destructively: `record.json` writes, and `yaml.safe_dump` raises on the
    enums afterwards, leaving a half-materialized directory.
    """
    if not isinstance(verdict, RunVerdict):
        raise ValueError(
            f"verdict must be a pinned RunVerdict token, got {verdict!r}; "
            f"Record.verdict is an untyped str, so the vocabulary is enforced here"
        )
    if not isinstance(config_doc, Mapping):
        raise TypeError("config_doc must be the raw config document (a mapping)")
    if ts.tzinfo is None or ts.utcoffset() != timezone.utc.utcoffset(None):
        raise ValueError(
            "ts must be UTC: _lib stamps a literal 'Z' without converting, so a "
            "naive or local timestamp yields a run_id that lies about its timezone"
        )

    # `_resolved_dict` raises on a Mapping that is not a dict, and returns the
    # SAME object rather than a copy -- so copy here, or a later mutation of the
    # caller's document would silently change the directory.
    document = copy.deepcopy(dict(config_doc))
    sha = _lib.config_sha256(document)
    run_id = _lib.make_run_id(experiment, ts, sha)
    root = Path(experiments_root)
    run_dir = root / "runs" / run_id

    # 1-2. Serialize, then validate the PARSED TEXT -- validating the object
    # graph would not tell us the bytes on disk are trustworthy.
    result_doc = (
        dict(sidecar)
        if sidecar is not None
        else _default_sidecar(run_id, sha, verdict, metrics)
    )
    result_doc.setdefault("blockers", list(sidecar_blockers))
    if sidecar_blockers:
        result_doc["blockers"] = list(sidecar_blockers)
    sidecar_text = _canonical(result_doc)
    findings = validate(json.loads(sidecar_text), _RESULT_SCHEMA)
    if findings:
        raise ValueError(f"sidecar does not conform: {[f.message for f in findings][:4]}")

    per_query_lines = [json.dumps(row, sort_keys=True, ensure_ascii=False) for row in per_query]
    for index, line in enumerate(per_query_lines):
        row_findings = validate(json.loads(line), _PER_QUERY_SCHEMA)
        if row_findings:
            raise ValueError(
                f"per-query line {index} does not conform: "
                f"{[f.message for f in row_findings][:4]}"
            )
    per_query_text = "".join(line + "\n" for line in per_query_lines)

    # 3. Claim the directory with one atomic syscall rather than exists-then-write.
    try:
        run_dir.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        existing = run_dir / SIDECAR_NAME
        prior = existing.read_text(encoding="utf-8") if existing.is_file() else None
        if prior == sidecar_text:
            return WriteOutcome(run_id=run_id, run_dir=run_dir)
        # The hashes are EQUAL by construction in a real collision -- the
        # measurements are what differ -- so the comparison is on bytes.
        return WriteOutcome(
            run_id=run_id,
            blocker=Blocker(
                code=BlockerCode.RUN_ID_COLLISION,
                message=(
                    f"run {run_id} already exists with different evidence; run_id is "
                    f"minute-resolution, so advance ts by a minute or change the config. "
                    f"Nothing was written."
                ),
                stage="writer.stage",
                detail={"run_id": run_id, "run_dir": str(run_dir)},
            ),
        )

    # 4. Sidecars land BEFORE the shared record and the index line.
    (run_dir / SIDECAR_NAME).write_text(sidecar_text, encoding="utf-8")
    (run_dir / PER_QUERY_NAME).write_text(per_query_text, encoding="utf-8")

    # 5. Materialize + append (one call, index last inside it).
    index_path = root / "index.jsonl"
    _lib.write_record(
        experiment,
        ts=ts,
        config_obj=document,
        metrics=dict(metrics),
        verdict=verdict.value,
        read=read,
        code=dict(code),
        corpus=dict(corpus),
        seeds=dict(seeds),
        env=dict(env),
        cost_usd=cost_usd,
        n=n,
        headline=dict(headline) if headline else None,
        tdd_evidence=dict(tdd_evidence) if tdd_evidence else None,
        artifacts=[f"runs/{run_id}/{SIDECAR_NAME}", f"runs/{run_id}/{PER_QUERY_NAME}"],
        base_dir=root,
        index_path=index_path,
    )

    # 6. INDEX.md is GENERATED from index.jsonl, so it regenerates after the
    # append -- and both paths are passed, or the default would overwrite the
    # repo's committed INDEX.md from this tmp index.
    _lib.regen_index_md(index_path=index_path, md_path=root / "INDEX.md")
    return WriteOutcome(run_id=run_id, run_dir=run_dir)


def _default_sidecar(
    run_id: str, sha: str, verdict: RunVerdict, metrics: Mapping[str, Any]
) -> dict[str, Any]:
    """The metrics ride in the sidecar, which is what makes a re-measurement
    byte-different from the prior run and therefore a detectable collision --
    the config hash alone is equal by construction."""
    return {
        "schema_version": SCHEMA_VERSION_RESULT,
        "run_id": run_id,
        "campaign": "diagnostic",
        "verdict": verdict.value,
        "scenario": {"config_sha256": sha, "query_call": "Engine.search_text_only"},
        "metrics": dict(metrics),
        "witnesses": [],
        "blockers": [],
    }


__all__ = ["PER_QUERY_NAME", "SCHEMA_VERSION_PER_QUERY", "SIDECAR_NAME", "WriteOutcome", "write_run"]
