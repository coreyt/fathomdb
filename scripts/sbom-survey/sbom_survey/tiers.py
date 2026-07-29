"""Tier assignment from the tracked `tiers.toml` data file (REQ-3, REQ-4, REQ-5).

Design §5.2 and §5.3. Three properties this module exists to guarantee:

1. **Rules are DATA.** There is no path literal anywhere in this package that
   special-cases a subtree; `dev/release/fixtures/` reaches the code only as a
   `prefix` read out of `tiers.toml` (REQ-5).
2. **Matching is longest-prefix-wins**, so rule ORDER in the file is
   irrelevant and moving a block can never silently re-tier a manifest (§5.3).
3. **There is deliberately NO catch-all rule.** A tracked manifest matched by no
   rule raises `UntieredManifestError` naming the path. A default would convert
   "somebody added a manifest and nobody classified it" — the exact event this
   tool exists to catch — into a silent mis-tag (REQ-4).
"""

from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

from . import TIER_VOCABULARY

__all__ = [
    "DuplicateTierPrefixError",
    "TierMap",
    "TierRule",
    "TierRuleFileError",
    "TierVerdict",
    "UntieredManifestError",
    "load_tier_map",
]

_ACTIONS = ("tier", "exclude")


class UntieredManifestError(Exception):
    """A tracked manifest matched no rule. Always names the offending path."""

    def __init__(self, path: str) -> None:
        self.path = path
        super().__init__(
            f"untiered manifest: {path!r} matches no rule in the tier map."
            " Every tracked manifest must carry exactly one tier from"
            f" {list(TIER_VOCABULARY)} (or an explicit exclusion rule) — add a"
            " rule to scripts/sbom-survey/tiers.toml. There is deliberately no"
            " catch-all: a default would silently mis-tag a manifest nobody"
            " classified."
        )


class TierRuleFileError(ValueError):
    """A `tiers.toml` that cannot be trusted — rejected at LOAD time."""


class DuplicateTierPrefixError(TierRuleFileError):
    """The same `prefix` appears twice in a rule file (§5.3).

    Deliberately NOT an `UntieredManifestError`: that error means "no rule
    matched this path", and no path has been classified yet when a rule file is
    rejected. Resolving a duplicate silently (first-wins or last-wins) is the
    same ambiguity longest-prefix matching exists to remove, by another route.
    """

    def __init__(self, prefix: str, source: str) -> None:
        self.prefix = prefix
        super().__init__(
            f"duplicate tier rule prefix {prefix!r} in {source}: the same prefix"
            " appears more than once, so which rule wins would depend on file"
            " order. Remove or merge one of them."
        )


@dataclass(frozen=True)
class TierRule:
    """One rule from `tiers.toml`.

    `action = "tier"` carries a `tier`; `action = "exclude"` carries a `reason`.
    """

    prefix: str
    action: str
    tier: str | None = None
    reason: str | None = None
    note: str | None = None


@dataclass(frozen=True)
class TierVerdict:
    """`TierMap.classify()`'s answer for one path."""

    path: str
    action: str
    tier: str | None
    reason: str | None
    rule: TierRule


class TierMap:
    """An ordered-irrelevant, longest-prefix-wins set of tier rules."""

    def __init__(self, rules: Iterable[TierRule]) -> None:
        self.rules: list[TierRule] = list(rules)

    def __repr__(self) -> str:  # pragma: no cover - diagnostic only
        return f"TierMap({len(self.rules)} rules)"

    def classify(self, path: str) -> TierVerdict:
        """The verdict for `path`, or `UntieredManifestError` naming it.

        LONGEST-PREFIX-WINS: among every rule whose prefix the path starts
        with, the longest prefix is selected. Two distinct prefixes of equal
        length cannot both prefix the same string, so the winner is unique and
        the answer does not depend on the order of `self.rules` — which is what
        makes reordering `tiers.toml` provably safe (§5.3, AC-SBOM-23).
        """
        matches = [rule for rule in self.rules if path.startswith(rule.prefix)]
        if not matches:
            raise UntieredManifestError(path)
        best = max(matches, key=lambda rule: len(rule.prefix))
        return TierVerdict(
            path=path,
            action=best.action,
            tier=best.tier,
            reason=best.reason,
            rule=best,
        )


def load_tier_map(path: Path | str) -> TierMap:
    """Load and VALIDATE a `tiers.toml`.

    Everything that could make a rule set ambiguous is rejected here, at load
    time, before any path is classified: an unknown `action`, a tier outside the
    ruled vocabulary, an exclusion with no reason, and — §5.3 — a duplicated
    prefix, whose error message names the offending prefix.
    """
    source = str(path)
    with open(path, "rb") as handle:
        data = tomllib.load(handle)

    schema = data.get("schema")
    if schema != 1:
        raise TierRuleFileError(
            f"{source}: unsupported tier-rule schema {schema!r}; expected 1"
        )

    raw_rules = data.get("rule")
    if not isinstance(raw_rules, list) or not raw_rules:
        raise TierRuleFileError(f"{source}: no [[rule]] entries")

    rules: list[TierRule] = []
    seen: set[str] = set()
    for entry in raw_rules:
        prefix = entry.get("prefix")
        if not isinstance(prefix, str) or not prefix:
            raise TierRuleFileError(f"{source}: a rule has no `prefix`")
        if prefix in seen:
            raise DuplicateTierPrefixError(prefix, source)
        seen.add(prefix)

        action = entry.get("action")
        if action not in _ACTIONS:
            raise TierRuleFileError(
                f"{source}: rule {prefix!r} has action {action!r};"
                f" expected one of {list(_ACTIONS)}"
            )

        tier = entry.get("tier")
        reason = entry.get("reason")
        if action == "tier":
            if tier not in TIER_VOCABULARY:
                raise TierRuleFileError(
                    f"{source}: rule {prefix!r} assigns tier {tier!r}, outside"
                    f" the ruled vocabulary {list(TIER_VOCABULARY)}"
                )
        else:
            if not isinstance(reason, str) or not reason:
                raise TierRuleFileError(
                    f"{source}: exclusion rule {prefix!r} carries no `reason`;"
                    " an exclusion must be auditable, never silent"
                )
            if tier is not None:
                raise TierRuleFileError(
                    f"{source}: exclusion rule {prefix!r} also assigns a tier"
                )

        rules.append(
            TierRule(
                prefix=prefix,
                action=action,
                tier=tier,
                reason=reason,
                note=entry.get("note"),
            )
        )

    return TierMap(rules)
