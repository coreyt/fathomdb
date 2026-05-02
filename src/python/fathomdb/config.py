"""Engine config dataclass mirrored across SDK bindings.

Knob set owned by `dev/design/engine.md` and pinned for symmetry across
SDK bindings by `dev/design/bindings.md` § 6. Field names follow Python
snake_case per `dev/interfaces/python.md`.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class EngineConfig:
    """Engine-owned runtime knobs.

    Equivalent to the keyword form on `Engine.open`; both forms accepted, but
    not in the same call.
    """

    embedder_pool_size: int | None = None
    scheduler_runtime_threads: int | None = None
    provenance_row_cap: int | None = None
    embedder_call_timeout_ms: int | None = None
    slow_threshold_ms: int | None = None


__all__ = ["EngineConfig"]
