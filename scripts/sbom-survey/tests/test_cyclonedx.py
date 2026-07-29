"""AC-SBOM-10 .. AC-SBOM-13 — the CycloneDX document.

REQ-6 (CycloneDX 1.6), REQ-7 (resolved versions), REQ-8 (direct vs transitive).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.5.
"""

from __future__ import annotations

import re
from urllib.parse import unquote

from conftest import (
    KNOWN_INVALID_CYCLONEDX_DOC,
    KNOWN_TRANSITIVE_ONLY_CARGO,
    PROJECT_ROOT,
    PURL_PREFIX_BY_ECOSYSTEM,
    PURL_PREFIXES,
    REPO_ROOT,
    TIER_VOCABULARY,
    independent_cyclonedx_validator,
    purl_type,
    require,
    require_external,
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

    Membership in the vocabulary is NOT on its own a grade of the value: an
    implementation that stamped `dev-tooling` on all 400 components would
    satisfy it while every tier in the BOM was wrong, and Slice 33 prioritises
    on exactly this field. So the tag is also required to AGREE with
    `TierMap.classify()` of the manifest that declares the component — which is
    the rule set `AC-SBOM-23` grades for longest-prefix correctness and
    `AC-SBOM-06` proves `run_survey()` consults. Only components with EXACTLY
    ONE declaring manifest have their tier VALUE checked: a package declared by
    two manifests of different tiers has no single ruled answer in §5.2/§5.3, so
    demanding one would invent a contract.

    A component with NO origin used to be skipped outright, which was codex §9
    round 6's finding. A `transitive` component legitimately has none — §5.5
    defines it as one whose name reaches no tracked dependency table — but a
    `direct` one CANNOT, by that same definition. Silently exempting it let
    UNTRACKED-MANIFEST LEAKAGE through both this oracle and the discovery
    boundary: `run_survey()` could read `python/pyproject.toml` (gitignored, the
    exact case `AC-SBOM-02` exists to prevent), have no tracked path to
    attribute its dependencies to, emit them originless, and still pass so long
    as one other component carried an origin. Zero-origin `direct` components
    are therefore a FAILURE; zero-origin `transitive` ones stay exempt.
    """
    survey = _offline_survey(
        "AC-SBOM-11",
        "every CycloneDX component must carry EXACTLY ONE `fathomdb:tier`"
        f" property whose value is one of {TIER_VOCABULARY!r}, and that value"
        " must EQUAL TierMap.classify(<its declaring manifest>).tier for every"
        " component declared by exactly one manifest. Every `direct` component"
        " must carry at least one `fathomdb:declared-in` origin — a direct"
        " package with none means an untracked manifest was read.",
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

    # --- the value itself, against the tracked rules ------------------------
    tiers_mod = require(
        "sbom_survey.tiers",
        "AC-SBOM-11",
        "the tracked tiers.toml is the source of record for a component's tier;"
        " the property must carry the tier that map assigns to the declaring"
        " manifest, not merely some value from the vocabulary.",
    )
    tier_map = tiers_mod.load_tier_map(PROJECT_ROOT / "tiers.toml")

    graded = 0
    for component in doc["components"]:
        props = component.get("properties", [])
        origins = [
            p["value"] for p in props if p.get("name") == "fathomdb:declared-in"
        ]
        ref = component.get("bom-ref", component.get("name"))

        if not origins:
            depth = next(
                (p["value"] for p in props if p.get("name") == "fathomdb:depth"),
                None,
            )
            assert depth != "direct", (
                f"{ref}: tagged fathomdb:depth='direct' but carries NO"
                " `fathomdb:declared-in` origin, so its tier went UNGRADED"
                " against the tracked tiers.toml. §5.5 makes a component"
                " `direct` IFF its name appears in a dependency table of a"
                " TRACKED, non-excluded manifest, so a direct component with no"
                " declaring manifest is a contradiction — and the way it arises"
                " is UNTRACKED-MANIFEST LEAKAGE: run_survey() read a file `git"
                " ls-files` does not report (python/pyproject.toml is the exact"
                " case AC-SBOM-02 exists to prevent), had no tracked path to"
                " attribute the dependency to, and emitted it originless."
                " Exempting these silently let that leak past this oracle AND"
                " the discovery-boundary checks, which only constrain origins"
                " that exist. Transitive components with no origin are"
                " legitimate (§5.5) and remain exempt."
            )
            continue

        if len(origins) != 1:
            continue
        tier = next(p["value"] for p in props if p.get("name") == "fathomdb:tier")
        verdict = tier_map.classify(origins[0])
        assert verdict.action == "tier", (
            f"{ref}: its only declaring manifest {origins[0]!r} is"
            f" {verdict.action!r} in tiers.toml, so it must contribute no"
            " component at all"
        )
        assert tier == verdict.tier, (
            f"{ref}: tagged {tier!r} but its declaring manifest {origins[0]!r}"
            f" tiers {verdict.tier!r} per the tracked tiers.toml — the tag does"
            " not come from the rules, so Slice 33 would prioritise on a"
            " fabricated value"
        )
        graded += 1

    assert graded, (
        "vacuous-pass guard: no component carried exactly one"
        " `fathomdb:declared-in` origin, so not one tier value was graded"
        " against the rules"
    )


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


# --- AC-SBOM-24 support (added under seq-168 / TC-112(a)) --------------------
#
# A SECOND, INDEPENDENT implementation of "does this range admit this version",
# built ONLY on the upstream ordering primitives (`semver.Version` for the
# cargo/npm arms, `packaging` for the pypi arm — both already declared in the
# mini-project's pyproject.toml, so no dependency is added for this criterion).
#
# It deliberately does NOT import `sbom_survey.constraints`. Grading the
# survey's constraint logic WITH the survey's own constraint logic is
# self-certification — an implementation whose matcher answered `True` to
# everything would attach every declaration to every locked version and then
# certify itself clean. That is the identical failure `AC-SBOM-10` was rewritten
# to remove at codex §9 round 2, and it is the reason this oracle is a separate
# implementation rather than a call into the package under test.

_AC24_PARTIAL = re.compile(
    r"^v?(\d+|[xX*])"
    r"(?:\.(\d+|[xX*]))?"
    r"(?:\.(\d+|[xX*]))?"
    r"(?:-([0-9A-Za-z.\-]+))?"
    r"(?:\+[0-9A-Za-z.\-]+)?$"
)
_AC24_COMPARATOR = re.compile(
    r"\s*(\^|~>|~|>=|<=|>|<|==|=)?\s*(v?[0-9xX*][0-9A-Za-z.\-+*xX]*)\s*"
)

# Constraint FORMS this oracle does not attempt to read: registry aliases, VCS
# and filesystem specifiers, and the placeholders a survey may legitimately emit
# for a dependency that carries no range at all. They are TOLERATED as
# unevaluable and excluded from the graded count.
#
# EVERYTHING ELSE THAT FAILS TO PARSE IS A FAILURE, NOT A SHRUG. Without that
# rule, "emit constraints this oracle cannot read" would be a hole straight
# through the criterion — a wrong implementation could evade it by degrading its
# own output, which is the vacuous-green class Slice 31 spent six rounds on.
_AC24_OPAQUE_WORDS = ("workspace", "path", "git")
_AC24_OPAQUE_CHARS = (":", "/", "\\")


def _ac24_is_opaque_form(constraint: str) -> bool:
    text = constraint.strip()
    return text in _AC24_OPAQUE_WORDS or any(c in text for c in _AC24_OPAQUE_CHARS)


def _ac24_satisfies(ecosystem, constraint, version, semver_mod, version_mod, specifiers_mod):
    """True / False, or None when THIS ORACLE cannot evaluate the form."""
    text = (constraint or "").strip()
    if text in ("", "*", "x", "X"):
        return True

    if ecosystem == "pypi":
        try:
            specifier = specifiers_mod.SpecifierSet(text)
            parsed = version_mod.Version(version)
        except Exception:  # noqa: BLE001 - an unreadable form is None, never True
            return None
        return specifier.contains(parsed, prereleases=True)

    if ecosystem not in ("cargo", "npm") or _ac24_is_opaque_form(text):
        return None
    try:
        actual = semver_mod.Version.parse(version)
    except Exception:  # noqa: BLE001
        return None

    def segment(raw):
        return None if raw is None or raw in ("x", "X", "*") else int(raw)

    def at(major, minor, patch, pre=None):
        return semver_mod.Version(major, minor or 0, patch or 0, prerelease=pre)

    satisfied_any = False
    for alternative in text.split("||"):
        cleaned = alternative.replace(",", " ").strip()
        if not cleaned:
            satisfied_any = True
            continue
        pairs, position = [], 0
        while position < len(cleaned):
            match = _AC24_COMPARATOR.match(cleaned, position)
            if match is None or match.end() == position:
                return None
            pairs.append((match.group(1), match.group(2)))
            position = match.end()
        if not pairs:
            return None

        holds = True
        for operator, raw in pairs:
            parsed = _AC24_PARTIAL.match(raw)
            if parsed is None:
                return None
            major = segment(parsed.group(1))
            if major is None:
                continue
            minor, patch = segment(parsed.group(2)), segment(parsed.group(3))
            pre = parsed.group(4)
            if operator is None:
                # cargo reads a bare requirement as CARET; npm reads a bare full
                # version as EXACT and a bare partial as a prefix range.
                operator = "^" if ecosystem == "cargo" else "="
            low = at(major, minor, patch, pre)
            if operator == "^":
                if major > 0 or minor is None:
                    high = at(major + 1, 0, 0)
                elif minor > 0 or patch is None:
                    high = at(0, minor + 1, 0)
                else:
                    high = at(0, 0, patch + 1)
                verdict = low <= actual < high
            elif operator in ("~", "~>"):
                high = at(major + 1, 0, 0) if minor is None else at(major, minor + 1, 0)
                verdict = low <= actual < high
            elif operator == ">=":
                verdict = actual >= low
            elif operator == ">":
                verdict = actual > low
            elif operator == "<=":
                verdict = actual <= low
            elif operator == "<":
                verdict = actual < low
            elif minor is None:
                verdict = low <= actual < at(major + 1, 0, 0)
            elif patch is None:
                verdict = low <= actual < at(major, minor + 1, 0)
            else:
                verdict = actual == low
            if not verdict:
                holds = False
                break
        satisfied_any = satisfied_any or holds
    return satisfied_any


def test_no_component_carries_a_constraint_its_version_violates() -> None:
    """AC-SBOM-24.

    NO COMPONENT MAY CARRY A `fathomdb:constraint` THAT ITS OWN VERSION DOES NOT
    SATISFY. (HITL ruling `seq-168`, TC-112(a); the one criterion the Slice-31
    suite was unfrozen for.)

    Why it exists. A manifest declares a dependency by NAME and RANGE; a
    lockfile resolves it to concrete versions. An implementation that attaches a
    declaration to every locked version of that name stamps ranges onto versions
    they cannot resolve to — on this repository it produced `sha2 0.10.9` under
    `constraint = "0.11"`, `thiserror 2.0.18` under `"1"` and `tokenizers
    0.22.2` under `"0.20"`, each of them tagged `direct`. `depth` and
    `edit_sites` are the two fields Slice 33 makes its surgical /
    not-surgical call on, and that survey feeds 0.8.22, so a permissive oracle
    here propagates into real dependency decisions.

    The suite could not see any of it: the defect was found by reading the code,
    and mutations restoring it left all 23 other criteria GREEN. This criterion
    closes that hole.

    Graded ON THE EMITTED DOCUMENT — `run_survey(...)` -> `to_cyclonedx()` — and
    not against any helper. That is the TC-105 class this slice has been
    fighting throughout, and re-introducing it from the test side would be no
    better than from the implementation side.

    THE RULE FOR CONSTRAINTS THIS ORACLE CANNOT READ, stated because a silent
    "unparseable therefore fine" would be the hole this criterion is meant to
    close:

    * a constraint whose FORM is deliberately opaque — a registry alias, a VCS
      or filesystem specifier, or a bare `workspace` / `path` / `git`
      placeholder — is TOLERATED and excluded from the graded count. Demanding a
      verdict on those would reject a correct implementation, which is the
      inverse defect codex §9 round 5 caught in `AC-SBOM-21`;
    * ANY OTHER unreadable constraint FAILS THE CRITERION, naming it. An
      implementation cannot escape this oracle by degrading its own output into
      something unparseable.

    Components carrying no version (`fathomdb:resolution = "unresolved"`) are
    outside the criterion by construction: they claim no version, so there is
    nothing for a constraint to contradict. That is the honest destination for a
    declaration whose range matches nothing, and it must stay available — the
    alternative is the fabrication this criterion forbids.
    """
    semver_mod = require_external(
        "semver",
        "AC-SBOM-24",
        "semver>=3.0,<4.0",
        "the cargo/npm arms of this criterion need semver ORDERING from the"
        " upstream library, so that the check is independent of the code under"
        " test rather than a call back into it.",
    )
    version_mod = require_external(
        "packaging.version",
        "AC-SBOM-24",
        "packaging>=24.0,<26.0",
        "the pypi arm needs PEP 440 version parsing from upstream `packaging`.",
    )
    specifiers_mod = require_external(
        "packaging.specifiers",
        "AC-SBOM-24",
        "packaging>=24.0,<26.0",
        "the pypi arm needs PEP 440 specifier evaluation from upstream"
        " `packaging`; PEP 440 ordering cannot be re-derived by hand.",
    )

    survey = _offline_survey(
        "AC-SBOM-24",
        "no component in the emitted document may carry a `fathomdb:constraint`"
        " that its own `version` does not satisfy. A declaration must be"
        " attached only to the locked versions its declared range can actually"
        " resolve to — never to every locked version sharing the name.",
    )
    doc = survey.to_cyclonedx()

    graded = 0
    tolerated = 0
    violations: list[str] = []
    unreadable: list[str] = []

    for component in doc["components"]:
        version = component.get("version")
        if not version:
            continue  # unresolved: claims no version, so nothing to contradict
        ecosystem = purl_type(component.get("purl"))
        ref = component.get("bom-ref", component.get("name"))
        for prop in component.get("properties", []):
            if prop.get("name") != "fathomdb:constraint":
                continue
            constraint = prop.get("value")
            verdict = _ac24_satisfies(
                ecosystem, constraint, version, semver_mod, version_mod, specifiers_mod
            )
            if verdict is None:
                if _ac24_is_opaque_form(str(constraint)):
                    tolerated += 1
                else:
                    unreadable.append(f"{ref}: constraint {constraint!r}")
                continue
            graded += 1
            if not verdict:
                violations.append(
                    f"{ref}: version {version!r} does NOT satisfy its own"
                    f" declared constraint {constraint!r}"
                )

    assert not unreadable, (
        "these components carry a `fathomdb:constraint` this criterion could not"
        " read, and whose form is not one of the tolerated opaque specifiers"
        f" ({list(_AC24_OPAQUE_WORDS)}, or anything containing"
        f" {list(_AC24_OPAQUE_CHARS)}):\n  " + "\n  ".join(sorted(unreadable)[:10])
        + "\nAn unreadable constraint is NOT a pass. Emitting ranges this oracle"
        " cannot evaluate would let a wrong implementation escape the criterion"
        " by degrading its own output."
    )

    assert graded, (
        "VACUOUS-PASS GUARD: this criterion graded ZERO (component, constraint)"
        f" pairs — {tolerated} were tolerated as opaque and"
        f" {len(doc['components'])} components were examined. Either no"
        " component carries `fathomdb:constraint` any more, or every constraint"
        " became unevaluable. Both turn AC-SBOM-24 green by EMPTINESS while the"
        " property it exists to enforce goes unchecked, which is exactly the"
        " failure class this suite was hardened against."
    )

    assert not violations, (
        f"{len(violations)} of {graded} graded (component, constraint) pairs"
        " carry a constraint the component's own version does not satisfy:\n  "
        + "\n  ".join(sorted(violations)[:15])
        + "\nA declaration must be attached only to the locked versions its"
        " declared range can actually resolve to. Attaching it to every locked"
        " version sharing the name corrupts `depth` and `edit_sites` — the two"
        " fields Slice 33 makes its surgical/not-surgical call on — and that"
        " survey feeds 0.8.22's dependency decisions."
    )
