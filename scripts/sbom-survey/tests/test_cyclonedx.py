"""AC-SBOM-10 .. AC-SBOM-13 — the CycloneDX document.

REQ-6 (CycloneDX 1.6), REQ-7 (resolved versions), REQ-8 (direct vs transitive).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.5.
"""

from __future__ import annotations

from urllib.parse import unquote

from conftest import (
    KNOWN_INVALID_CYCLONEDX_DOC,
    KNOWN_TRANSITIVE_ONLY_CARGO,
    PURL_PREFIX_BY_ECOSYSTEM,
    PURL_PREFIXES,
    REPO_ROOT,
    TIER_VOCABULARY,
    independent_cyclonedx_validator,
    purl_type,
    require,
)


def _offline_survey(criterion: str, behaviour: str):
    survey_mod = require("sbom_survey.survey", criterion, behaviour)
    registry = require("sbom_survey.registry", criterion, behaviour)
    return survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())


def test_cyclonedx_document_is_schema_valid_and_purl_identified() -> None:
    """AC-SBOM-10.

    The emitted document validates against the bundled CycloneDX 1.6 JSON
    schema. Hand-rolled JSON that no consumer will validate is not an SBOM.

    The schema verdict comes from an INDEPENDENT validator — the upstream
    cyclonedx-python-lib one, not `sbom_survey.cyclonedx.validate()` — with a
    known-invalid negative control proving the oracle actually rejects
    something (codex §9 round 2). The tool's own validate() is then required to
    agree with it in both directions rather than to be believed.

    Schema validity alone is NOT enough, and asserting only it was the gap
    codex §9 round 1 caught: `name` + `version` satisfy the 1.6 schema, so a
    purl-less document would have passed. REQ-6 requires a `purl` PER
    COMPONENT, and §5.5 makes the purl the `bom-ref` — the component identity
    an advisory feed matches a locked version against. A component without one
    is unmatchable, which defeats the reason the SBOM is produced.
    """
    survey = _offline_survey(
        "AC-SBOM-10",
        "Survey.to_cyclonedx() must produce a document that validates against"
        " the bundled CycloneDX 1.6 JSON schema"
        " (cyclonedx-python-lib[json-validation]), AND every component must"
        " carry a `purl` that IS its `bom-ref`, bears the ecosystem prefix"
        f" ({', '.join(PURL_PREFIXES)}) and encodes the component's own"
        " locked version.",
    )
    doc = survey.to_cyclonedx()

    assert doc.get("bomFormat") == "CycloneDX"
    assert doc.get("specVersion") == "1.6"
    assert str(doc.get("serialNumber", "")).startswith("urn:uuid:")
    assert doc.get("components"), "the BOM has no components"

    seen_types: set[str] = set()
    for component in doc["components"]:
        name = component.get("name")
        purl = component.get("purl")
        assert purl, (
            f"{name!r}: component has NO purl — REQ-6 requires one per component,"
            " and without it the component cannot be matched against an advisory"
        )
        assert purl.startswith(PURL_PREFIXES), (
            f"{name!r}: purl {purl!r} does not carry a recognized ecosystem"
            f" prefix (expected one of {PURL_PREFIXES})"
        )
        assert component.get("bom-ref") == purl, (
            f"{name!r}: bom-ref {component.get('bom-ref')!r} != purl {purl!r} —"
            " §5.5 makes the purl the bom-ref so refs are stable across runs"
        )
        seen_types.add(purl_type(purl) or "")

        props = {p["name"]: p["value"] for p in component.get("properties", [])}
        if props.get("fathomdb:resolution") != "unresolved":
            version = component.get("version")
            assert version, f"{purl}: resolved component has no version"
            assert "@" in purl, (
                f"{purl}: a resolved component's purl must pin its version"
            )
            assert unquote(purl.rsplit("@", 1)[-1]) == version, (
                f"{purl}: purl version does not match component version"
                f" {version!r} — the identity and the reported version disagree"
            )

    assert seen_types == set(PURL_PREFIX_BY_ECOSYSTEM), (
        "the BOM's purl types must be exactly"
        f" {sorted(PURL_PREFIX_BY_ECOSYSTEM)} — this repo tracks cargo, npm and"
        f" pypi manifests, so all three must be represented; got {sorted(seen_types)}"
    )

    # --- schema validity, graded by an INDEPENDENT oracle -------------------
    #
    # This half used to call `sbom_survey.cyclonedx.validate()` — a function
    # from the implementation under test — which is self-certification: a
    # `validate()` that returns None unconditionally passed while the tool
    # emitted invalid CycloneDX JSON, i.e. exactly the hand-rolled-SBOM failure
    # this criterion exists to stop (codex §9 round 2, fix-2 finding 1). The
    # oracle is now the upstream library's own 1.6 schema validator.
    schema_valid = independent_cyclonedx_validator("AC-SBOM-10")

    # Negative control FIRST — prove the oracle bites before trusting a clean
    # result from it. An "independent validator" that accepts anything is no
    # better than the self-certifying one it replaced.
    assert schema_valid(KNOWN_INVALID_CYCLONEDX_DOC) is not None, (
        "the independent CycloneDX 1.6 validator ACCEPTED a document whose only"
        " component carries no `type`, which the 1.6 schema requires. The"
        " oracle is not validating, so a clean verdict from it proves nothing."
    )

    problem = schema_valid(doc)
    assert problem is None, f"CycloneDX 1.6 schema validation failed: {problem}"

    # The tool's own validate() must AGREE with the independent oracle in BOTH
    # directions. The second assertion is the one that kills a stub: a
    # `validate()` that returns None unconditionally agrees on the valid
    # document and is caught here.
    validator_mod = require(
        "sbom_survey.cyclonedx",
        "AC-SBOM-10",
        "sbom_survey.cyclonedx.validate(doc) must really run the CycloneDX 1.6"
        " schema — returning None for a valid document AND a diagnostic for an"
        " invalid one. It is cross-checked against the independent"
        " cyclonedx-python-lib validator in both directions.",
    )
    assert validator_mod.validate(doc) is None, (
        "sbom_survey.cyclonedx.validate() rejected a document the independent"
        f" CycloneDX 1.6 validator accepts: {validator_mod.validate(doc)}"
    )
    assert validator_mod.validate(KNOWN_INVALID_CYCLONEDX_DOC) is not None, (
        "sbom_survey.cyclonedx.validate() returned None for a document the"
        " independent CycloneDX 1.6 validator REJECTS — it is not running the"
        " schema, so it certifies nothing and must never be the oracle for"
        " this criterion."
    )


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

    Closure ALONE is vacuous, and that was codex §9 round 1's third finding: an
    empty `dependencies` array is trivially closed, so an implementation that
    emitted the manifest-derived DIRECT set and skipped every lockfile-derived
    library<->library edge used to pass. The graph is therefore asserted
    non-empty, closed in BOTH directions (every declared component appears as a
    `ref`, leaves carrying an empty `dependsOn`), to contain at least one edge
    whose source is a library rather than the root component, and to have
    carried at least one known lockfile-only crate through as `transitive`.
    """
    survey = _offline_survey(
        "AC-SBOM-12",
        "the CycloneDX `dependencies` array must be NON-EMPTY and CLOSED IN BOTH"
        " DIRECTIONS (every dependsOn ref resolves to a declared component"
        " bom-ref, and every declared component appears as a `ref` entry), must"
        " contain at least one library->library edge, and every component must"
        " carry `fathomdb:depth` in {direct, transitive} with at least one"
        " lockfile-only package tagged `transitive`.",
    )
    doc = survey.to_cyclonedx()

    component_refs = {c["bom-ref"] for c in doc["components"]}
    root_ref = doc["metadata"]["component"]["bom-ref"]
    declared = component_refs | {root_ref}

    entries = doc.get("dependencies")
    assert isinstance(entries, list) and entries, (
        "the `dependencies` array is empty — REQ-8 and §5.5 require the"
        " lockfile-derived library<->library edge set, and an empty graph is"
        " vacuously 'closed' while enumerating nothing"
    )

    dangling: list[str] = []
    for entry in entries:
        if entry["ref"] not in declared:
            dangling.append(entry["ref"])
        for target in entry.get("dependsOn", []):
            if target not in declared:
                dangling.append(target)
    assert not dangling, f"dangling dependency refs (graph not closed): {sorted(set(dangling))}"

    ungraphed = sorted(component_refs - {e["ref"] for e in entries})
    assert not ungraphed, (
        "these components have no `dependencies` entry, so the graph does not"
        f" cover the component list (leaves take an empty dependsOn): {ungraphed[:5]}"
        f" (+{max(0, len(ungraphed) - 5)} more)"
    )

    library_edges = [
        e for e in entries if e["ref"] != root_ref and e.get("dependsOn")
    ]
    assert library_edges, (
        "every edge in the graph originates at the root component — not one"
        " library->library edge was emitted, which is the core of REQ-8"
    )

    for component in doc["components"]:
        depths = [
            p["value"]
            for p in component.get("properties", [])
            if p.get("name") == "fathomdb:depth"
        ]
        ref = component["bom-ref"]
        assert len(depths) == 1, f"{ref}: expected one fathomdb:depth, got {depths}"
        assert depths[0] in ("direct", "transitive"), f"{ref}: bad depth {depths[0]!r}"

    def _depth(component: dict) -> str | None:
        for prop in component.get("properties", []):
            if prop.get("name") == "fathomdb:depth":
                return prop.get("value")
        return None

    assert any(_depth(c) == "direct" for c in doc["components"]), (
        "no component was tagged direct — the manifest-derived direct set is empty"
    )

    transitive_cargo = {
        c.get("name")
        for c in doc["components"]
        if _depth(c) == "transitive"
        and str(c.get("purl", "")).startswith(PURL_PREFIX_BY_ECOSYSTEM["cargo"])
    }
    lockfile_only_seen = sorted(set(KNOWN_TRANSITIVE_ONLY_CARGO) & transitive_cargo)
    assert lockfile_only_seen, (
        "not one of the known lockfile-only crates"
        f" {list(KNOWN_TRANSITIVE_ONLY_CARGO)} reached the BOM tagged"
        " `transitive` — these are declared by NO tracked Cargo.toml, so their"
        " absence means the Cargo.lock library<->library graph was never walked"
        f" (transitive cargo components seen: {len(transitive_cargo)})"
    )


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
