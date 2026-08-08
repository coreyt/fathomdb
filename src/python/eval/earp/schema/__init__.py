"""EARP v1 schema lock.

Declarations only -- no runner, no SDK calls, no engine opened. The JSON Schema
files beside this module are the machine-checkable lock; `models.py` mirrors
them as frozen dataclasses and pinned vocabularies.

    earp.config.v1.schema.json      campaign configuration (strict)
    earp.result.v1.schema.json      run sidecar
    earp.per-query.v1.schema.json   per-query JSONL line

Of record: `dev/design/earp.md`, `dev/plans/earp-foundation.md`,
`dev/notes/earp-hitl-decisions.md`.
"""

from __future__ import annotations

from pathlib import Path

SCHEMA_DIR = Path(__file__).resolve().parent

CONFIG_SCHEMA_PATH = SCHEMA_DIR / "earp.config.v1.schema.json"
RESULT_SCHEMA_PATH = SCHEMA_DIR / "earp.result.v1.schema.json"
PER_QUERY_SCHEMA_PATH = SCHEMA_DIR / "earp.per-query.v1.schema.json"

__all__ = [
    "CONFIG_SCHEMA_PATH",
    "PER_QUERY_SCHEMA_PATH",
    "RESULT_SCHEMA_PATH",
    "SCHEMA_DIR",
]
