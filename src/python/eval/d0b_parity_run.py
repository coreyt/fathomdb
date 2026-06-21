"""0.8.3 D0b parity runner — RED stub (implemented in the GREEN commit)."""
from __future__ import annotations
from typing import Any

TREATMENT_ARM = "fathomdb"
COMPARATOR_ARMS = ("mem0_oss", "graphiti_zep", "naive_rag")
ALL_ARMS = (TREATMENT_ARM, *COMPARATOR_ARMS)


def paired_metric_deltas(*a: Any, **k: Any) -> Any:
    raise NotImplementedError


def class_delta(*a: Any, **k: Any) -> Any:
    raise NotImplementedError


def external_per_class_for_decide(*a: Any, **k: Any) -> Any:
    raise NotImplementedError


def answer_completeness(*a: Any, **k: Any) -> Any:
    raise NotImplementedError


def resume_map(*a: Any, **k: Any) -> Any:
    raise NotImplementedError


def run_d0b(*a: Any, **k: Any) -> Any:
    raise NotImplementedError
