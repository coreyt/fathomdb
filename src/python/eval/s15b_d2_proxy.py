"""Slice-15b — D2 content-at-scale proxy ($0 / LLM-free at measure).

Decide, cheaply and before any schema migration, whether **fielded enrichment
content** (entity/fact keys appended to a doc's OWN FTS content — one row per doc,
NOT a graph arm) carries *retrieval* value at scale, separated from the length
artifact. This module is the executable runner + the pure statistics it gates on;
it **CONSUMES** the frozen :func:`eval.decision_rule_083.probe_15b_pass` and must
not reimplement or relax it.

Binding contract: ``dev/design/0.8.3-slice-15b-d2-proxy.md`` (decision-ready,
codex round-3 converged). Pinned decisions (do NOT deviate):

* **Criterion 1 (content):** paired ``enriched − length-matched-placebo`` per-query
  recall margin; fixed-seed percentile bootstrap CI; gate ``margin_ci_lo > 0``.
  Also emit the 15b paired **MDE**.
* **Criterion 2 (length-norm) — pre-registered, post-selection-free, length-isolated:**
  ``b_hi = 0.75`` (production) and ``b_lo = 0.00`` (length-norm OFF), no argmax.
  ``penalty_present := (plain − placebo)@b_hi paired CI_lo > 0``.
  ``neutralized := (plain − placebo)@b_lo paired CI ⊂ [−δ_d2, +δ_d2]`` (TOST
  equivalence, ``δ_d2 = 0.03``); if that CI is too wide to fit the band (the
  neutralization contrast is itself underpowered) the leg is **INCONCLUSIVE**, never
  a silent ``neutralized``.
  ``removes_length_norm_penalty := (NOT penalty_present) OR (penalty_present AND
  neutralized)``.
* **3-way verdict:** ``PASS`` (``probe_15b_pass`` True) ⇒ Slice-25 eligible;
  ``FAIL`` at power (False AND ``enriched−placebo`` MDE ≤ δ_d2) ⇒ defer;
  ``INCONCLUSIVE`` (MDE > δ_d2, or neutralization underpowered) ⇒ escalate, do NOT
  drop D2. INCONCLUSIVE overrides every "FAIL ⇒ defer".

Extraction is OFFLINE-BUILD / $0: the runner reuses cached extractions for the
``--full`` arm; **measure time is LLM-free**. ``--smoke`` runs end-to-end on a tiny
in-module synthetic corpus with a pure-Python BM25 backend (no native extension, no
network, no GPU), so it is deterministic and runnable anywhere. The heavy
FathomDB-FTS / cached-extraction ``--full`` run is owned by the orchestrator.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import random
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Any, Optional

from eval.decision_rule_083 import probe_15b_pass
from eval.p0a_base_retrieval import (
    DEFAULT_DATASET,
    DEFAULT_SPLIT,
    SMOKE_CLASSES,
    SmokeQuestion,
    SmokeSet,
    hit_at_k,
    load_lme_smoke,
)
from eval.r2_parity_eval import NaiveRAGAdapter, session_id_of
from eval.r6_index_key_enrichment import (
    _entities,
    _facts,
    _fts_adapter,
    enrich_doc,
    placebo_doc,
)

# --------------------------------------------------------------------------- #
# Frozen constants (design §2; pinned — auditable).
# --------------------------------------------------------------------------- #

#: The internal D2 lift-of-interest — the smallest enriched−placebo recall lift that
#: would justify a schema migration. NOT the external near-parity band ε (design §2,
#: codex concern #4). Used for the equivalence margin AND the power guard.
DELTA_D2: float = 0.03

#: Pre-registered length-normalization ``b`` points (NO argmax post-selection).
B_HI: float = 0.75  # production length-norm ON
B_LO: float = 0.00  # length-norm OFF
B_SECONDARY: float = 0.25  # reported only, non-gating

#: 95% two-sided normal-approx multiplier (matches ``eval.paired_power_proxy._Z``).
_Z: float = 1.96

#: Deterministic defaults.
DEFAULT_N_BOOT: int = 10000
DEFAULT_SEED: int = 20260622

#: Recall@K cut used for the proxy.
RECALL_K: int = 10

NeutralizationStatus = str  # "neutralized" | "not_neutralized" | "inconclusive"
RemovesStatus = str  # "removed" | "not_removed" | "inconclusive"
Verdict = str  # "PASS" | "FAIL" | "INCONCLUSIVE"


# --------------------------------------------------------------------------- #
# Pure statistics (deterministic; stdlib only so they run without numpy/native).
# --------------------------------------------------------------------------- #
def paired_deltas(
    a: Mapping[str, float],
    b: Mapping[str, float],
) -> list[float]:
    """Return ``a[q] − b[q]`` over the common keys, in sorted-key order.

    Pairs strictly by key (per-query); a key present in only one arm is dropped so
    the bootstrap stays paired. Raises :class:`ValueError` if there is no overlap
    (a malformed/empty pairing must fail loudly, never yield a vacuous CI).
    """
    keys = sorted(set(a) & set(b))
    if not keys:
        raise ValueError("paired_deltas: no overlapping keys (empty pairing)")
    return [float(a[k]) - float(b[k]) for k in keys]


def _mean(values: Sequence[float]) -> float:
    return sum(values) / len(values)


def _popvar(values: Sequence[float]) -> float:
    n = len(values)
    if n == 0:
        return 0.0
    m = sum(values) / n
    return sum((v - m) ** 2 for v in values) / n


def _quantile(sorted_xs: Sequence[float], q: float) -> float:
    """Linear-interpolation quantile (matches ``numpy.quantile`` default)."""
    n = len(sorted_xs)
    if n == 1:
        return float(sorted_xs[0])
    pos = q * (n - 1)
    lo = math.floor(pos)
    hi = math.ceil(pos)
    frac = pos - lo
    return float(sorted_xs[lo] + (sorted_xs[hi] - sorted_xs[lo]) * frac)


def paired_bootstrap_ci(
    deltas: Sequence[float],
    *,
    seed: int = DEFAULT_SEED,
    n_boot: int = DEFAULT_N_BOOT,
    alpha: float = 0.05,
) -> tuple[float, float]:
    """Percentile paired-bootstrap CI for the **mean** of ``deltas``.

    Resamples ``deltas`` with replacement ``n_boot`` times under a fixed-seed RNG
    and returns the ``(alpha/2, 1−alpha/2)`` percentiles of the resampled means.
    Deterministic: same ``deltas``/``seed``/``n_boot`` → identical bounds.
    """
    if not deltas:
        raise ValueError("paired_bootstrap_ci: empty deltas")
    n = len(deltas)
    rng = random.Random(seed)
    boot_means: list[float] = []
    for _ in range(n_boot):
        s = 0.0
        for _ in range(n):
            s += deltas[rng.randrange(n)]
        boot_means.append(s / n)
    boot_means.sort()
    return _quantile(boot_means, alpha / 2.0), _quantile(boot_means, 1.0 - alpha / 2.0)


def paired_mde(deltas: Sequence[float]) -> float:
    """Minimal detectable effect for the paired mean at this N.

    ``MDE = Z · sqrt(var_diff / n)`` — the half-width of the normal-approx 95% CI on
    the paired mean, i.e. the smallest true effect this sample could resolve. Matches
    the ``eval.paired_power_proxy`` convention (``_implied_n`` inverts this exact
    relation: ``n = (Z/eps)^2 · var``). Zero variance ⇒ MDE 0.
    """
    if not deltas:
        raise ValueError("paired_mde: empty deltas")
    n = len(deltas)
    return _Z * math.sqrt(_popvar(deltas) / n)


# --------------------------------------------------------------------------- #
# Pure classification (no RNG; the gate logic — directly unit-tested).
# --------------------------------------------------------------------------- #
def neutralization_classify(
    ci_lo: float,
    ci_hi: float,
    mde: float,
    *,
    delta_d2: float = DELTA_D2,
) -> NeutralizationStatus:
    """TOST-style classification of the ``(plain − placebo)@b_lo`` length contrast.

    * ``neutralized`` — the paired CI is **fully contained** in ``[−δ, +δ]`` (the
      residual pure-length effect is demonstrably small: equivalence, not merely
      absence-of-evidence).
    * ``inconclusive`` — the CI is not contained AND the contrast is underpowered
      (``mde > δ``): too wide to fit the band, so we cannot conclude either way
      (turning "not significant" into "removed" is forbidden — design §2).
    * ``not_neutralized`` — a real residual effect outside the band at adequate power.
    """
    if ci_lo >= -delta_d2 and ci_hi <= delta_d2:
        return "neutralized"
    if mde > delta_d2:
        return "inconclusive"
    return "not_neutralized"


def removes_length_norm_status(
    penalty_present: bool,
    neutralization: NeutralizationStatus,
) -> RemovesStatus:
    """Combine the two pre-registered legs into ``removes_length_norm_penalty``.

    ``removed`` iff the enriched arm is not held back by an *unremoved* length-norm
    penalty: either no penalty exists (``NOT penalty_present``) or it exists and was
    neutralized. An **underpowered neutralization** ⇒ ``inconclusive`` (never a
    silent ``removed``) — this propagates to an INCONCLUSIVE 15b verdict (design §2).
    """
    if not penalty_present:
        return "removed"
    if neutralization == "neutralized":
        return "removed"
    if neutralization == "not_neutralized":
        return "not_removed"
    return "inconclusive"


def classify_verdict(
    margin_ci_lo: float,
    margin_mde: float,
    removes: RemovesStatus,
    *,
    delta_d2: float = DELTA_D2,
) -> Verdict:
    """Map the two criteria to the frozen 3-way 15b verdict.

    The pass gate is routed through the **real** :func:`probe_15b_pass` (frozen
    rule), never re-implemented. Order (design §1, INCONCLUSIVE overrides FAIL):

    1. neutralization underpowered (``removes == "inconclusive"``) ⇒ ``INCONCLUSIVE``.
    2. else ``PASS`` iff ``probe_15b_pass`` is True.
    3. else (not a pass) ``INCONCLUSIVE`` if the enriched−placebo paired MDE exceeds
       ``δ_d2`` (underpowered to detect a migration-worthy lift), else ``FAIL``
       (at power ⇒ Slice-25 defers).
    """
    if removes == "inconclusive":
        return "INCONCLUSIVE"
    removes_bool = removes == "removed"
    passed = probe_15b_pass(
        {
            "recall": 0.0,
            "margin_ci_lo": margin_ci_lo,
            "removes_length_norm_penalty": removes_bool,
        },
        {"recall": 0.0},
    )
    if passed:
        return "PASS"
    if margin_mde > delta_d2:
        return "INCONCLUSIVE"
    return "FAIL"


# --------------------------------------------------------------------------- #
# Enriched / placebo corpus build (reuse r6; extend placebo to the FULL key set).
# --------------------------------------------------------------------------- #
def _key_tokens(graph: Mapping[str, Any]) -> list[str]:
    """Every whitespace token the enrichment appends for ``graph`` — across the FULL
    key set actually used (entities AND fact triples). The placebo's foreign pool is
    built to **exclude** these so the placebo is foreign-only w.r.t. every key type
    (design §2 key-set↔placebo invariant), not just entities."""
    toks: list[str] = []
    for name in _entities(dict(graph)):
        toks.extend(name.split())
    for fact in _facts(dict(graph)):
        toks.extend(fact.split())
    return toks


def build_enriched_placebo(
    documents: Mapping[str, str],
    graphs: Mapping[str, Mapping[str, Any]],
    *,
    seed: int = DEFAULT_SEED,
) -> tuple[dict[str, str], dict[str, str]]:
    """Build the enriched arm (own keys appended) and a length-matched, foreign-only
    placebo arm. The placebo is token-matched to the **full** enriched addition
    (entities + facts) and its foreign pool excludes the doc's own key tokens of
    EVERY type (design §2). Deterministic (per-doc stable hashed seed)."""
    enriched = {s: enrich_doc(b, dict(graphs.get(s, {}))) for s, b in documents.items()}
    global_tokens = [t for g in graphs.values() for t in _key_tokens(g)]
    placebo: dict[str, str] = {}
    for s, body in documents.items():
        g = dict(graphs.get(s, {}))
        own = set(_key_tokens(g))
        pool = [t for t in global_tokens if t not in own]
        dseed = seed ^ int.from_bytes(
            hashlib.blake2b(s.encode(), digest_size=4).digest(), "big"
        )
        placebo[s] = placebo_doc(body, g, foreign=pool, seed=dseed)
    return enriched, placebo


# --------------------------------------------------------------------------- #
# Per-query recall (captures the paired per-query vector the bootstrap needs).
# --------------------------------------------------------------------------- #
def per_query_recall(
    smoke: SmokeSet,
    adapter: Any,
    *,
    k: int = RECALL_K,
) -> dict[str, float]:
    """Return ``{qid: recall_hit@k}`` for the scored (non-abstention) questions.

    Uses :func:`eval.p0a_base_retrieval.hit_at_k` (multi_session full-gold-set rule,
    abstention-excluded). ``session_id_of`` collapses any chunk suffix so BM25 and
    FathomDB-FTS doc ids both reduce to the corpus session id."""
    out: dict[str, float] = {}
    for q in smoke.questions:
        if not q.gold_sessions:
            continue
        hits = adapter.retrieve(q.question, k)
        ranked = tuple(session_id_of(h.doc_id) for h in hits)
        h = hit_at_k(q.gold_sessions, ranked, k, q.reporting_class)
        if h is not None:
            out[q.qid] = h
    return out


# --------------------------------------------------------------------------- #
# Top-level proxy computation.
# --------------------------------------------------------------------------- #
def compute_proxy(
    *,
    enriched_pq: Mapping[str, float],
    placebo_pq: Mapping[str, float],
    plain_bhi_pq: Mapping[str, float],
    placebo_bhi_pq: Mapping[str, float],
    plain_blo_pq: Mapping[str, float],
    placebo_blo_pq: Mapping[str, float],
    seed: int = DEFAULT_SEED,
    n_boot: int = DEFAULT_N_BOOT,
    delta_d2: float = DELTA_D2,
) -> dict[str, Any]:
    """Run both criteria on per-query recall vectors and return the 3-way verdict.

    Criterion 1 (content) uses the enriched/placebo arms; criterion 2 (length-norm)
    uses plain-vs-placebo at the pre-registered ``b_hi`` / ``b_lo``. All CIs share
    the one fixed-seed bootstrap helper. The pass gate is the real
    :func:`probe_15b_pass`; the 3-way mapping is :func:`classify_verdict`."""
    # Criterion 1 — content margin (enriched − placebo).
    margin_deltas = paired_deltas(enriched_pq, placebo_pq)
    margin_ci_lo, margin_ci_hi = paired_bootstrap_ci(
        margin_deltas, seed=seed, n_boot=n_boot
    )
    margin_mde = paired_mde(margin_deltas)

    # Criterion 2a — length-norm penalty present? (plain − placebo) @ b_hi.
    penalty_deltas = paired_deltas(plain_bhi_pq, placebo_bhi_pq)
    penalty_ci_lo, penalty_ci_hi = paired_bootstrap_ci(
        penalty_deltas, seed=seed, n_boot=n_boot
    )
    penalty_present = penalty_ci_lo > 0.0

    # Criterion 2b — neutralized at b_lo? (plain − placebo) @ b_lo, TOST.
    neut_deltas = paired_deltas(plain_blo_pq, placebo_blo_pq)
    neut_ci_lo, neut_ci_hi = paired_bootstrap_ci(neut_deltas, seed=seed, n_boot=n_boot)
    neut_mde = paired_mde(neut_deltas)
    neutralization = neutralization_classify(
        neut_ci_lo, neut_ci_hi, neut_mde, delta_d2=delta_d2
    )

    removes = removes_length_norm_status(penalty_present, neutralization)
    verdict = classify_verdict(margin_ci_lo, margin_mde, removes, delta_d2=delta_d2)

    enriched_recall = round(_mean(list(enriched_pq.values())), 6)
    placebo_recall = round(_mean(list(placebo_pq.values())), 6)

    # Echo the frozen-probe inputs/outcome for audit (probe is the gate of record).
    removes_bool_for_probe = removes == "removed"
    probe_pass = (
        None
        if removes == "inconclusive"
        else probe_15b_pass(
            {
                "recall": enriched_recall,
                "margin_ci_lo": margin_ci_lo,
                "removes_length_norm_penalty": removes_bool_for_probe,
            },
            {"recall": placebo_recall},
        )
    )

    return {
        "verdict": verdict,
        "slice_25_eligible": verdict == "PASS",
        "probe_15b_pass": probe_pass,
        "delta_d2": delta_d2,
        "n_questions_scored": len(margin_deltas),
        "criterion_1_content": {
            "enriched_recall": enriched_recall,
            "placebo_recall": placebo_recall,
            "margin_point": round(_mean(margin_deltas), 6),
            "margin_ci_lo": round(margin_ci_lo, 6),
            "margin_ci_hi": round(margin_ci_hi, 6),
            "margin_mde": round(margin_mde, 6),
            "pass": margin_ci_lo > 0.0,
        },
        "criterion_2_length_norm": {
            "b_hi": B_HI,
            "b_lo": B_LO,
            "penalty_present": penalty_present,
            "penalty_ci_lo": round(penalty_ci_lo, 6),
            "penalty_ci_hi": round(penalty_ci_hi, 6),
            "neutralization": neutralization,
            "neut_ci_lo": round(neut_ci_lo, 6),
            "neut_ci_hi": round(neut_ci_hi, 6),
            "neut_mde": round(neut_mde, 6),
            "removes_length_norm_penalty": removes,
        },
        "seed": seed,
        "n_boot": n_boot,
    }


# --------------------------------------------------------------------------- #
# Tiny synthetic smoke corpus ($0; no datasets, no native engine, no GPU).
# --------------------------------------------------------------------------- #
def _smoke_corpus() -> tuple[SmokeSet, dict[str, dict[str, Any]]]:
    """A deterministic toy LME-shaped corpus + graphs that exercises the full wiring.

    Each gold session's body OMITS a unique entity token that the question asks for;
    the enrichment appends that token (making the doc findable), while the placebo
    appends foreign tokens — so the enriched arm should out-recall the placebo. This
    is a *wiring* fixture (any verdict is acceptable), not a powered measurement."""
    classes = ("factoid", "temporal", "knowledge_update", "multi_session")
    documents: dict[str, str] = {}
    graphs: dict[str, dict[str, Any]] = {}
    questions: list[SmokeQuestion] = []
    for ci, cls in enumerate(classes):
        for i in range(4):
            sid = f"s_{cls}_{i}"
            ent = f"Zenith{ci}{i}"  # a token that does NOT appear in the body
            partner = f"Apex{ci}{i}"
            documents[sid] = (
                f"A session about ordinary daily plans number {ci}{i} with no special token."
            )
            graphs[sid] = {
                "entities": [{"name": ent}, {"name": partner}],
                "relations": [
                    {"subject": ent, "predicate": "meets", "object": partner}
                ],
            }
            questions.append(
                SmokeQuestion(
                    qid=f"q_{cls}_{i}",
                    reporting_class=cls,
                    question=ent,  # ask for the enrichment-only token
                    answer=ent,
                    gold_sessions=(sid,),
                    haystack_session_ids=tuple(documents),
                )
            )
    return SmokeSet(questions=questions, documents=documents), graphs


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #
def _build_arms_and_run(
    smoke: SmokeSet,
    graphs: Mapping[str, Mapping[str, Any]],
    *,
    backend: str,
    db_dir: Path,
    seed: int,
    n_boot: int,
) -> dict[str, Any]:
    enriched, placebo = build_enriched_placebo(dict(smoke.documents), graphs, seed=seed)

    # Criterion 1 — production arms. ``fts`` uses the FathomDB FTS engine (full run,
    # native, fixed b≈0.75); ``bm25`` uses the pure-Python BM25 (smoke, no native).
    if backend == "fts":
        plain_ad: Any = _fts_adapter(dict(smoke.documents), str(db_dir / "fts_plain.sqlite"))
        enriched_ad: Any = _fts_adapter(enriched, str(db_dir / "fts_enriched.sqlite"))
        placebo_ad: Any = _fts_adapter(placebo, str(db_dir / "fts_placebo.sqlite"))
    elif backend == "bm25":
        plain_ad = NaiveRAGAdapter(dict(smoke.documents))
        enriched_ad = NaiveRAGAdapter(enriched)
        placebo_ad = NaiveRAGAdapter(placebo)
    else:  # pragma: no cover - guarded by argparse choices
        raise ValueError(f"unknown backend: {backend!r}")

    enriched_pq = per_query_recall(smoke, enriched_ad)
    placebo_pq = per_query_recall(smoke, placebo_ad)

    # Criterion 2 — length-norm legs ALWAYS use tunable-b BM25 (FathomDB FTS has a
    # fixed, un-tunable b). Pre-registered b_hi / b_lo; placebo is the pure-length
    # control by construction.
    plain_bhi = NaiveRAGAdapter(dict(smoke.documents), b=B_HI)
    placebo_bhi = NaiveRAGAdapter(placebo, b=B_HI)
    plain_blo = NaiveRAGAdapter(dict(smoke.documents), b=B_LO)
    placebo_blo = NaiveRAGAdapter(placebo, b=B_LO)

    proxy = compute_proxy(
        enriched_pq=enriched_pq,
        placebo_pq=placebo_pq,
        plain_bhi_pq=per_query_recall(smoke, plain_bhi),
        placebo_bhi_pq=per_query_recall(smoke, placebo_bhi),
        plain_blo_pq=per_query_recall(smoke, plain_blo),
        placebo_blo_pq=per_query_recall(smoke, placebo_blo),
        seed=seed,
        n_boot=n_boot,
    )

    # Close any engines the FTS backend opened.
    for ad in (plain_ad, enriched_ad, placebo_ad):
        eng = getattr(ad, "_engine", None)
        if eng is not None:
            eng.close()
    return proxy


def _corpus_hash(documents: Mapping[str, str]) -> str:
    h = hashlib.blake2b(digest_size=16)
    for sid in sorted(documents):
        h.update(sid.encode())
        h.update(b"\x00")
        h.update(documents[sid].encode())
        h.update(b"\x01")
    return h.hexdigest()


def main(argv: Optional[list[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="Slice-15b D2 content-at-scale proxy")
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--smoke", action="store_true", help="tiny synthetic corpus, BM25 backend ($0, no native)")
    mode.add_argument("--full", action="store_true", help="power-sized LME + cached extractions, FathomDB-FTS backend (orchestrator-owned)")
    ap.add_argument("--per-class", type=int, default=0, help="full: per-class question N (power-sized; from Slice 5)")
    ap.add_argument("--seed", type=int, default=DEFAULT_SEED)
    ap.add_argument("--n-boot", type=int, default=DEFAULT_N_BOOT)
    ap.add_argument("--graphs", default="data/corpus-data/graph-cache/0.8.2-m1-v1/extractions.json",
                    help="full: cached OFFLINE-BUILD extractions (reuse; LLM-free at measure)")
    ap.add_argument("--db-dir", default="/tmp/s15b")
    ap.add_argument("--lme-endpoint", default=None, help="full: local Qwen endpoint URL (pinned in output; fail-closed)")
    ap.add_argument("--lme-model", default=None, help="full: served model name (pinned in output)")
    ap.add_argument("--output", required=True)
    args = ap.parse_args(argv)

    db_dir = Path(args.db_dir)
    db_dir.mkdir(parents=True, exist_ok=True)

    if args.full:
        if args.per_class <= 0:
            ap.error("--full requires --per-class > 0 (power-sized; re-pin from Slice 5)")
        smoke = load_lme_smoke(
            DEFAULT_DATASET, DEFAULT_SPLIT, per_class=args.per_class,
            seed=args.seed, classes=SMOKE_CLASSES,
        )
        graphs_raw = json.loads(Path(args.graphs).read_text())
        graphs = {k: v for k, v in graphs_raw.items() if k in smoke.documents}
        cov = sum(1 for s in smoke.documents if graphs.get(s))
        backend = "fts"
        extraction = {
            "source": args.graphs,
            "coverage": cov,
            "n_sessions": len(smoke.documents),
            "lme_endpoint": args.lme_endpoint,
            "lme_model": args.lme_model,
        }
        mode_name = "full"
    else:
        smoke, graphs = _smoke_corpus()
        backend = "bm25"
        extraction = {"source": "synthetic-smoke", "coverage": len(graphs),
                      "n_sessions": len(smoke.documents)}
        mode_name = "smoke"

    proxy = _build_arms_and_run(
        smoke, graphs, backend=backend, db_dir=db_dir, seed=args.seed, n_boot=args.n_boot,
    )

    result = {
        "mode": f"s15b-d2-proxy-{mode_name}",
        "backend": backend,
        "n_questions": len(smoke.questions),
        "n_sessions": len(smoke.documents),
        "corpus_hash": _corpus_hash(smoke.documents),
        "extraction": extraction,
        "footprint": "OFFLINE-BUILD / EVAL-ONLY / LLM-free-at-measure",
        **proxy,
    }
    Path(args.output).write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(f"[s15b] wrote {args.output}")
    print(f"[s15b] verdict={proxy['verdict']}  slice_25_eligible={proxy['slice_25_eligible']}  "
          f"probe_15b_pass={proxy['probe_15b_pass']}")
    c1 = proxy["criterion_1_content"]
    c2 = proxy["criterion_2_length_norm"]
    print(f"[s15b] C1 margin={c1['margin_point']:+.3f} ci=[{c1['margin_ci_lo']:+.3f},"
          f"{c1['margin_ci_hi']:+.3f}] mde={c1['margin_mde']:.3f}")
    print(f"[s15b] C2 penalty_present={c2['penalty_present']} neutralization={c2['neutralization']} "
          f"removes={c2['removes_length_norm_penalty']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
