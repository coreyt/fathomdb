"""AC-SBOM-10 .. AC-SBOM-13 — the CycloneDX document.

REQ-6 (CycloneDX 1.6), REQ-7 (resolved versions), REQ-8 (direct vs transitive).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.5.
"""

from __future__ import annotations

from conftest import REPO_ROOT, TIER_VOCABULARY, require


def _offline_survey(criterion: str, behaviour: str):
    survey_mod = require("sbom_survey.survey", criterion, behaviour)
    registry = require("sbom_survey.registry", criterion, behaviour)
    return survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())


def test_cyclonedx_document_is_schema_valid() -> None:
    """AC-SBOM-10.

    The emitted document validates against the bundled CycloneDX 1.6 JSON
    schema. Hand-rolled JSON that no consumer will validate is not an SBOM.
    """
    survey = _offline_survey(
        "AC-SBOM-10",
        "Survey.to_cyclonedx() must produce a document that validates against"
        " the bundled CycloneDX 1.6 JSON schema"
        " (cyclonedx-python-lib[json-validation]).",
    )
    doc = survey.to_cyclonedx()

    assert doc.get("bomFormat") == "CycloneDX"
    assert doc.get("specVersion") == "1.6"
    assert str(doc.get("serialNumber", "")).startswith("urn:uuid:")
    assert doc.get("components"), "the BOM has no components"

    validator_mod = require(
        "sbom_survey.cyclonedx",
        "AC-SBOM-10",
        "sbom_survey.cyclonedx.validate(doc) must run the CycloneDX 1.6 schema"
        " validator and return None on success / a diagnostic on failure.",
    )
    problem = validator_mod.validate(doc)
    assert problem is None, f"CycloneDX 1.6 schema validation failed: {problem}"


def test_every_component_carries_a_tier_property() -> None:
    """AC-SBOM-11.

    Tier tagging is what made TC-93 a cheap call. Every component carries
    exactly one `fathomdb:tier` property, and its value is in the ruled
    vocabulary.
    """
    survey = _offline_survey(
        "AC-SBOM-11",
        "every CycloneDX component must carry EXACTLY ONE `fathomdb:tier`"
        f" property whose value is one of {TIER_VOCABULARY!r}.",
    )
    doc = survey.to_cyclonedx()

    for component in doc["components"]:
        tiers = [
            p["value"]
            for p in component.get("properties", [])
            if p.get("name") == "fathomdb:tier"
        ]
        ref = component.get("bom-ref", component.get("name"))
        assert len(tiers) == 1, f"{ref}: expected one fathomdb:tier, got {tiers}"
        assert tiers[0] in TIER_VOCABULARY, f"{ref}: bad tier {tiers[0]!r}"


def test_dependency_graph_is_closed_and_depth_tagged() -> None:
    """AC-SBOM-12.

    library<->library enumeration. The `dependencies` array is the edge set;
    every `dependsOn` ref must resolve to a declared component (no dangling
    refs), and every component is tagged direct or transitive — LBS §2's
    triage turns on that distinction.
    """
    survey = _offline_survey(
        "AC-SBOM-12",
        "the CycloneDX `dependencies` array must be CLOSED (every dependsOn ref"
        " resolves to a declared component bom-ref) and every component must"
        " carry `fathomdb:depth` in {direct, transitive}.",
    )
    doc = survey.to_cyclonedx()

    declared = {c["bom-ref"] for c in doc["components"]}
    declared.add(doc["metadata"]["component"]["bom-ref"])

    dangling: list[str] = []
    for entry in doc.get("dependencies", []):
        if entry["ref"] not in declared:
            dangling.append(entry["ref"])
        for target in entry.get("dependsOn", []):
            if target not in declared:
                dangling.append(target)
    assert not dangling, f"dangling dependency refs (graph not closed): {sorted(set(dangling))}"

    for component in doc["components"]:
        depths = [
            p["value"]
            for p in component.get("properties", [])
            if p.get("name") == "fathomdb:depth"
        ]
        ref = component["bom-ref"]
        assert len(depths) == 1, f"{ref}: expected one fathomdb:depth, got {depths}"
        assert depths[0] in ("direct", "transitive"), f"{ref}: bad depth {depths[0]!r}"

    assert any(
        p.get("value") == "direct"
        for c in doc["components"]
        for p in c.get("properties", [])
        if p.get("name") == "fathomdb:depth"
    ), "no component was tagged direct — the manifest-derived direct set is empty"


def test_component_version_is_locked_and_constraint_preserved() -> None:
    """AC-SBOM-13.

    "What version are we using" is answered by the tracked LOCKFILE, not by a
    manifest range. The declared constraint survives alongside it so triage can
    see both.
    """
    survey = _offline_survey(
        "AC-SBOM-13",
        "a component's `version` must be the LOCKED/resolved version from the"
        " tracked lockfile (never a manifest range such as \"1.0\"), and the"
        " declared range must survive as a `fathomdb:constraint` property on"
        " every DIRECT component.",
    )
    doc = survey.to_cyclonedx()

    range_chars = set("^~*><= ,|")
    for component in doc["components"]:
        version = component.get("version")
        props = {p["name"]: p["value"] for p in component.get("properties", [])}
        ref = component["bom-ref"]
        if props.get("fathomdb:resolution") == "unresolved":
            assert version in (None, ""), (
                f"{ref}: an unresolved component must carry no version, got {version!r}"
            )
            continue
        assert version, f"{ref}: resolved component has no version"
        assert not (set(version) & range_chars), (
            f"{ref}: version {version!r} looks like a manifest RANGE, not a"
            " locked version"
        )
        if props.get("fathomdb:depth") == "direct":
            assert "fathomdb:constraint" in props, (
                f"{ref}: direct component lost its declared constraint"
            )
