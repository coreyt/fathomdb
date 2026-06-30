"""0.8.8 Slice 20 (OPP-9) real-gold capture — telemetry JSONL → ``GoldRecord``.

This is FathomDB's exit from synthetic-gold purgatory: real
query→result→agent-feedback rows captured by the engine's opt-in telemetry
sink (Slice 15) become labeled gold *in the engine's own id namespace*.

Binding spec: ``dev/design/0.8.8-explain-and-telemetry-adr.md §B.2/§B.3`` +
``dev/plans/runs/0.8.8-explanation-fieldset-ratification.md §3d``.

ROW-SHAPE alignment (§3d Q4 / B.3). :class:`GoldRecord` mirrors the *row shape*
of :class:`eval.r2_parity_eval.GoldQuery` (a ``query_id`` join key plus the gold
fields) but NOT its id *values*: the harness gold is string-keyed over an
external corpus the engine never sees, whereas telemetry gold lives in the
engine's own id namespace. Two corpora, two namespaces — forcing one into the
other is a category error, so we align the shape and tag the namespace with
``id_space``.

ID CONTRACT (§B.2, RATIFIED). Every ``u64`` id carried here
(``candidate_ids`` / ``labels`` keys) is the engine identity carrier ==
``SearchHit.id`` == the telemetry sink's ``result_ids`` / ``*_ids``. Per the
ratification this is the **stable ``logical_id``** and ``id_space`` is tagged
``"engine-logical-id"`` accordingly. CAVEAT (flagged for HITL): per ADR-0.8.0
``SearchHit.id`` is *today* the interim ``write_cursor`` and only swaps to the
true ``logical_id`` at the identity-substrate keystone. For this within-session
fixture pipeline the distinction is invisible — the gold ids are byte-identical
to the telemetry/search ids that produced them, so correlation is exact — but
the ``id_space`` tag is forward-looking, not a claim that the carrier is
``logical_id`` on this base. See the closure flag.

PRIVACY (§C). The sink never contains query text, document bodies, or
``source_id``; this module only ever reads ids, lengths, and caller-supplied
labels, so nothing exogenous is reconstructed here.

EVAL-ONLY: this module lives under ``eval/`` (test-infra, NOT shipped in the
wheel), exactly like the other ``eval/`` harness modules.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

#: The gold schema version (matches the telemetry sink ``schema_version``).
SCHEMA_VERSION = 1

#: Machine-checkable namespace tag (§3d). The ids live in the engine's own id
#: namespace, NOT an external corpus string namespace.
ID_SPACE = "engine-logical-id"

#: How this gold was produced (§B.2).
PROVENANCE = "telemetry-capture"


@dataclass(frozen=True)
class GoldRecord:
    """One labeled gold query in the engine id namespace (§B.2).

    Row-shape mirror of :class:`eval.r2_parity_eval.GoldQuery` (``query_id`` +
    gold fields) but engine-id-keyed. Frozen so a captured record is an
    immutable fixture artifact.

    Fields (EXACTLY, per §B.2):

    - ``schema_version`` — gold schema version.
    - ``query_id`` — join key back to ``TelemetryEvent.query_id``.
    - ``id_space`` — namespace tag, always ``"engine-logical-id"``.
    - ``query_chars`` — query length only (never the text; §C).
    - ``candidate_ids`` — the frozen returned hit ids, in rank order (the
      telemetry event's ``result_ids``). UNCHANGED interim ``write_cursor`` space.
    - ``candidate_stable_ids`` — Cause-A (0.8.11.2) additive PARALLEL array, same
      length/order as ``candidate_ids``, carrying the cross-session-stable id
      (``SearchHit.stable_id``: ``"l:"`` logical_id or ``"h:"`` content-hash;
      ``None`` for a hit with no stable id). Read from the event's
      ``result_stable_ids``; empty when the sink predates Cause-A. This is the
      additive landing of the F-8a remap — ``candidate_ids`` / ``id_space`` are
      RETAINED unchanged (no in-place flip; the conscious ``id_space`` switch is a
      separate later step, ``NOTE-0.8.8-to-steward-id-contract.md``).
    - ``labels`` — ``{id: relevance}`` with ``relevance`` in ``{0, 1}``, derived
      from the correlated feedback row (relevant→1, irrelevant→0). Keyed in the
      same ``write_cursor`` space as ``candidate_ids`` (the feedback row's
      ``*_ids``); map to stable ids via the ``candidate_ids`` ↔
      ``candidate_stable_ids`` positional correspondence.
    - ``embedder_id`` — caller-supplied embedder identity (``""`` sentinel when
      unknown, never ``None`` — mirrors the engine's ``embedder_id`` sentinel
      convention in the explain field set).
    - ``provenance`` — always ``"telemetry-capture"``.
    """

    schema_version: int
    query_id: str
    id_space: str
    query_chars: int
    candidate_ids: tuple[int, ...]
    labels: dict[int, int]
    embedder_id: str
    provenance: str
    #: Cause-A (0.8.11.2) — additive parallel cross-session-stable ids, same
    #: length/order as ``candidate_ids``. Defaults to ``()`` (pre-Cause-A sinks).
    candidate_stable_ids: tuple[str | None, ...] = ()


def _as_int_list(value: object) -> list[int]:
    """Coerce a sink field expected to be a list-of-ids into ``list[int]``.

    Robustness (codex §9 [P2-a]): a truncated or hand-edited sink can put a
    scalar/``null`` where a list belongs (e.g. ``"result_ids": 123`` or
    ``"relevant_ids": null``). Iterating that value directly would raise
    ``TypeError`` and abort the WHOLE capture. We treat any non-list as empty
    and drop non-int elements, so one malformed row never prevents later valid
    gold records from building. ``bool`` is excluded (``True``/``False`` are not
    ids even though ``bool`` subclasses ``int``)."""
    if not isinstance(value, list):
        return []
    return [int(i) for i in value if isinstance(i, int) and not isinstance(i, bool)]


def _as_opt_str_list(value: object) -> list[str | None]:
    """Coerce a sink ``result_stable_ids`` field into ``list[str | None]``.

    Cause-A (0.8.11.2). Mirrors :func:`_as_int_list`'s robustness: a non-list
    (truncated/old sink) yields ``[]``; ``None`` entries (a hit with no stable
    id — synthetic passages) are preserved as ``None``; any non-str/non-null
    element is dropped so one malformed row never aborts the capture."""
    if not isinstance(value, list):
        return []
    return [v if isinstance(v, str) else None for v in value]


def _iter_jsonl(text: str) -> list[dict]:
    """Parse JSONL text into dict rows, skipping blank/malformed lines.

    Robustness (§ "ignore malformed lines gracefully"): a line that is not valid
    JSON, or is valid JSON but not an object, is silently dropped — a truncated
    final line or a partially-flushed sink can never crash gold capture.
    """
    rows: list[dict] = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except (ValueError, TypeError):
            continue
        if isinstance(obj, dict):
            rows.append(obj)
    return rows


def build_gold_records(sink_path: str | Path, embedder_id: str = "") -> list[GoldRecord]:
    """Read a telemetry JSONL sink and emit one :class:`GoldRecord` per *labeled*
    query.

    Correlation (§B.2; occurrence/order-based, codex §9 [P2-b]). The
    deterministic ``query_id`` restarts at ``q0-0`` on every
    ``enable_telemetry`` (each new session), and the sink is append-only, so the
    SAME ``query_id`` can recur across sessions in one file. A global
    last-wins dict would collapse those distinct captures (or pair a session-2
    event with session-1 feedback). Instead we walk rows in FILE ORDER and pair
    each ``feedback`` row with the most recent preceding **un-paired** ``event``
    row sharing its ``query_id`` (LIFO per ``query_id``), emitting one
    GoldRecord per paired ``(event, feedback)``. Records are emitted in
    feedback-encounter order. Within a single sink the engine writes a session's
    event(s) before that session's feedback, so this recovers each session's own
    capture. For each pair the GoldRecord carries:

    - ``candidate_ids`` = the paired event's ``result_ids`` (frozen returned pool),
    - ``query_chars``   = the paired event's ``query_chars``,
    - ``labels``        = ``{id: 1 for id in relevant_ids}`` unioned with
      ``{id: 0 for id in irrelevant_ids}``. On the (contradictory) overlap of an
      id in both lists, ``irrelevant`` wins (the conservative 0 — a contested
      candidate is not counted as relevant gold).

    Events WITHOUT a feedback row (left un-paired at end of file) are dropped (no
    unlabeled gold): an unlabeled query carries no exogenous relevance signal, so
    it cannot be scored by the offline frozen-candidate scorer and is not gold.
    (This is the documented "no record" branch of the spec's "no record /
    unlabeled record — your call" choice.)

    Robustness: malformed lines, rows missing ``query_id``/``type``, rows with a
    scalar/``null`` where a list belongs (coerced to empty via
    :func:`_as_int_list`), and feedback rows with no preceding un-paired event
    are all skipped gracefully — one bad row never aborts the capture.

    ``embedder_id`` is caller-supplied (the telemetry sink does not capture it);
    it defaults to the ``""`` sentinel.
    """
    text = Path(sink_path).read_text(encoding="utf-8")
    rows = _iter_jsonl(text)

    # Per query_id, a LIFO stack of un-paired event rows seen so far (in file
    # order). A feedback row pops the most recent un-paired event for its id.
    pending_events: dict[str, list[dict]] = {}
    records: list[GoldRecord] = []

    for row in rows:
        qid = row.get("query_id")
        if not isinstance(qid, str) or not qid:
            continue
        row_type = row.get("type")
        if row_type == "event":
            pending_events.setdefault(qid, []).append(row)
        elif row_type == "feedback":
            stack = pending_events.get(qid)
            if not stack:
                # Feedback with no preceding un-paired event → skipped (orphan).
                continue
            ev = stack.pop()  # most recent preceding un-paired event (LIFO)
            fb = row

            candidate_ids = tuple(_as_int_list(ev.get("result_ids")))
            # Cause-A: additive parallel stable ids (empty for pre-Cause-A sinks).
            candidate_stable_ids = tuple(_as_opt_str_list(ev.get("result_stable_ids")))
            # Build labels: relevant→1 first, then irrelevant→0 (overlap → 0 wins).
            labels: dict[int, int] = {i: 1 for i in _as_int_list(fb.get("relevant_ids"))}
            for i in _as_int_list(fb.get("irrelevant_ids")):
                labels[i] = 0

            query_chars = ev.get("query_chars", 0)
            query_chars = (
                int(query_chars)
                if isinstance(query_chars, int) and not isinstance(query_chars, bool)
                else 0
            )

            records.append(
                GoldRecord(
                    schema_version=SCHEMA_VERSION,
                    query_id=qid,
                    id_space=ID_SPACE,
                    query_chars=query_chars,
                    candidate_ids=candidate_ids,
                    labels=labels,
                    embedder_id=embedder_id,
                    provenance=PROVENANCE,
                    candidate_stable_ids=candidate_stable_ids,
                )
            )
    return records
