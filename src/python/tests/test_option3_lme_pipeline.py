"""Slice 30 Option 3 — LongMemEval loader + R2Harness.from_queries TDD tests.

TDD: RED commit first (functions not yet implemented). GREEN commit lands after
implementation.

Tests:
  T5  test_lme_loader_produces_documents_and_queries
  T6  test_lme_class_map_covers_all_seven_types
  T7  test_r2_harness_from_queries_lme_end_to_end
"""

from __future__ import annotations

import json
from pathlib import Path


# ---------------------------------------------------------------------------
# T5 — load_longmemeval_from_file parses LME JSON into (documents, queries)
# ---------------------------------------------------------------------------


def test_lme_loader_produces_documents_and_queries(tmp_path: Path) -> None:
    """load_longmemeval_from_file parses LME JSON and returns (documents, queries)."""
    from eval.r2_parity_eval import load_longmemeval_from_file

    fixture = [
        {
            "question_id": "lme-001",
            "question_type": "temporal-reasoning",
            "question": "When did Alice first mention her car service?",
            "answer": "March 15th",
            "question_date": "2023/05/30 (Tue) 23:40",
            "haystack_dates": ["2023/03/15 (Wed) 10:00", "2023/04/01 (Sat) 14:00"],
            "haystack_session_ids": ["sess-car-001", "sess-distractor-001"],
            "haystack_sessions": [
                [
                    {
                        "role": "user",
                        "content": "I just got my car serviced on March 15th.",
                        "has_answer": True,
                    },
                    {"role": "assistant", "content": "Great, hope it went well!"},
                ],
                [
                    {"role": "user", "content": "What's the weather today?"},
                    {"role": "assistant", "content": "Sunny and 72°F."},
                ],
            ],
            "answer_session_ids": ["sess-car-001"],
        },
        {
            "question_id": "lme-002",
            "question_type": "knowledge-update",
            "question": "What city does Bob live in now?",
            "answer": "Seattle",
            "question_date": "2023/06/01 (Thu) 09:00",
            "haystack_dates": ["2023/01/01 (Sun) 12:00", "2023/05/15 (Mon) 08:00"],
            "haystack_session_ids": ["sess-bob-old", "sess-bob-new"],
            "haystack_sessions": [
                [
                    {"role": "user", "content": "I live in Portland."},
                ],
                [
                    {"role": "user", "content": "I just moved to Seattle!"},
                    {"role": "assistant", "content": "Exciting! Seattle is a great city."},
                ],
            ],
            "answer_session_ids": ["sess-bob-new"],
        },
    ]

    fixture_path = tmp_path / "lme_fixture.json"
    fixture_path.write_text(json.dumps(fixture), encoding="utf-8")

    documents, queries = load_longmemeval_from_file(fixture_path)

    # 4 unique sessions total (sess-car-001, sess-distractor-001, sess-bob-old, sess-bob-new)
    assert len(documents) == 4, f"expected 4 unique sessions, got {len(documents)}"
    assert "sess-car-001" in documents
    assert "sess-distractor-001" in documents
    assert "sess-bob-old" in documents
    assert "sess-bob-new" in documents

    # Session body contains formatted turns
    car_body = documents["sess-car-001"]
    assert "March 15th" in car_body
    assert "[User]:" in car_body
    assert "[Assistant]:" in car_body

    # 2 queries
    assert len(queries) == 2, f"expected 2 queries, got {len(queries)}"

    q1 = next(q for q in queries if q.query_id == "lme-001")
    assert q1.reporting_class == "temporal", f"expected 'temporal', got {q1.reporting_class!r}"
    assert "sess-car-001" in q1.gold_doc_ids

    q2 = next(q for q in queries if q.query_id == "lme-002")
    assert q2.reporting_class == "knowledge_update", f"expected 'knowledge_update', got {q2.reporting_class!r}"
    assert "sess-bob-new" in q2.gold_doc_ids


# ---------------------------------------------------------------------------
# T6 — LME_CLASS_MAP covers all 7 canonical LME question_types
# ---------------------------------------------------------------------------


def test_lme_class_map_covers_all_seven_types() -> None:
    """LME_CLASS_MAP maps all 7 canonical LME question_types to R2 classes."""
    from eval.r2_parity_eval import LME_CLASS_MAP

    expected_mappings = {
        "temporal-reasoning": "temporal",
        "knowledge-update": "knowledge_update",
        "multi-session": "multi_session",
        "single-session-user": "factoid",
        "single-session-assistant": "factoid",
        "single-session-preference": "factoid",
        # abstention variants
        "temporal-reasoning_abs": "negative",
        "knowledge-update_abs": "negative",
        "multi-session_abs": "negative",
    }
    for lme_type, expected_r2_class in expected_mappings.items():
        got = LME_CLASS_MAP.get(lme_type)
        assert got == expected_r2_class, (
            f"LME_CLASS_MAP[{lme_type!r}] = {got!r}, expected {expected_r2_class!r}"
        )


# ---------------------------------------------------------------------------
# T7 — R2Harness.from_queries + LME queries: end-to-end recall
# ---------------------------------------------------------------------------


def test_r2_harness_from_queries_lme_end_to_end(tmp_path: Path) -> None:
    """R2Harness.from_queries + LME queries: temporal class appears with non-null recall."""
    from eval.r2_parity_eval import (
        Hit,
        NullAnswerer,
        R2Harness,
        StubAdapter,
        load_longmemeval_from_file,
    )

    fixture = [
        {
            "question_id": "lme-t1",
            "question_type": "temporal-reasoning",
            "question": "When did Alice first mention her car service?",
            "answer": "March 15th",
            "question_date": "2023/05/30 (Tue) 23:40",
            "haystack_dates": ["2023/03/15 (Wed) 10:00"],
            "haystack_session_ids": ["sess-car-001"],
            "haystack_sessions": [
                [{"role": "user", "content": "I got my car serviced on March 15th.", "has_answer": True}]
            ],
            "answer_session_ids": ["sess-car-001"],
        }
    ]
    fixture_path = tmp_path / "lme_t7.json"
    fixture_path.write_text(json.dumps(fixture), encoding="utf-8")

    _documents, lme_queries = load_longmemeval_from_file(fixture_path)

    harness = R2Harness.from_queries(
        lme_queries,
        NullAnswerer(),
        corpus_hash="lme-oracle-v1",
        qrels_version="lme-test-v1",
    )

    # StubAdapter returns the correct session for the question
    stub_hits = {
        "When did Alice first mention her car service?": [
            Hit(doc_id="sess-car-001", body="I got my car serviced on March 15th.", score=1.0)
        ]
    }
    systems = {
        "fathomdb": StubAdapter(name="fathomdb", hits_by_query=stub_hits),
        "naive_rag": StubAdapter(name="naive_rag", hits_by_query=stub_hits),
    }

    result = harness.run(systems, k=5)

    assert "temporal" in result["n_queries_per_class"]
    assert result["n_queries_per_class"]["temporal"] == 1
    fdb_temporal = result["r2_results"]["fathomdb"]["temporal"]
    assert fdb_temporal["recall_at_k"] is not None
    assert fdb_temporal["recall_at_k"] == 1.0, f"expected perfect recall, got {fdb_temporal['recall_at_k']}"
