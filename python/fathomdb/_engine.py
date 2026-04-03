from __future__ import annotations

import json
import os
from pathlib import Path

from ._admin import AdminClient
from ._feedback import run_with_feedback
from ._fathomdb import EngineCore
from ._query import Query
from ._types import (
    FeedbackConfig,
    LastAccessTouchReport,
    LastAccessTouchRequest,
    ProvenanceMode,
    WriteReceipt,
    WriteRequest,
)


class Engine:
    """Entry point for interacting with a fathomdb database.

    Use :meth:`open` to create an instance, then call :meth:`nodes` to build
    queries or :meth:`write` to submit mutations.  Administrative operations
    are available via the :attr:`admin` attribute.
    """

    def __init__(self, core: EngineCore) -> None:
        self._core = core
        self.admin = AdminClient(core)

    @classmethod
    def open(
        cls,
        database_path: str | os.PathLike[str],
        *,
        provenance_mode: ProvenanceMode | str = ProvenanceMode.WARN,
        vector_dimension: int | None = None,
        telemetry_level: str | None = None,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> "Engine":
        """Open a fathomdb database at the given path.

        Args:
            database_path: Path to the SQLite database file.
            provenance_mode: Provenance enforcement level ("warn" or "require").
            vector_dimension: Embedding dimension for vector search, or None to disable.
            telemetry_level: Telemetry collection level — "counters" (default),
                "statements", or "profiling".
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.

        Returns
        -------
            A new Engine instance connected to the database.

        Raises
        ------
            FathomError: If the database cannot be opened or schema bootstrap fails.
        """
        mode = provenance_mode.value if isinstance(provenance_mode, ProvenanceMode) else provenance_mode
        path = os.fspath(Path(database_path))
        core = run_with_feedback(
            surface="python",
            operation_kind="engine.open",
            metadata={"database_path": path},
            progress_callback=progress_callback,
            feedback_config=feedback_config,
            operation=lambda: EngineCore.open(path, mode, vector_dimension, telemetry_level),
        )
        return cls(core)

    def close(self) -> None:
        """Close the engine, flushing pending writes and releasing resources.

        Idempotent — safe to call multiple times.
        """
        self._core.close()

    def __enter__(self) -> "Engine":
        return self

    def __exit__(self, *exc) -> bool:
        self.close()
        return False

    def telemetry_snapshot(self) -> dict:
        """Read all telemetry counters and SQLite cache statistics.

        Returns a dict with keys: ``queries_total``, ``writes_total``,
        ``write_rows_total``, ``errors_total``, ``admin_ops_total``,
        ``cache_hits``, ``cache_misses``, ``cache_writes``, ``cache_spills``.
        """
        return self._core.telemetry_snapshot()

    def nodes(self, kind: str) -> Query:
        """Start building a query rooted at nodes of the given kind."""
        return Query(self._core, kind)

    def query(self, kind: str) -> Query:
        """Alias for :meth:`nodes`."""
        return self.nodes(kind)

    def write(
        self,
        request: WriteRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> WriteReceipt:
        """Submit a write request (nodes, edges, chunks, etc.) to the database.

        Args:
            request: The write request to submit.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.

        Returns
        -------
            A WriteReceipt summarizing the committed changes.

        Raises
        ------
            InvalidWriteError: If the request contains invalid data.
            WriterRejectedError: If the write is rejected by the engine.
        """
        payload = run_with_feedback(
            surface="python",
            operation_kind="write.submit",
            metadata={"label": request.label},
            progress_callback=progress_callback,
            feedback_config=feedback_config,
            operation=lambda: self._core.submit_write(json.dumps(request.to_wire())),
        )
        return WriteReceipt.from_wire(json.loads(payload))

    def submit(
        self,
        request: WriteRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> WriteReceipt:
        """Alias for :meth:`write`."""
        return self.write(
            request,
            progress_callback=progress_callback,
            feedback_config=feedback_config,
        )

    def touch_last_accessed(
        self,
        request: LastAccessTouchRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> LastAccessTouchReport:
        """Update the last-accessed timestamp for a set of nodes.

        Args:
            request: Specifies which logical IDs to touch and the timestamp.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.

        Returns
        -------
            A report indicating how many nodes were touched.
        """
        payload = run_with_feedback(
            surface="python",
            operation_kind="write.touch_last_accessed",
            metadata={
                "logical_ids": str(len(request.logical_ids)),
                "touched_at": str(request.touched_at),
            },
            progress_callback=progress_callback,
            feedback_config=feedback_config,
            operation=lambda: self._core.touch_last_accessed(json.dumps(request.to_wire())),
        )
        return LastAccessTouchReport.from_wire(json.loads(payload))
