"""AC-SBOM-05 .. AC-SBOM-09 and AC-SBOM-23 — tiering, fixture exclusion, the
loud gap, and the longest-prefix matching rule.

REQ-3 (tiering), REQ-4 (loud gaps), REQ-5 (fixture exclusion).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.2 and §5.3.

`AC-SBOM-23` carries a criterion id out of file order: it was added at fix-3
(codex §9 round 3) and numbered last so that AC-SBOM-10..22 keep the ids the
design, the README and the closure JSON already cite. Its subject matter is
§5.3, which is why the test lives here beside AC-SBOM-05..09.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from conftest import (
    FIXTURE_PREFIX,
    PROJECT_ROOT,
    REPO_ROOT,
    TIER_VOCABULARY,
    require,
    tracked_manifest_paths,
)


def _fixture_manifests() -> list[str]:
    return [p for p in tracked_manifest_paths() if p.startswith(FIXTURE_PREFIX)]


# --- AC-SBOM-23 overlapping-rule fixture data (design §5.3) -----------------
#
# `_SPECIFIC_PREFIX` is a PROPER PREFIX EXTENSION of `_BROAD_PREFIX`, and the
# two map to DIFFERENT tiers, so first-match-wins and longest-prefix-wins give
# DIFFERENT answers for `_OVERLAP_PATH`. That divergence is the whole point:
# with two rules that agree, or that cannot both match, every matching strategy
# looks identical and the criterion would be vacuous.
#
# Neither path needs to exist on disk. `TierMap.classify()` is a pure function
# of the rule set and the path string, and deliberately so — a rule for a
# subtree that does not exist yet must still be expressible.
_BROAD_PREFIX = "dev/tools/"
_SPECIFIC_PREFIX = "dev/tools/vendored-shipped/"
_OVERLAP_PATH = _SPECIFIC_PREFIX + "Cargo.toml"
_BROAD_ONLY_PATH = _BROAD_PREFIX + "mermaid/package.json"

# A tier file with the SAME prefix twice — a load-time error per §5.3. Written
# to pytest's `tmp_path` scratch dir at test time; Slice 31 creates no tracked
# `tiers.toml` (that is a Slice 32 artifact, §6).
_DUPLICATE_PREFIX_TOML = f"""\
schema = 1

[[rule]]
prefix = "{_BROAD_PREFIX}"
action = "tier"
tier   = "dev-tooling"

[[rule]]
prefix = "{_BROAD_PREFIX}"
action = "tier"
tier   = "shipped"
"""


def test_release_fixtures_are_excluded_and_auditable() -> None:
    """AC-SBOM-05.

    `dev/release/fixtures/cargo-skew/**` and `dev/release/fixtures/pip-skew/**`
    are deliberately fake, deliberately skewed manifests that exist to make the
    release version-skew gates demonstrate their catch. They must contribute
    ZERO components, and the exclusion must be RECORDED (auditable), not silent.
    """
    fixtures = _fixture_manifests()
    assert len(fixtures) == 8, (
        "precondition changed: expected the 8 tracked skew fixtures, found "
        f"{len(fixtures)}: {fixtures}"
    )

    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-05",
        "run_survey() must exclude every dev/release/fixtures/** manifest from"
        " `components` AND record each one in `survey.excluded` with"
        " reason='fixture' — excluded, but auditable, never silently dropped.",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-05",
        "an OfflineSource is needed to run the survey without network.",
    )

    survey = survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())

    excluded = {e.path: e.reason for e in survey.excluded}
    for path in fixtures:
        assert excluded.get(path) == "fixture", (
            f"{path} must appear in survey.excluded with reason='fixture';"
            f" got {excluded.get(path)!r}"
        )

    doc = survey.to_cyclonedx()
    origins = {
        prop.get("value")
        for component in doc.get("components", [])
        for prop in component.get("properties", [])
        if prop.get("name") == "fathomdb:declared-in"
    }
    leaked = sorted(o for o in origins if o and o.startswith(FIXTURE_PREFIX))
    assert not leaked, f"fixture manifests produced real components: {leaked}"


def test_fixture_exclusion_is_data_driven_not_hardcoded() -> None:
    """AC-SBOM-06.

    The exclusion must be a rule in the tracked `tiers.toml`, not an `if` in
    code. Proof: load a tier map with the fixture rule REMOVED and the fixture
    manifests must then flow through to tiering (and, having no rule, trip the
    REQ-4 loud failure). A hardcoded special case cannot satisfy this.

    Proving it at `TierMap.classify()` alone is NOT enough, and that was codex
    §9 round 2's second finding: Slice 32 could make classify() perfectly
    data-driven and still write `if path.startswith("dev/release/fixtures/")`
    into `run_survey()`, passing both AC-SBOM-05 and the classify-level
    assertions while REQ-5's "never by a hardcoded special case in code" went
    untested where it counts. Both tier maps are therefore driven THROUGH the
    survey boundary as well.
    """
    tiers_file = PROJECT_ROOT / "tiers.toml"
    tiers = require(
        "sbom_survey.tiers",
        "AC-SBOM-06",
        "the fixture exclusion must live as an `action = \"exclude\"` rule in the"
        " tracked scripts/sbom-survey/tiers.toml; removing that rule from a"
        " loaded TierMap must change the behaviour, proving the rule is data,"
        " not a hardcoded path check in code.",
    )

    assert tiers_file.is_file(), (
        f"{tiers_file} does not exist — the tier/exclusion rules must be tracked"
        " DATA (Slice 32 artifact), not code"
    )
    text = tiers_file.read_text(encoding="utf-8")
    assert FIXTURE_PREFIX in text, (
        f"{tiers_file} carries no rule for {FIXTURE_PREFIX!r}; the exclusion is"
        " not data-driven"
    )

    full = tiers.load_tier_map(tiers_file)
    assert full.classify(FIXTURE_PREFIX + "cargo-skew/Cargo.toml").action == "exclude"

    stripped = tiers.TierMap(
        [r for r in full.rules if not r.prefix.startswith(FIXTURE_PREFIX)]
    )
    with pytest.raises(tiers.UntieredManifestError):
        stripped.classify(FIXTURE_PREFIX + "cargo-skew/Cargo.toml")

    # --- and now at the SURVEY BOUNDARY, which is where REQ-5 actually bites -
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-06",
        "run_survey(repo_root, *, published, tier_map=…) must take its exclusion"
        " decisions FROM the injected tier map: with the fixture rule present"
        " the fixture manifests are excluded; with that one rule removed they"
        " must reach tiering and raise UntieredManifestError naming a fixture"
        ' path. A hardcoded path.startswith("dev/release/fixtures/") inside'
        " run_survey() cannot satisfy both halves.",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-06",
        "an OfflineSource keeps the survey-boundary run hermetic.",
    )

    fixtures = _fixture_manifests()

    with_rule = survey_mod.run_survey(
        REPO_ROOT, published=registry.OfflineSource(), tier_map=full
    )
    still_excluded = {e.path for e in with_rule.excluded}
    missing = sorted(set(fixtures) - still_excluded)
    assert not missing, (
        "run_survey() did not honour the INJECTED tier map — these fixture"
        f" manifests were not excluded: {missing}"
    )

    with pytest.raises(tiers.UntieredManifestError) as excinfo:
        survey_mod.run_survey(
            REPO_ROOT, published=registry.OfflineSource(), tier_map=stripped
        )
    assert any(path in str(excinfo.value) for path in fixtures), (
        "run_survey() with the fixture rule REMOVED must fail loudly, naming a"
        " fixture manifest (REQ-4). If it excluded them anyway, the exclusion"
        " lives in CODE rather than in tiers.toml, which is exactly what REQ-5"
        f" forbids. Error was: {excinfo.value}"
    )


def test_untiered_manifest_raises_untiered_manifest_error() -> None:
    """AC-SBOM-07.

    A tracked manifest matched by NO rule is a hard error naming the path.
    There is deliberately no catch-all rule: a default would turn "somebody
    added a manifest and nobody classified it" — the event this tool exists to
    catch — into a silent mis-tag.
    """
    tiers = require(
        "sbom_survey.tiers",
        "AC-SBOM-07",
        "TierMap.classify() must raise UntieredManifestError NAMING THE PATH for"
        " a tracked manifest that matches no rule. It must never return a null"
        " tier, never fall back to a default tier, and never drop the manifest.",
    )

    tier_map = tiers.TierMap(
        [tiers.TierRule(prefix="src/rust/crates/", action="tier", tier="shipped")]
    )
    with pytest.raises(tiers.UntieredManifestError) as excinfo:
        tier_map.classify("some/brand/new/Cargo.toml")
    assert "some/brand/new/Cargo.toml" in str(excinfo.value), (
        "the error must name the offending path so the fix is obvious; got: "
        f"{excinfo.value}"
    )


def test_tier_map_covers_every_discovered_manifest() -> None:
    """AC-SBOM-08.

    The repository's own tiers.toml must classify every path discovery
    currently returns. This is the drift detector: a manifest added tomorrow
    goes red here until somebody tiers it.
    """
    tiers = require(
        "sbom_survey.tiers",
        "AC-SBOM-08",
        "scripts/sbom-survey/tiers.toml must classify EVERY tracked manifest"
        " this repository currently has — an unclassified one fails loudly.",
    )
    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-08",
        "discovery supplies the paths that the tier map must cover.",
    )

    tier_map = tiers.load_tier_map(PROJECT_ROOT / "tiers.toml")
    unclassified: list[str] = []
    for ref in discovery.discover_manifests(REPO_ROOT):
        try:
            tier_map.classify(ref.path)
        except tiers.UntieredManifestError:
            unclassified.append(ref.path)
    assert not unclassified, (
        "tracked manifests with no tier assignment: "
        f"{sorted(unclassified)} — add a rule to {PROJECT_ROOT / 'tiers.toml'}"
    )


def test_tier_vocabulary_is_exactly_the_three_ruled_values() -> None:
    """AC-SBOM-09.

    The tier vocabulary is HITL-ruled: shipped / dev-tooling / eval-only.
    `fixture` is an EXCLUSION REASON, not a fourth tier — putting fake packages
    in the component list under any tier would hand a vulnerability feed real-
    looking phantoms.
    """
    pkg = require(
        "sbom_survey",
        "AC-SBOM-09",
        "sbom_survey.TIER_VOCABULARY must be exactly"
        f" {TIER_VOCABULARY!r} — the three HITL-ruled values, with `fixture`"
        " modelled as an exclusion reason rather than a fourth tier.",
    )
    assert tuple(pkg.TIER_VOCABULARY) == TIER_VOCABULARY
    assert "fixture" not in pkg.TIER_VOCABULARY

    tiers = require("sbom_survey.tiers", "AC-SBOM-09", "tier rules must validate against the vocabulary.")
    tier_map = tiers.load_tier_map(Path(PROJECT_ROOT) / "tiers.toml")
    for rule in tier_map.rules:
        if rule.action == "tier":
            assert rule.tier in TIER_VOCABULARY, (
                f"rule {rule.prefix!r} uses tier {rule.tier!r}, outside the"
                f" ruled vocabulary {TIER_VOCABULARY!r}"
            )


def test_longest_prefix_wins_and_rule_order_is_irrelevant(tmp_path: Path) -> None:
    """AC-SBOM-23.

    §5.3 freezes matching as LONGEST-PREFIX-WINS, and promises in terms that
    reordering `tiers.toml` "can never change the answer" — a property that
    matters precisely because that file will be edited by whoever adds the next
    manifest, and a re-tiering caused by moving a block is silent.

    Nothing enforced it (codex §9 round 3). A first-match-wins implementation
    passes every other criterion whenever the tracked file happens to list the
    more specific rule first, and then a later reordering silently re-tiers
    manifests despite the design saying it cannot.

    Three obligations, all from the same paragraph:

    1. With two overlapping rules — one prefix a proper extension of the other,
       mapping to DIFFERENT tiers — the LONGEST match wins.
    2. The same rule set in BOTH orders classifies IDENTICALLY. First-match-wins
       necessarily fails one of the two orderings, which is what makes this
       falsifiable rather than decorative.
    3. Duplicate prefixes are a LOAD-TIME error naming the offending prefix.

    Obligation 1 is checked against a second path that matches ONLY the broad
    rule, so "always answer with the longest rule in the map" — which would pass
    a single-path test — is caught too.
    """
    tiers = require(
        "sbom_survey.tiers",
        "AC-SBOM-23",
        "TierMap.classify() must select the LONGEST matching prefix, not the"
        " first one in file order, so the answer is independent of rule order"
        " in tiers.toml (design §5.3); and load_tier_map() must REJECT a file"
        " carrying the same prefix twice, naming that prefix.",
    )

    broad = tiers.TierRule(prefix=_BROAD_PREFIX, action="tier", tier="dev-tooling")
    specific = tiers.TierRule(prefix=_SPECIFIC_PREFIX, action="tier", tier="shipped")

    verdicts: dict[str, tuple[tuple[str, str], tuple[str, str]]] = {}
    for label, rules in (
        ("specific-rule-first", [specific, broad]),
        ("broad-rule-first", [broad, specific]),
    ):
        tier_map = tiers.TierMap(list(rules))

        overlap = tier_map.classify(_OVERLAP_PATH)
        assert (overlap.action, overlap.tier) == ("tier", "shipped"), (
            f"[{label}] {_OVERLAP_PATH!r} matches BOTH {_BROAD_PREFIX!r}"
            f" (-> dev-tooling) and the longer {_SPECIFIC_PREFIX!r} (-> shipped)."
            " §5.3 makes matching LONGEST-PREFIX-WINS, so the answer must be"
            f" ('tier', 'shipped'); got {(overlap.action, overlap.tier)!r}. A"
            " first-match-wins implementation produces exactly this failure in"
            " one of the two rule orders."
        )

        broad_only = tier_map.classify(_BROAD_ONLY_PATH)
        assert (broad_only.action, broad_only.tier) == ("tier", "dev-tooling"), (
            f"[{label}] {_BROAD_ONLY_PATH!r} matches ONLY {_BROAD_PREFIX!r}, so"
            " it must tier dev-tooling; got"
            f" {(broad_only.action, broad_only.tier)!r}. 'Longest prefix' means"
            " the longest rule that ACTUALLY MATCHES — never simply the longest"
            " rule present in the map."
        )

        verdicts[label] = (
            (overlap.action, overlap.tier),
            (broad_only.action, broad_only.tier),
        )

    assert verdicts["specific-rule-first"] == verdicts["broad-rule-first"], (
        "ORDER-INDEPENDENCE FAILED: the same two rules classified differently"
        " when their order was swapped —"
        f" specific-first={verdicts['specific-rule-first']!r} vs"
        f" broad-first={verdicts['broad-rule-first']!r}. §5.3 states that"
        " reordering tiers.toml 'can never change the answer'; here it did, so"
        " a later edit that merely moves a block would silently re-tier"
        " manifests."
    )

    # --- obligation 3: duplicate prefixes are a LOAD-TIME error --------------
    duplicate_file = tmp_path / "duplicate-prefix-tiers.toml"
    duplicate_file.write_text(_DUPLICATE_PREFIX_TOML, encoding="utf-8")

    with pytest.raises(Exception) as excinfo:  # noqa: PT011 - no error type is frozen by §5.3
        tiers.load_tier_map(duplicate_file)

    assert not isinstance(excinfo.value, tiers.UntieredManifestError), (
        "a duplicated prefix must fail while LOADING the rules, before any path"
        " is classified, so UntieredManifestError ('no rule matched this path')"
        f" is the wrong signal; got {excinfo.value!r}"
    )
    assert _BROAD_PREFIX in str(excinfo.value), (
        "the load-time error must NAME the duplicated prefix so the fix is"
        f" obvious; expected {_BROAD_PREFIX!r} in the message, got:"
        f" {excinfo.value}"
    )
