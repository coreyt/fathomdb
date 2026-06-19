"""M1 (0.8.2 Slice 15) — the lexically-seeded PPR-fusion arm (graph mechanism KEYSTONE).

Binding spec: ``dev/plans/plan-0.8.2.md`` §4 (Slice 15) and the SIGNED
``dev/design/0.8.2-m1-multihop-harness.md`` §2 (``ppr-fusion`` mechanism, 3 steps)
+ §6 (the three built-in ablations the Slice-15 RED pins).

This is the HippoRAG-style graph arm under test (HippoRAG NeurIPS 2024
arXiv:2405.14831; HippoRAG 2 arXiv:2502.14802). It is the principled lightweight
replacement for the 0.8.1 raw-BFS arm (the documented anti-pattern — subgraph
blow-up + hub drift = the entity-co-mingling failure; see
[[graph-arm-doesnt-beat-bm25-pivot]]). PPR strictly dominates BFS, so if PPR-fusion
cannot beat the fused-RRF baseline (Slice 20), BFS never would.

Mechanism (design §2), all **$0** — pure lexical + graph, **no dense embedder**, no
API, no answerer (Slice 20 owns the priced answerer pass). Because this arm is
lexical+graph only, the CLS-pooling constraint that governs the dense-derived arms
in ``m1_baseline.py`` **does not apply** here.

  1. **Build the per-question entity graph** from the preserved extractions (union
     over the question's paragraphs): nodes = entities ∪ relation endpoints
     (normalized name), edges = the extracted relations — **body-less** (the relation
     predicate text is irrelevant to PPR topology). The graph is built with a stable
     sorted node + edge order so PageRank is byte-deterministic. Passage→entity
     membership is recorded per entity (which passage positions mention it).
  2. **Seed lexically (NOT self-seeding).** Take the BM25 top-K passages (reuse
     ``m1_baseline.bm25_rank``); the seed set = the entities appearing in those top-K
     passages. Seeds are **IDF / specificity weighted** by entity passage
     document-frequency across the pool (``log((N+1)/(df+0.5))``) so hub entities do
     not dominate (the 0.8.1 entity-co-mingling failure). FTS5-free, $0.
  3. **Propagate** ONE Personalized-PageRank pass biased to the weighted seeds via
     ``networkx.pagerank(personalization=…)`` — deterministic: fixed teleport/restart
     ``α`` (damping ``0.85`` ⇒ restart prob ``0.15``), fixed ``max_iter``+``tol``,
     stable node order, no RNG. The teleport=1.0 ablation is ``alpha=0.0``: PageRank
     then returns the personalization vector exactly, so the arm degenerates to the
     seeds (≈ BM25) — the built-in propagation sanity check (RED test (b)).
  4. **Rank + fuse.** Passage score = summed PPR mass of the entities it mentions;
     rank passages by that; then **RRF-fuse with BM25 at k=60** (reuse
     ``m1_baseline.rrf_fuse_scored``) → the final ``ppr_fusion`` ranking (HippoRAG-2's
     lesson: graph+lexical fusion beats graph-alone and avoids entity-only context
     loss).

**Membership decision (deterministic, documented).** A passage "mentions" an entity
iff that entity name is in the passage's extracted ``entities`` list. Relation
endpoints that never appear in any ``entities`` list (e.g. the extractor's
relation-only phrase nodes) still become graph nodes — they carry/relay PPR mass —
but contribute to no passage's score and can never be a seed. This keeps passage
membership, the seed set, and the IDF document-frequency a single well-defined
function of the ``entities`` lists (so the RED fixtures are provable by construction)
while preserving full relation topology for propagation.

**Integration.** This module is a *parallel* ``retrieve_ppr`` arm — it deliberately
does NOT touch ``m1_baseline.retrieve_arms`` (the four baseline arms stay
byte-identical). ``add_ppr_fusion_arm`` is the convenience that emits ``ppr_fusion``
alongside the baseline arms for the Slice-20 harness.
"""

from __future__ import annotations

import math
import re
from collections import defaultdict
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any, Optional

from eval.m1_baseline import Paragraph, Question, bm25_rank, rrf_fuse, rrf_fuse_scored

# --------------------------------------------------------------------------- #
# Config
# --------------------------------------------------------------------------- #


@dataclass(frozen=True)
class PPRConfig:
    """Frozen, deterministic PPR-fusion configuration.

    ``alpha`` is the PageRank **damping** (probability of following an edge); the
    teleport/restart probability is ``1 - alpha``. ``alpha=0.0`` ⇒ teleport=1.0 ⇒ the
    arm collapses to its seeds (the design §6 sanity ablation).
    """

    alpha: float = 0.85
    seed_k: int = 10
    # ``max_iter``/``tol`` are pinned (no RNG) and sized so the power iteration
    # converges on near-bipartite entity graphs. Bipartite/periodic structure (a
    # common entity-graph shape) makes power iteration decay *oscillate* at rate
    # ``alpha`` (the negative second eigenvalue), so reaching ``tol`` needs
    # ``log(tol)/log(alpha)`` iterations — ~85 for ``tol=1e-6`` at ``alpha=0.85``.
    # ``max_iter=1000`` is a wide deterministic margin; ``tol=1e-6`` is the networkx
    # default. (A too-strict ``tol`` with too-few iters silently fell back to the
    # seed vector — masking propagation; see the Slice-15 RED fix.)
    max_iter: int = 1000
    tol: float = 1.0e-6
    idf_weighting: bool = True


DEFAULT_PPR_CONFIG = PPRConfig()

#: RRF fusion constant — pinned at k=60 across every fused arm (design §3). Reused
#: transitively via ``m1_baseline.rrf_fuse``/``rrf_fuse_scored`` (their ``RRF_K``).
RRF_K = 60


# --------------------------------------------------------------------------- #
# Normalization + graph build
# --------------------------------------------------------------------------- #


def _norm_entity(name: Any) -> Optional[str]:
    """Normalize an entity name to a stable node key (lowercase, collapsed
    whitespace). Returns ``None`` for an unusable name."""
    if not isinstance(name, str):
        return None
    s = re.sub(r"\s+", " ", name.strip().lower())
    return s or None


@dataclass
class EntityGraph:
    """The per-question entity graph + passage membership (design §2 step 1)."""

    #: networkx undirected Graph (body-less edges), nodes in stable sorted order.
    graph: Any
    #: entity -> frozenset of passage positions whose ``entities`` list mentions it.
    entity_paras: dict[str, frozenset[int]]
    #: passage position -> frozenset of entities it mentions.
    para_entities: dict[int, frozenset[str]]
    #: stable sorted node order (drives PageRank determinism).
    nodes: tuple[str, ...]
    #: number of passages in the pool (the IDF document count).
    n_paras: int


def _relations(ext: Mapping[str, Any]) -> list[dict]:
    """Relations may be keyed ``relations`` or ``edges`` (inspect both)."""
    rels = ext.get("relations")
    if rels is None:
        rels = ext.get("edges")
    return [r for r in (rels or []) if isinstance(r, dict)]


def build_entity_graph(
    extractions_by_pos: Mapping[int, Mapping[str, Any]], n_paras: int
) -> EntityGraph:
    """Build the per-question entity graph from per-passage extractions.

    ``extractions_by_pos`` maps a passage **position** (the same index space
    ``bm25_rank`` ranks over) to its ``{"entities": [...], "relations": [...]}``
    extraction. Nodes = entities ∪ relation endpoints (normalized); edges = relations
    (undirected, **body-less**); passage membership = the ``entities`` list only.
    """
    import networkx as nx

    entity_paras: dict[str, set[int]] = defaultdict(set)
    para_entities: dict[int, set[str]] = defaultdict(set)
    all_nodes: set[str] = set()
    edges: set[tuple[str, str]] = set()

    for pos, ext in extractions_by_pos.items():
        # Passage membership = the explicit ``entities`` list (deterministic).
        for e in ext.get("entities", []) or []:
            name = e.get("name") if isinstance(e, dict) else e
            ne = _norm_entity(name)
            if ne is not None:
                entity_paras[ne].add(pos)
                para_entities[pos].add(ne)
                all_nodes.add(ne)
        # Edges (+ relation-endpoint nodes) — body-less; predicate text discarded.
        for r in _relations(ext):
            s = _norm_entity(r.get("subject"))
            o = _norm_entity(r.get("object"))
            if s is not None:
                all_nodes.add(s)
            if o is not None:
                all_nodes.add(o)
            if s is not None and o is not None and s != o:
                edges.add((s, o) if s < o else (o, s))

    nodes = tuple(sorted(all_nodes))
    g = nx.Graph()
    g.add_nodes_from(nodes)  # stable sorted insertion order
    for a, b in sorted(edges):
        g.add_edge(a, b)

    return EntityGraph(
        graph=g,
        entity_paras={k: frozenset(v) for k, v in entity_paras.items()},
        para_entities={k: frozenset(v) for k, v in para_entities.items()},
        nodes=nodes,
        n_paras=n_paras,
    )


# --------------------------------------------------------------------------- #
# Seeding (lexical, IDF-weighted) — design §2 step 2
# --------------------------------------------------------------------------- #


def seed_weights(
    bm25_ranking: Sequence[int], eg: EntityGraph, cfg: PPRConfig
) -> dict[str, float]:
    """Weighted seed set: entities appearing in the BM25 top-``seed_k`` passages,
    IDF/specificity-weighted by passage document-frequency.

    ``log((N+1)/(df+0.5))`` is large for a rare (specific) entity and small for a hub
    entity that appears in many passages — suppressing the hub-drift failure. With
    ``idf_weighting=False`` every seed gets weight ``1.0`` (the ablation knob).
    """
    top = set(bm25_ranking[: cfg.seed_k])
    n = max(eg.n_paras, 1)
    weights: dict[str, float] = {}
    for ent, paras in eg.entity_paras.items():
        if paras & top:
            if cfg.idf_weighting:
                df = len(paras)
                w = math.log((n + 1) / (df + 0.5))
                weights[ent] = w if w > 0.0 else 1.0e-6
            else:
                weights[ent] = 1.0
    return weights


# --------------------------------------------------------------------------- #
# Propagation (one Personalized-PageRank pass) — design §2 step 3
# --------------------------------------------------------------------------- #


def ppr_mass(eg: EntityGraph, seeds: Mapping[str, float], cfg: PPRConfig) -> dict[str, float]:
    """One deterministic Personalized-PageRank pass biased to the weighted seeds.

    Returns the per-node PPR mass. At ``alpha=0.0`` (teleport=1.0) PageRank returns
    the (normalized) personalization vector exactly, so non-seed nodes have mass 0 —
    the propagation-off collapse. ``dangling=personalization`` makes dangling-node
    redistribution deterministic and seed-biased (no uniform leak)."""
    import networkx as nx

    if not seeds or eg.graph.number_of_nodes() == 0:
        return {}
    total = float(sum(seeds.values()))
    if total <= 0.0:
        return {n: 0.0 for n in eg.nodes}
    personalization = {n: 0.0 for n in eg.nodes}
    for n, w in seeds.items():
        if n in personalization:
            personalization[n] = float(w) / total
    try:
        pr = nx.pagerank(
            eg.graph,
            alpha=cfg.alpha,
            personalization=personalization,
            max_iter=cfg.max_iter,
            tol=cfg.tol,
            dangling=personalization,
        )
    except nx.PowerIterationFailedConvergence:
        # Loud deterministic retry with a 10× iteration budget before giving up —
        # never silently collapse to the seed vector (that masks propagation).
        pr = nx.pagerank(
            eg.graph,
            alpha=cfg.alpha,
            personalization=personalization,
            max_iter=cfg.max_iter * 10,
            tol=cfg.tol,
            dangling=personalization,
        )
    return {n: float(pr.get(n, 0.0)) for n in eg.nodes}


# --------------------------------------------------------------------------- #
# Rank + fuse — design §2 step 4
# --------------------------------------------------------------------------- #


def passage_scores(eg: EntityGraph, mass: Mapping[str, float]) -> dict[int, float]:
    """Per-passage score = summed PPR mass of the entities the passage mentions."""
    scores = {i: 0.0 for i in range(eg.n_paras)}
    for pos, ents in eg.para_entities.items():
        scores[pos] = float(sum(mass.get(e, 0.0) for e in ents))
    return scores


def _rank_from_scores(scores: Mapping[int, float], n_paras: int) -> list[int]:
    """Stable rank: higher score first, ties broken by ascending passage index."""
    return sorted(range(n_paras), key=lambda i: (-scores.get(i, 0.0), i))


def retrieve_ppr(
    query: str,
    passages: Sequence[Paragraph],
    extractions_by_pos: Mapping[int, Mapping[str, Any]],
    cfg: PPRConfig = DEFAULT_PPR_CONFIG,
) -> dict[str, Any]:
    """Run the PPR-fusion arm over one question's identical passage pool.

    ``extractions_by_pos`` is keyed by passage **position** (``0..len(passages)-1``),
    the same index space ``bm25_rank`` and the returned rankings use. Returns the
    final ``ppr_fusion`` ranking plus the intermediate signals the RED ablations pin
    (``ppr_only``, ``ppr_mass``, ``passage_scores``, ``seeds``, ``bm25``).
    """
    n = len(passages)
    bm = bm25_rank(query, passages)
    eg = build_entity_graph(extractions_by_pos, n)
    seeds = seed_weights(bm, eg, cfg)
    mass = ppr_mass(eg, seeds, cfg)
    pscores = passage_scores(eg, mass)
    ppr_only = _rank_from_scores(pscores, n)
    fused = rrf_fuse([bm, ppr_only], k=RRF_K)
    return {
        "bm25": bm,
        "ppr_only": ppr_only,
        "ppr_fusion": fused,
        "ppr_fusion_scored": rrf_fuse_scored([bm, ppr_only], k=RRF_K),
        "passage_scores": pscores,
        "ppr_mass": mass,
        "seeds": seeds,
        "n_entities": len(eg.nodes),
        "n_edges": eg.graph.number_of_edges(),
    }


# --------------------------------------------------------------------------- #
# Question-level convenience (maps Paragraph.idx → position) + harness hook
# --------------------------------------------------------------------------- #


def extractions_for_question(
    question: Question, question_extractions: Mapping[str, Mapping[str, Any]]
) -> dict[int, Mapping[str, Any]]:
    """Map a question's per-paragraph extractions (keyed ``"{qid}#{para_idx}"`` OR by
    bare ``para_idx``) to ``{passage_position: extraction}`` — the position space the
    arm uses. Paragraphs without an extraction are simply absent (no graph mass)."""
    out: dict[int, Mapping[str, Any]] = {}
    for pos, p in enumerate(question.paragraphs):
        ext = question_extractions.get(f"{question.id}#{p.idx}")
        if ext is None:
            ext = question_extractions.get(str(p.idx))
        if ext is None:
            ext = question_extractions.get(p.idx)  # type: ignore[arg-type]
        if ext is not None:
            out[pos] = ext
    return out


def ppr_fusion_ranking(
    question: Question,
    question_extractions: Mapping[str, Mapping[str, Any]],
    cfg: PPRConfig = DEFAULT_PPR_CONFIG,
) -> list[int]:
    """The final ``ppr_fusion`` passage-position ranking for one ``Question``."""
    by_pos = extractions_for_question(question, question_extractions)
    return retrieve_ppr(question.question, question.paragraphs, by_pos, cfg)["ppr_fusion"]


def add_ppr_fusion_arm(
    arm_rankings: dict[str, Any],
    question: Question,
    question_extractions: Mapping[str, Mapping[str, Any]],
    cfg: PPRConfig = DEFAULT_PPR_CONFIG,
) -> dict[str, Any]:
    """Emit ``ppr_fusion`` alongside the baseline arms (Slice-20 harness hook).

    Takes the dict ``m1_baseline.retrieve_arms`` returns (left byte-identical) and
    adds the ``ppr_fusion`` ranking — the 5th arm Slice 20 adjudicates against the
    fixed ``fused-RRF`` comparator. Does not mutate the input's other arms.
    """
    out = dict(arm_rankings)
    out["ppr_fusion"] = ppr_fusion_ranking(question, question_extractions, cfg)
    return out
