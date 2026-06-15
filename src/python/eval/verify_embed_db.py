"""Verify a FathomDB dense-embed DB is COMPLETE and correct **before** trusting
fused retrieval or committing to a multi-hour full-corpus embed.

Motivation (handoff §5.5 / readiness): a bare ``drain()`` or a
``_fathomdb_projection_terminal`` count does NOT prove the docs were embedded.
Two real partial-embed failure modes were observed 2026-06-15, both of which pass
those naive checks:

  - **0%**  (`/tmp/r2-lme-s-nograph.sqlite`): the ``doc`` vector kind was never
    registered, so the engine marked all 19,195 docs ``projection_terminal``
    WITHOUT embedding them (`_fathomdb_vector_kinds` lacks ``doc``; 0 doc vectors).
  - **~7%** (`/tmp/r2-lme-s-v2.sqlite`): an aborted embed left only 1,408 vectors,
    and all of them ``edge_fact`` — ZERO ``doc`` vectors despite 19,195 docs marked
    terminal.

Only **per-kind vector attribution** via ``_fathomdb_vector_rows(kind, write_cursor)``
detects these. This module is the ground-truth completeness gate: coverage (every doc
has >=1 vector), correct dimension (384), correct embedder, and — optionally —
functional queryability (the dense arm actually returns hits).

Pure DB inspection (read-only sqlite, no embedder needed to RUN the check); the
optional functional probe takes an already-open engine. Unit-tested in
``tests/test_verify_embed_db.py``.

CLI:
    python -m eval.verify_embed_db /tmp/p0a_fused.sqlite --expected-docs 7680
exits non-zero if the DB is not a complete, correct doc-embed.
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
from dataclasses import asdict, dataclass, field
from typing import Any, Optional, Sequence

DEFAULT_EMBEDDER = "fathomdb-bge-small-en-v1.5"
DEFAULT_DIM = 384


@dataclass
class Check:
    name: str
    ok: bool
    detail: str


@dataclass
class VerifyReport:
    db: str
    kind: str
    n_docs: int
    n_docs_embedded: int  # distinct doc cursors carrying >=1 vector
    n_doc_vectors: int  # total vector rows for the kind (>= n_docs_embedded if chunked)
    coverage: float  # n_docs_embedded / n_docs   (1.0 == complete)
    dimension: Optional[int]
    embedder: Optional[str]
    checks: list[Check] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return bool(self.checks) and all(c.ok for c in self.checks)

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        d["ok"] = self.ok
        return d


def _table_exists(con: sqlite3.Connection, name: str) -> bool:
    return (
        con.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?", (name,)
        ).fetchone()
        is not None
    )


def _scalar(con: sqlite3.Connection, sql: str, params: Sequence[Any] = ()) -> int:
    row = con.execute(sql, tuple(params)).fetchone()
    return int(row[0]) if row and row[0] is not None else 0


def inspect_embed_db(
    db_path: str,
    *,
    kind: str = "doc",
    expected_docs: Optional[int] = None,
    expected_dim: int = DEFAULT_DIM,
    expected_embedder: Optional[str] = DEFAULT_EMBEDDER,
    min_coverage: float = 1.0,
) -> VerifyReport:
    """Inspect a DB and return a :class:`VerifyReport`. Read-only; never mutates.

    Opens ``mode=ro`` (NOT ``immutable=1``): the verifier must see committed-but-
    not-yet-checkpointed rows in the **-wal** file when run as a post-``drain()`` gate
    while the engine still holds the DB open. ``immutable=1`` reads only the main DB
    file and would report an empty DB for a freshly-built (uncheckpointed) embed —
    falsely failing every live fused build."""
    con = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    try:
        # --- raw measurements -------------------------------------------------
        has_kinds = _table_exists(con, "_fathomdb_vector_kinds")
        has_vrows = _table_exists(con, "_fathomdb_vector_rows")
        has_nodes = _table_exists(con, "canonical_nodes")
        has_profiles = _table_exists(con, "_fathomdb_embedder_profiles")

        kind_registered = has_kinds and _scalar(
            con, "SELECT count(*) FROM _fathomdb_vector_kinds WHERE kind=?", (kind,)
        ) > 0

        n_docs = (
            _scalar(
                con,
                "SELECT count(*) FROM canonical_nodes WHERE kind=? AND superseded_at IS NULL",
                (kind,),
            )
            if has_nodes
            else 0
        )
        # distinct doc cursors that carry at least one vector of this kind, joined
        # back to a live node so a stale/tombstoned cursor cannot inflate coverage.
        n_docs_embedded = (
            _scalar(
                con,
                """SELECT count(DISTINCT vr.write_cursor)
                   FROM _fathomdb_vector_rows vr
                   JOIN canonical_nodes cn ON cn.write_cursor = vr.write_cursor
                   WHERE vr.kind=? AND cn.kind=? AND cn.superseded_at IS NULL""",
                (kind, kind),
            )
            if (has_vrows and has_nodes)
            else 0
        )
        n_doc_vectors = (
            _scalar(con, "SELECT count(*) FROM _fathomdb_vector_rows WHERE kind=?", (kind,))
            if has_vrows
            else 0
        )

        dimension: Optional[int] = None
        embedder: Optional[str] = None
        if has_profiles:
            row = con.execute(
                "SELECT name, dimension FROM _fathomdb_embedder_profiles "
                "ORDER BY (profile='default') DESC LIMIT 1"
            ).fetchone()
            if row:
                embedder = row[0]
                dimension = int(row[1]) if row[1] is not None else None

        coverage = (n_docs_embedded / n_docs) if n_docs else 0.0

        # --- checks (each independently actionable) ---------------------------
        checks: list[Check] = [
            Check(
                "vector_kind_registered",
                kind_registered,
                f"`{kind}` in _fathomdb_vector_kinds = {kind_registered} "
                "(if False the engine marks docs terminal WITHOUT embedding)",
            ),
            Check(
                "docs_present",
                n_docs > 0
                and (expected_docs is None or n_docs == expected_docs),
                f"n_docs={n_docs}"
                + ("" if expected_docs is None else f" expected={expected_docs}"),
            ),
            Check(
                "coverage_complete",
                n_docs > 0 and coverage >= min_coverage,
                f"coverage={coverage:.4f} ({n_docs_embedded}/{n_docs}) "
                f"min={min_coverage}",
            ),
            Check(
                "vectors_present",
                n_doc_vectors > 0,
                f"n_doc_vectors={n_doc_vectors} (kind=`{kind}`)",
            ),
            Check(
                "dimension_correct",
                dimension == expected_dim,
                f"dimension={dimension} expected={expected_dim}",
            ),
            Check(
                "embedder_correct",
                expected_embedder is None or embedder == expected_embedder,
                f"embedder={embedder!r} expected={expected_embedder!r}",
            ),
        ]
        return VerifyReport(
            db=db_path,
            kind=kind,
            n_docs=n_docs,
            n_docs_embedded=n_docs_embedded,
            n_doc_vectors=n_doc_vectors,
            coverage=round(coverage, 6),
            dimension=dimension,
            embedder=embedder,
            checks=checks,
        )
    finally:
        con.close()


def assert_embed_complete(
    db_path: str, *, expected_docs: Optional[int] = None, kind: str = "doc"
) -> VerifyReport:
    """Raise :class:`EmbedIncompleteError` unless the DB is a complete doc-embed.

    Intended as a post-``drain()`` gate in the fused build path so a partial embed
    becomes a hard failure (variant skipped / re-embed) instead of silently scoring
    fused recall over a half-projected corpus."""
    report = inspect_embed_db(db_path, expected_docs=expected_docs, kind=kind)
    if not report.ok:
        failed = [c for c in report.checks if not c.ok]
        raise EmbedIncompleteError(
            "embed DB incomplete/incorrect: "
            + "; ".join(f"{c.name} [{c.detail}]" for c in failed),
            report,
        )
    return report


class EmbedIncompleteError(RuntimeError):
    def __init__(self, message: str, report: "VerifyReport") -> None:
        super().__init__(message)
        self.report = report


def verify_queryable(retrieve: Any, queries: Sequence[str], *, k: int = 10) -> Check:
    """Functional probe: each query must return >=1 hit (the arm actually serves).

    ``retrieve`` is a callable ``(query, k) -> list`` — e.g. a ``FathomDBAdapter``'s
    ``adapter.retrieve``, or ``lambda q, k: engine.search(q).hits``. The verifier
    itself is DB-only; this is the optional functional complement. A query that
    returns nothing on an embedded corpus means results are not actually served."""
    misses = []
    for q in queries:
        try:
            hits = retrieve(q, k)
        except Exception as exc:  # noqa: BLE001
            misses.append(f"{q!r}->ERR {exc}")
            continue
        if not hits:
            misses.append(f"{q!r}->0 hits")
    return Check(
        "queryable",
        not misses,
        f"{len(queries) - len(misses)}/{len(queries)} queries returned hits"
        + (f"; misses={misses}" if misses else ""),
    )


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="Verify a FathomDB dense-embed DB is complete")
    ap.add_argument("db")
    ap.add_argument("--kind", default="doc")
    ap.add_argument("--expected-docs", type=int, default=None)
    ap.add_argument("--expected-dim", type=int, default=DEFAULT_DIM)
    ap.add_argument("--min-coverage", type=float, default=1.0)
    ap.add_argument("--json", action="store_true", help="emit the full report as JSON")
    args = ap.parse_args(argv)

    report = inspect_embed_db(
        args.db,
        kind=args.kind,
        expected_docs=args.expected_docs,
        expected_dim=args.expected_dim,
        min_coverage=args.min_coverage,
    )
    if args.json:
        print(json.dumps(report.to_dict(), indent=2))
    else:
        verdict = "OK" if report.ok else "FAIL"
        print(
            f"[{verdict}] {args.db} kind={report.kind} "
            f"coverage={report.coverage:.4f} ({report.n_docs_embedded}/{report.n_docs}) "
            f"vectors={report.n_doc_vectors} dim={report.dimension} embedder={report.embedder}"
        )
        for c in report.checks:
            print(f"  [{'ok' if c.ok else 'XX'}] {c.name}: {c.detail}")
    return 0 if report.ok else 1


if __name__ == "__main__":
    sys.exit(main())
