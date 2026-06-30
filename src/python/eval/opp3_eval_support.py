"""0.8.11.2 Phase-0 (P0-4) — OPP-3 eval-support knobs (MEASUREMENT-side only).

This module adds *harness-side* controls and measurements that let OPP-3 probe
**recall robustness** WITHOUT changing any product/engine behavior. Everything
here operates on the ranked ids / scores the engine *already* returns
(:class:`eval.r2_parity_eval.Hit`, the engine ``SearchHit`` / ``SearchResult``):
no new engine field is emitted, no retrieval is re-run from text, no RNG-by-default.

Three groups (P0-4 deliverables):

1. **Distractor injection** + **gold-rank demotion** — pure transforms on a
   ranked id list that simulate a noisier / weaker retriever, so a downstream
   metric (recall@k, MRR, the margins below, an e2e reader) can be re-scored
   under controlled degradation. :func:`inject_distractors`, :func:`demote_gold`.

2. **Per-corpus decision guard** — :func:`decide_per_corpus` runs a frozen
   decision rule (``decide_083`` / ``decide_084``) **once per corpus** and
   refuses a reserved *cross-corpus* pooled key, operationalizing the confirmed
   finding that ``decide_08x`` is per-corpus (each corpus is its own
   :mod:`eval.r2_parity_eval` run → its own Resolution). NB: the per-CLASS
   ``"pooled"`` view inside a single corpus (e.g.
   :mod:`eval.ce_rerank_probe`) is a *class* pool, NOT a corpus pool, and is
   unaffected.

3. **Margin as a measurement** — :func:`top_gap`, :func:`nearest_rival_margin`,
   :func:`margins_from_triples`, :func:`margins_from_search_result`. The margin
   (top-1 vs top-2 score gap; gold-vs-nearest-non-gold separation) is computed
   ENTIRELY from scores already on each hit: the fused ``score`` and the
   per-candidate ``ce_score`` are both fields of the engine ``SearchHit`` today
   (``fathomdb-py`` ``PySearchHit.score`` / ``.ce_score``). No engine change is
   required; the CE-margin only needs the harness ``Hit`` to *carry* the
   already-emitted ``ce_score`` through (see :class:`eval.r2_parity_eval.Hit`).

Pure stdlib — **no ``fathomdb`` / ``numpy`` / ``scipy`` import** — so it (and its
test) run anywhere, independent of the native-extension build or the ``.venv``
binding (mirrors :mod:`eval.decision_rule_083`). Deterministic: no clock, no RNG.

EVAL-ONLY: test-infra under ``eval/`` (NOT shipped in the wheel).
"""

from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Callable, Mapping, Sequence
from typing import Any, Optional, TypeVar

T = TypeVar("T")

# --------------------------------------------------------------------------- #
# 1. Distractor injection + gold-rank demotion (pure ranked-list transforms)
# --------------------------------------------------------------------------- #


def inject_distractors(
    ranked: Sequence[T],
    distractors: Sequence[T],
    *,
    positions: Optional[Sequence[int]] = None,
    spacing: Optional[int] = None,
    skip_present: bool = True,
) -> list[T]:
    """Inject distractor ids into a ranked candidate list (OPP-3 recall-noise knob).

    Returns a NEW ranked list with the distractors woven in. Injecting items
    *above* the gold passage pushes its rank down, which is the point: it lets
    OPP-3 measure how recall@k / MRR / the margins below degrade as hard
    negatives crowd the pool — all without touching the engine.

    Placement (choose at most one of ``positions`` / ``spacing``):

    * ``positions`` — explicit insertion indices interpreted against the
      ORIGINAL ``ranked`` list (so ``[0, 0]`` puts the first two distractors at
      the very top, in order). ``len(positions)`` distractors are injected; the
      rest of ``distractors`` is ignored. An index is clamped to ``[0, len]``.
    * ``spacing`` (>=1) — insert one distractor before index ``0``, then before
      every ``spacing``-th original item (``0, spacing, 2*spacing, ...``) until
      the distractors run out.
    * neither — prepend ALL distractors at the top (the worst case for gold
      rank): equivalent to ``positions=[0, 0, ...]``.

    ``skip_present`` (default True) drops any distractor id already present in
    ``ranked`` and de-dupes the distractor sequence itself, so an injected id is
    a genuine *added* candidate (a true negative), never a duplicate of an
    existing hit. Order is preserved; deterministic (no RNG).

    Raises :class:`ValueError` if both ``positions`` and ``spacing`` are given,
    or if ``spacing`` < 1.
    """
    if positions is not None and spacing is not None:
        raise ValueError("pass at most one of positions / spacing")
    if spacing is not None and spacing < 1:
        raise ValueError(f"spacing must be >= 1, got {spacing}")

    present = set(ranked)
    queued: list[T] = []
    if skip_present:
        seen: set = set()
        for d in distractors:
            if d in present or d in seen:
                continue
            seen.add(d)
            queued.append(d)
    else:
        queued = list(distractors)

    n = len(ranked)

    if positions is not None:
        # Pair queued[i] -> positions[i]; apply left-to-right tracking the
        # running offset so each index refers to the ORIGINAL list.
        out = list(ranked)
        pairs = sorted(
            zip(positions, range(len(queued))),  # (orig_index, distractor_rank)
            key=lambda p: (p[0], p[1]),
        )
        offset = 0
        for orig_idx, di in pairs:
            idx = max(0, min(int(orig_idx), n)) + offset
            out.insert(idx, queued[di])
            offset += 1
        return out

    if spacing is not None:
        out = list(ranked)
        offset = 0
        di = 0
        orig_idx = 0
        while di < len(queued) and orig_idx <= n:
            out.insert(orig_idx + offset, queued[di])
            offset += 1
            di += 1
            orig_idx += spacing
        # Any leftover distractors append at the tail (deterministic).
        if di < len(queued):
            out.extend(queued[di:])
        return out

    # Default: prepend all at the top.
    return [*queued, *ranked]


def demote_gold(
    ranked: Sequence[T],
    gold_ids: Sequence[T],
    *,
    by: Optional[int] = None,
    to_rank: Optional[int] = None,
) -> list[T]:
    """Demote the top-ranked gold item's rank (OPP-3 gold-rank-demotion knob).

    Operates on the single highest-ranked gold item present in ``ranked`` (the
    one recall@1 / MRR keys on) and moves it DOWN, returning a new list. The
    relative order of every other item is preserved. Exactly one of:

    * ``by`` (>=0) — shift the top gold item down by ``by`` positions, clamped to
      the end of the list (``by=0`` is a no-op copy).
    * ``to_rank`` (>=0) — move the top gold item to absolute index ``to_rank``
      (clamped to ``[0, len-1]``); a ``to_rank`` above its current rank would
      *promote* it, which is rejected (this knob only demotes).

    If no gold id is present in ``ranked``, the list is returned unchanged
    (copied). Deterministic; no RNG.

    Raises :class:`ValueError` if neither/both of ``by``/``to_rank`` are given,
    if ``by`` < 0, if ``to_rank`` < 0, or if ``to_rank`` would promote.
    """
    if (by is None) == (to_rank is None):
        raise ValueError("pass exactly one of by / to_rank")
    if by is not None and by < 0:
        raise ValueError(f"by must be >= 0, got {by}")
    if to_rank is not None and to_rank < 0:
        raise ValueError(f"to_rank must be >= 0, got {to_rank}")

    out = list(ranked)
    gold = set(gold_ids)
    cur = next((i for i, x in enumerate(out) if x in gold), None)
    if cur is None:
        return out  # no gold present — nothing to demote.

    last = len(out) - 1
    if by is not None:
        target = min(cur + by, last)
    else:
        assert to_rank is not None
        target = min(to_rank, last)
        if target < cur:
            raise ValueError(
                f"to_rank={to_rank} would promote the gold item from rank {cur} "
                "(demote_gold only demotes)"
            )

    item = out.pop(cur)
    out.insert(target, item)
    return out


# --------------------------------------------------------------------------- #
# 2. Per-corpus decision guard (operationalizes the confirmed finding)
# --------------------------------------------------------------------------- #

#: Corpus ids that would denote a CROSS-CORPUS pool — forbidden as a key into
#: :func:`decide_per_corpus`, which decides each corpus independently. (The
#: per-CLASS ``"pooled"`` view INSIDE a single corpus is a different axis and is
#: not a corpus id, so it never reaches here.)
RESERVED_POOL_KEYS: frozenset[str] = frozenset(
    {"pooled", "all", "combined", "all_corpora", "overall", "global"}
)


def decide_per_corpus(
    by_corpus: Mapping[str, T],
    decide: Callable[[T], Any],
) -> dict[str, Any]:
    """Run a frozen decision rule **once per corpus**, never pooled.

    ``by_corpus`` maps a corpus id (``"lme"`` / ``"locomo"`` / ``"musique"`` /
    ``"apnews"`` / ...) to that corpus's already-computed decision payload;
    ``decide`` is the frozen rule applied to one payload (e.g. a partial of
    :func:`eval.decision_rule_083.decide_083` bound to that corpus's eu7 +
    latency, or :func:`eval.decision_rule_084.decide_084`). Returns
    ``{corpus_id: Resolution}``.

    Guard: a reserved cross-corpus key (:data:`RESERVED_POOL_KEYS`) raises
    :class:`ValueError` — pooling queries across *different* corpora before
    deciding would mix incomparable gold distributions and mask a per-corpus
    regression. This is the executable form of the P0-4 finding that
    ``decide_08x`` is per-corpus.
    """
    bad = sorted(k for k in by_corpus if str(k).strip().lower() in RESERVED_POOL_KEYS)
    if bad:
        raise ValueError(
            "cross-corpus pooling is forbidden — decide each corpus separately; "
            f"reserved key(s): {bad}"
        )
    return {cid: decide(payload) for cid, payload in by_corpus.items()}


# --------------------------------------------------------------------------- #
# 3. Margin as a measurement (computed from scores the engine already returns)
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class Margins:
    """The margin readout for one ranked result (all harness-computed).

    * ``top_gap`` — fused-score gap between rank-1 and rank-2 (``None`` if < 2
      hits). How decisively the top hit beats the runner-up.
    * ``gold_rival_margin`` — best-gold fused score minus best-non-gold fused
      score (signed). > 0: gold sits above its nearest distractor; < 0: a
      distractor outranks every gold hit (a recall-at-1 miss, with cushion).
      ``None`` if no gold OR no non-gold hit is present.
    * ``top_gap_ce`` / ``gold_rival_margin_ce`` — the same two over the
      per-candidate ``ce_score`` (computed over the sub-pool that carries a CE
      score, in rank order). ``None`` when fewer than the needed CE scores exist.
    """

    top_gap: Optional[float]
    gold_rival_margin: Optional[float]
    top_gap_ce: Optional[float]
    gold_rival_margin_ce: Optional[float]


def top_gap(scores: Sequence[float]) -> Optional[float]:
    """Rank-1 minus rank-2 score (``None`` if fewer than two scores).

    Assumes ``scores`` is in the ranked (descending) order the engine returns;
    does not re-sort, so it measures the *as-served* gap.
    """
    if len(scores) < 2:
        return None
    return float(scores[0]) - float(scores[1])


def nearest_rival_margin(
    ranked_ids: Sequence[T],
    scores: Sequence[float],
    gold_ids: Sequence[T],
) -> Optional[float]:
    """Best-gold score minus best-non-gold score over a ranked result.

    ``ranked_ids`` and ``scores`` are parallel (same length, same order).
    Returns ``best_gold - best_non_gold`` (signed), or ``None`` when the result
    lacks a gold hit or lacks a non-gold hit (the margin is undefined then).
    """
    if len(ranked_ids) != len(scores):
        raise ValueError(
            f"ranked_ids ({len(ranked_ids)}) and scores ({len(scores)}) "
            "must be parallel"
        )
    gold = set(gold_ids)
    gold_scores = [float(s) for i, s in zip(ranked_ids, scores) if i in gold]
    rival_scores = [float(s) for i, s in zip(ranked_ids, scores) if i not in gold]
    if not gold_scores or not rival_scores:
        return None
    return max(gold_scores) - max(rival_scores)


def margins_from_triples(
    triples: Sequence[tuple[T, float, Optional[float]]],
    gold_ids: Sequence[T],
) -> Margins:
    """Compute :class:`Margins` from ranked ``(id, score, ce_score)`` triples.

    ``triples`` is in ranked order (rank-1 first). ``ce_score`` may be ``None``
    for hits outside the reranked pool; the CE margins are computed over the
    rank-ordered sub-pool of hits that DO carry a CE score. Pure / deterministic.
    """
    ids = [t[0] for t in triples]
    scores = [float(t[1]) for t in triples]

    ce_pairs = [(t[0], float(t[2])) for t in triples if t[2] is not None]
    ce_ids = [p[0] for p in ce_pairs]
    ce_scores = [p[1] for p in ce_pairs]

    return Margins(
        top_gap=top_gap(scores),
        gold_rival_margin=nearest_rival_margin(ids, scores, gold_ids),
        top_gap_ce=top_gap(ce_scores),
        gold_rival_margin_ce=nearest_rival_margin(ce_ids, ce_scores, gold_ids),
    )


def margins_from_search_result(
    result: Any,
    gold_ids: Sequence[str],
    *,
    id_of: Optional[Callable[[Any], str]] = None,
) -> Margins:
    """Compute :class:`Margins` directly from an engine ``SearchResult``.

    Reads ``result.results`` — each hit's ``.score`` (fused) and ``.ce_score``
    (per-candidate CE, ``None`` outside the pool) are existing engine-emitted
    fields, so this needs NO engine change. ``id_of`` maps a hit to its corpus
    doc id (the gold join key); defaults to ``str(hit.id)``. The hits are taken
    in the order the engine returned them (already ranked).
    """
    resolve = id_of if id_of is not None else (lambda h: str(h.id))
    triples: list[tuple[str, float, Optional[float]]] = []
    for h in result.results:
        ce = getattr(h, "ce_score", None)
        triples.append(
            (resolve(h), float(h.score), None if ce is None else float(ce))
        )
    return margins_from_triples(triples, gold_ids)
