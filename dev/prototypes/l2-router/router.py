"""Agent-side L2 router PROTOTYPE + dispatcher pre-stage (0.8.11 Slice 35).

CALLER-SIDE, $0, NO LLM. **Recommends a retrieval stack WITHOUT executing it.**
This is the DP-A hedge: a working agent-side router that ships regardless of the
0.8.12 EXP-S locus verdict, and the keystone hand-off the 0.8.15 dispatcher
integrates (it consumes ``registry.json`` rather than reinventing the tuples).

HARD constraint (R-L2-4): this lives OUTSIDE the shipped library. It imports
NOTHING from ``fathomdb`` (engine/SDK) and runs no retrieval — a reviewer sees
zero diff to ``src/rust`` / ``src/python/fathomdb`` / ``src/ts``.

Intent resolution preference order (PSD §II.A):
  (1) ``agent_hint`` if given  -> used VERBATIM, confidence=1.0, NO fallback to
      the internal classifier (R-L2-3). The agent owns intent.
  (2) internal lexical classifier fallback (mirrors the Slice-20 Rocchio
      TF-IDF nearest-centroid; a lower-bound proxy, macro 0.768 — fine for a
      prototype). Provider-callback (preference #2 in the PSD) is out of scope
      for a $0 caller-side prototype.

feedback_arm is ALWAYS False (EXP-AF KILL, Slice 30): the router stays on
internal ce_score; there is no agent-signal escalation loop.
"""
from __future__ import annotations

import json
import math
import re
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

HERE = Path(__file__).resolve().parent
REGISTRY_PATH = HERE / "registry.json"

_TOKEN_RE = re.compile(r"[a-z0-9]+")


# --------------------------------------------------------------------------- #
# Public types
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class Recommendation:
    """A recommended plan — emitted WITHOUT executing retrieval (contract §4)."""

    intent: str            # one of the 5 classes (agent_hint if given, else classifier)
    stack: list[str]       # operator chain from the intent's EXP-B' tuple
    config: dict           # (index, retrieval, alpha, pool_n, mmr, recency, forbidden_ops)
    confidence: float      # classifier confidence (1.0 if agent_hint overrides)
    cost_tier: str         # "low" | "medium" | "high" from Gate-2 per-arm cost tiers
    rationale: str         # human-readable: chosen stack, runner-up, why
    feedback_arm: bool     # always False (EXP-AF KILL); no agent-signal loop


class ForbiddenCompositionError(ValueError):
    """Raised when a stack contains an operator forbidden for the intent.

    This is the EXP-B'.5 router-isolation seam the 0.8.15 plan validator inherits:
    map_reduce_qfs / community_summary are valid ONLY for ``global`` and forbidden
    on needle/multi_session/temporal/multi_hop (the §II.B blind-distiller -0.362
    cross-wire confirmed by EXP-Fr-acc base).
    """


class UnknownIntentError(ValueError):
    """Raised when an agent_hint is not one of the 5 registered intent classes."""


# --------------------------------------------------------------------------- #
# Internal lexical classifier (fallback only; mirrors Slice-20 Rocchio TF-IDF)
# --------------------------------------------------------------------------- #
# Seed phrases per class — a self-contained, $0, deterministic stand-in for the
# Slice-20 classifier trained on the (gitignored EVAL-ONLY) corpora. It is a
# lower-bound proxy by design; the agent-passed label (preference #1) is always
# preferred. Measured fallback accuracy = macro 0.768 (dev/plans/runs/fracc-base.md).
_SEED_PHRASES: dict[str, list[str]] = {
    "needle": [
        "what is the capital of france",
        "who is the author of this book",
        "what color is her car",
        "what is my phone number",
        "where does john work",
        "what did she order at the restaurant",
    ],
    "multi_session": [
        "what have we discussed across our conversations about my job",
        "based on everything you know about me what is my favorite food",
        "in our previous chats what did i say about my sister",
        "summarize what i have told you over time about my hobbies",
        "across all sessions what pets do i own",
    ],
    "temporal": [
        "what did i do before i moved to seattle",
        "what happened after the meeting last week",
        "list the events in chronological order",
        "when did i start my new job relative to the wedding",
        "what was the sequence of events that day",
    ],
    "global": [
        "across the dataset what are the main themes",
        "what are the overall trends in the corpus",
        "summarize the key topics discussed throughout all the documents",
        "what is the big picture across the entire collection",
        "give me a high level overview of the whole dataset",
    ],
    "multi_hop": [
        "which company employs the person who founded the charity",
        "what is the capital of the country where the inventor was born",
        "who directed the film that starred the actor from this show",
        "what year did the team that won the championship relocate",
        "which author wrote the book that inspired the movie she watched",
    ],
}


def _tokenize(text: str) -> list[str]:
    return _TOKEN_RE.findall(text.lower())


def _fit_tfidf(train_texts: list[str]) -> tuple[dict[str, int], list[float]]:
    df: Counter[str] = Counter()
    for t in train_texts:
        for tok in set(_tokenize(t)):
            df[tok] += 1
    vocab = {tok: i for i, tok in enumerate(sorted(df))}
    n_docs = len(train_texts)
    idf = [0.0] * len(vocab)
    for tok, i in vocab.items():
        idf[i] = math.log((1.0 + n_docs) / (1.0 + df[tok])) + 1.0
    return vocab, idf


def _vectorize(text: str, vocab: dict[str, int], idf: list[float]) -> list[float]:
    vec = [0.0] * len(vocab)
    for tok, c in Counter(t for t in _tokenize(text) if t in vocab).items():
        vec[vocab[tok]] = c * idf[vocab[tok]]
    nrm = math.sqrt(sum(v * v for v in vec))
    if nrm:
        vec = [v / nrm for v in vec]
    return vec


class _LexicalClassifier:
    """Pure-Python TF-IDF nearest-centroid (Rocchio) — the Slice-20 fallback."""

    def __init__(self, seeds: dict[str, list[str]]):
        self.classes = list(seeds)
        texts = [t for c in self.classes for t in seeds[c]]
        labels = [c for c in self.classes for _ in seeds[c]]
        self.vocab, self.idf = _fit_tfidf(texts)
        # L2-normalized class centroids.
        self.centroids: dict[str, list[float]] = {}
        for c in self.classes:
            rows = [self._vec(t) for t, lab in zip(texts, labels) if lab == c]
            dim = len(self.vocab)
            mean = [sum(r[i] for r in rows) / len(rows) for i in range(dim)]
            nrm = math.sqrt(sum(v * v for v in mean))
            self.centroids[c] = [v / nrm for v in mean] if nrm else mean

    def _vec(self, text: str) -> list[float]:
        return _vectorize(text, self.vocab, self.idf)

    def predict(self, text: str) -> tuple[str, float, str]:
        """Return (intent, confidence, runner_up).

        confidence = top cosine similarity (in [0, 1]); a lower-bound proxy
        consistent with the lexical fallback's measured macro 0.768.
        """
        x = self._vec(text)
        sims = sorted(
            ((sum(x[i] * cen[i] for i in range(len(x))), c)
             for c, cen in self.centroids.items()),
            reverse=True,
        )
        top_sim, top_c = sims[0]
        runner_up = sims[1][1] if len(sims) > 1 else top_c
        return top_c, round(max(0.0, top_sim), 4), runner_up


# --------------------------------------------------------------------------- #
# Router
# --------------------------------------------------------------------------- #
class L2Router:
    """Loads the committed registry and recommends a plan per query."""

    def __init__(self, registry_path: Path = REGISTRY_PATH):
        self.registry = json.loads(Path(registry_path).read_text(encoding="utf-8"))
        self.intents = self.registry["intents"]
        self.classes = tuple(self.intents)
        self.guard = self.registry["forbidden_composition_guard"]["router_isolation_rule"]
        self._clf = _LexicalClassifier(_SEED_PHRASES)

    # -- forbidden-composition validator (EXP-B'.5 seam; R-L2 §3) ------------- #
    def check_forbidden(self, intent: str, stack: list[str]) -> None:
        """Raise ForbiddenCompositionError if ``stack`` uses an op forbidden for
        ``intent``. The 0.8.15 dispatcher inherits this validator seam."""
        rule = self.guard.get(intent)
        if rule is None:
            raise UnknownIntentError(f"no isolation rule for intent {intent!r}")
        forbidden = set(rule["forbidden_ops"])
        bad = [op for op in stack if op in forbidden]
        if bad:
            raise ForbiddenCompositionError(
                f"intent {intent!r} forbids {bad} (router-isolation: map_reduce_qfs/"
                f"community_summary are valid only for `global`); offending stack={stack}"
            )

    def _resolve_intent(self, query: str, agent_hint: Optional[str]):
        if agent_hint is not None:
            # Preference #1: agent owns intent. Verbatim, confidence 1.0, NO
            # fallback to the internal classifier (R-L2-3).
            if agent_hint not in self.intents:
                raise UnknownIntentError(
                    f"agent_hint {agent_hint!r} is not one of {self.classes}"
                )
            return agent_hint, 1.0, None, "agent_hint"
        # Preference #3: internal classifier fallback (lower-bound proxy).
        intent, conf, runner_up = self._clf.predict(query)
        return intent, conf, runner_up, "internal_classifier"

    def recommend(self, query: str, *, agent_hint: Optional[str] = None) -> Recommendation:
        """Recommend a retrieval stack for ``query`` WITHOUT executing it."""
        intent, confidence, runner_up, source = self._resolve_intent(query, agent_hint)
        rec = self.intents[intent]
        stack = list(rec["stack"])
        # Enforce the EXP-B'.5 router-isolation seam on the emitted stack.
        self.check_forbidden(intent, stack)

        if source == "agent_hint":
            why = (
                f"intent={intent!r} from agent_hint (verbatim, no classifier "
                f"fallback). "
            )
        else:
            why = (
                f"intent={intent!r} from internal lexical classifier "
                f"(confidence={confidence}, runner-up={runner_up!r}; "
                f"lower-bound fallback proxy — prefer agent_hint). "
            )
        prov = rec["provenance"]
        cfg = rec["config"]
        rationale = (
            why
            + f"stack={stack} (cost_tier={rec['cost_tier']}, {prov}). "
            + f"config: alpha={cfg['alpha']}, pool_n={cfg['pool_n']}, "
            + f"candidate_k={cfg['retrieval']['candidate_k']}, "
            + f"final_K={cfg['retrieval']['final_K']}. "
            + f"source_exp={rec['source_exp']}. "
            + f"forbidden_ops={cfg['forbidden_ops']}. "
            + "feedback_arm=False (EXP-AF KILL: agent signal does not beat ce_score "
            + "net of round-trip). SCREENING DATA — 0.8.15 must re-validate"
            + (" (PROVISIONAL pin)." if rec["provisional"] else ".")
        )
        return Recommendation(
            intent=intent,
            stack=stack,
            config=cfg,
            confidence=confidence,
            cost_tier=rec["cost_tier"],
            rationale=rationale,
            feedback_arm=False,  # EXP-AF KILL — never True in this prototype.
        )


__all__ = [
    "L2Router",
    "Recommendation",
    "ForbiddenCompositionError",
    "UnknownIntentError",
]
