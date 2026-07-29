"""AC-SBOM-05 .. AC-SBOM-09 — tiering, fixture exclusion, and the loud gap.

REQ-3 (tiering), REQ-4 (loud gaps), REQ-5 (fixture exclusion).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.2 and §5.3.
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
