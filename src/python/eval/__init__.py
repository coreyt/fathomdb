"""FathomDB R2 end-to-end parity eval harness (Slice 25, test-infra).

This package is **not** part of the shipped ``fathomdb`` wheel — maturin only
packages the ``fathomdb`` module. It is the LongMemEval-style end-to-end memory
eval described in ``dev/adr/ADR-0.8.1-ir-measure-eval-design.md §3``: an identical
answerer over FathomDB (post-R1) vs a local Mem0-OSS baseline vs naive-RAG, scored
per query class with abstention.
"""

from eval.r2_parity_eval import (
    CORPUS_HASH_PREFIX,
    R2_CLASSES,
    FathomDBAdapter,
    Hit,
    Mem0OSSAdapter,
    NaiveRAGAdapter,
    PerClassScorer,
    R2Harness,
    run_r2_eval,
)

__all__ = [
    "CORPUS_HASH_PREFIX",
    "R2_CLASSES",
    "FathomDBAdapter",
    "Hit",
    "Mem0OSSAdapter",
    "NaiveRAGAdapter",
    "PerClassScorer",
    "R2Harness",
    "run_r2_eval",
]
