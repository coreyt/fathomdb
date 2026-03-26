from __future__ import annotations

from functools import lru_cache
from pathlib import Path
from tempfile import TemporaryDirectory

from fathomdb import (
    CapabilityMissingError,
    ChunkInsert,
    ChunkPolicy,
    Engine,
    NodeInsert,
    SchemaError,
    VecInsert,
    WriteRequest,
    new_row_id,
)


DEFAULT_VECTOR_DIMENSION = 4


def open_engine(
    database_path: str | Path,
    *,
    mode: str,
    vector_dimension: int = DEFAULT_VECTOR_DIMENSION,
) -> Engine:
    path = Path(database_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    if mode == "baseline":
        return Engine.open(path)
    if mode == "vector":
        return Engine.open(path, vector_dimension=vector_dimension)
    raise ValueError(f"unsupported harness mode: {mode}")


@lru_cache(maxsize=1)
def supports_vector_mode() -> bool:
    with TemporaryDirectory(prefix="fathomdb-py-harness-") as temp_dir:
        db_path = Path(temp_dir) / "probe.db"
        try:
            db = Engine.open(db_path, vector_dimension=DEFAULT_VECTOR_DIMENSION)
        except (CapabilityMissingError, SchemaError):
            return False
        db.write(
            WriteRequest(
                label="vector-probe",
                nodes=[
                    NodeInsert(
                        row_id=new_row_id(),
                        logical_id="document:vector-probe",
                        kind="Document",
                        properties={"title": "Vector probe"},
                        source_ref="source:vector-probe",
                        upsert=True,
                        chunk_policy=ChunkPolicy.REPLACE,
                    )
                ],
                chunks=[
                    ChunkInsert(
                        id="chunk:document:vector-probe:0",
                        node_logical_id="document:vector-probe",
                        text_content="vector probe chunk",
                    )
                ],
                vec_inserts=[
                    VecInsert(
                        chunk_id="chunk:document:vector-probe:0",
                        embedding=[0.1, 0.2, 0.3, 0.4],
                    )
                ],
            )
        )
        rows = db.nodes("Document").vector_search("[0.1, 0.2, 0.3, 0.4]", limit=1).execute()
        return rows.was_degraded is False
