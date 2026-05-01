from __future__ import annotations

import time
from pathlib import Path


class _FakeCore:
    def __init__(self, *, delay_s: float = 0.0, fail: Exception | None = None) -> None:
        self.delay_s = delay_s
        self.fail = fail

    @staticmethod
    def _compiled() -> str:
        return (
            '{"sql":"SELECT 1","binds":[],"shape_hash":1,'
            '"driving_table":"nodes","hints":{"recursion_limit":8,"hard_limit":1000}}'
        )

    @staticmethod
    def _rows() -> str:
        return '{"nodes":[],"runs":[],"steps":[],"actions":[],"was_degraded":false}'

    @staticmethod
    def _integrity() -> str:
        return (
            '{"physical_ok":true,"foreign_keys_ok":true,"missing_fts_rows":0,'
            '"duplicate_active_logical_ids":0,"warnings":[]}'
        )

    @staticmethod
    def _receipt() -> str:
        return '{"label":"seed","optional_backfill_count":0,"provenance_warnings":[]}'

    def _maybe_block(self) -> None:
        if self.delay_s:
            time.sleep(self.delay_s)
        if self.fail is not None:
            raise self.fail

    @classmethod
    def open(cls, database_path: str, provenance_mode: str, vector_dimension: int | None = None) -> "_FakeCore":
        del database_path, provenance_mode, vector_dimension
        return cls()

    def compile_ast(self, ast_json: str) -> str:
        del ast_json
        self._maybe_block()
        return self._compiled()

    def explain_ast(self, ast_json: str) -> str:
        del ast_json
        self._maybe_block()
        return '{"sql":"SELECT 1","bind_count":0,"driving_table":"nodes","shape_hash":1,"cache_hit":false}'

    def execute_ast(self, ast_json: str) -> str:
        del ast_json
        self._maybe_block()
        return self._rows()

    def submit_write(self, request_json: str) -> str:
        del request_json
        self._maybe_block()
        return self._receipt()

    def check_integrity(self) -> str:
        self._maybe_block()
        return self._integrity()


def test_engine_open_write_query_and_admin_callbacks_are_publicly_available(tmp_path: Path) -> None:
    from fathomdb import ChunkInsert, ChunkPolicy, Engine, FeedbackConfig, NodeInsert, ResponseCyclePhase, WriteRequest, new_row_id

    events = []
    config = FeedbackConfig(slow_threshold_ms=1, heartbeat_interval_ms=5)

    db = Engine.open(
        tmp_path / "agent.db",
        feedback_config=config,
        progress_callback=events.append,
    )

    phases = [event.phase for event in events]
    assert phases[0] is ResponseCyclePhase.STARTED
    assert phases[-1] is ResponseCyclePhase.FINISHED

    events.clear()
    db.write(
        WriteRequest(
            label="feedback-seed",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting:feedback",
                    kind="Meeting",
                    properties={"title": "Feedback"},
                    source_ref="source:feedback",
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id="chunk:feedback:0",
                    node_logical_id="meeting:feedback",
                    text_content="feedback budget text",
                )
            ],
        ),
        feedback_config=config,
        progress_callback=events.append,
    )
    phases = [event.phase for event in events]
    assert phases[0] is ResponseCyclePhase.STARTED
    assert phases[-1] is ResponseCyclePhase.FINISHED

    events.clear()
    db.nodes("Meeting").text_search("feedback", limit=5).execute(
        feedback_config=config,
        progress_callback=events.append,
    )
    phases = [event.phase for event in events]
    assert phases[0] is ResponseCyclePhase.STARTED
    assert phases[-1] is ResponseCyclePhase.FINISHED

    events.clear()
    db.admin.check_integrity(feedback_config=config, progress_callback=events.append)
    phases = [event.phase for event in events]
    assert phases[0] is ResponseCyclePhase.STARTED
    assert phases[-1] is ResponseCyclePhase.FINISHED


def test_python_feedback_emits_slow_and_heartbeat_for_slow_operation() -> None:
    from fathomdb import AdminClient, FeedbackConfig, ResponseCyclePhase

    # Operation duration must be comfortably larger than
    # slow_threshold_ms + heartbeat_interval_ms + thread-wake jitter so a
    # heartbeat reliably fires before FINISHED, even on slow CI runners
    # (observed flake: macOS CI missing HEARTBEAT when delay_s was 0.05).
    core = _FakeCore(delay_s=0.2)
    admin = AdminClient(core)
    events = []

    report = admin.check_integrity(
        feedback_config=FeedbackConfig(slow_threshold_ms=5, heartbeat_interval_ms=10),
        progress_callback=events.append,
    )

    assert report.physical_ok is True
    phases = [event.phase for event in events]
    assert phases[0] is ResponseCyclePhase.STARTED
    assert ResponseCyclePhase.SLOW in phases
    assert ResponseCyclePhase.HEARTBEAT in phases
    assert phases[-1] is ResponseCyclePhase.FINISHED


def test_python_feedback_contains_failure_and_suppresses_callback_errors() -> None:
    from fathomdb import Engine, FeedbackConfig, ResponseCyclePhase

    core = _FakeCore(fail=RuntimeError("boom"))
    query = Engine(core).nodes("Meeting")
    events = []
    callback_events = []

    def callback(event) -> None:
        callback_events.append(event.phase)
        if event.phase is ResponseCyclePhase.STARTED:
            raise RuntimeError("observer should be suppressed")
        events.append(event)

    try:
        query.execute(
            feedback_config=FeedbackConfig(slow_threshold_ms=1, heartbeat_interval_ms=5),
            progress_callback=callback,
        )
    except RuntimeError as error:
        assert str(error) == "boom"
    else:
        raise AssertionError("execute should fail")

    assert callback_events == [ResponseCyclePhase.STARTED]
    assert events == []
