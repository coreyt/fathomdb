"""AC-SBOM-14 .. AC-SBOM-17 — the published-version seam.

REQ-9 (injectable registry), REQ-10 (honest degradation).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.4.

A false "up-to-date" is the single worst output this tool can produce: it would
let a live advisory be closed as `CLOSE-satisfied` in the LIBRARY-BUMP-STEWARD
§2 triage. AC-SBOM-15 and AC-SBOM-16 are the two named guards against it.
"""

from __future__ import annotations

import re
import socket
from collections import Counter

import pytest

from conftest import REPO_ROOT, require

# --- AC-SBOM-17 survey-boundary probe data ----------------------------------
#
# A locked version safe to build sentinels around: three numeric segments (so
# semver parses it as well as PEP 440) with a major of at least 1 (so
# `_LOWER_SENTINEL` is unambiguously below it under both orderings).
_PROBE_VERSION = re.compile(r"^(\d+)\.\d+\.\d+$")

# Chosen to be beyond anything this repository could plausibly lock, and to
# parse under BOTH comparators (§5.4: semver for cargo/npm, PEP 440 for pypi),
# so the expected verdict does not depend on which package the probe lands on.
_HIGHER_SENTINEL = "9999.0.0"
_LOWER_SENTINEL = "0.0.1"


def test_survey_performs_no_network_io_by_default() -> None:
    """AC-SBOM-14.

    `run_survey` takes `published` as a REQUIRED keyword-only argument, so no
    code path reaches the network implicitly. With `OfflineSource` a full
    survey must open zero sockets — proved by making `socket.socket` explode.
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-14",
        "run_survey(root, *, published=...) must take the published-version"
        " source as a REQUIRED keyword-only argument and must perform ZERO"
        " socket I/O when given an OfflineSource.",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-14",
        "registry.OfflineSource must be a PublishedVersionSource that never"
        " touches the network.",
    )

    with pytest.raises(TypeError):
        survey_mod.run_survey(REPO_ROOT)  # `published` must not have a default

    real_socket = socket.socket
    opened: list[object] = []

    def exploding_socket(*args, **kwargs):
        opened.append(args)
        raise AssertionError("the offline survey opened a socket")

    socket.socket = exploding_socket  # type: ignore[assignment]
    try:
        survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())
    finally:
        socket.socket = real_socket  # type: ignore[assignment]

    assert not opened, "the offline survey opened a socket"


def test_unknown_latest_is_never_reported_current() -> None:
    """AC-SBOM-15.

    With no registry available, every row is `unknown`. NOT ONE row may be
    `current`: "we could not check" must never be rendered as "up to date".
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-15",
        "when the published-version source returns None for every lookup, every"
        " staleness row must have status == 'unknown' and latest_version is"
        " None. NO row may be 'current' — an unknown latest must never be"
        " rendered as up-to-date.",
    )
    registry = require("sbom_survey.registry", "AC-SBOM-15", "OfflineSource returns None for every lookup.")

    survey = survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())
    rows = survey.staleness()
    assert rows, "vacuous-pass guard: the survey produced zero staleness rows"

    falsely_current = [r.name for r in rows if r.status == "current"]
    assert not falsely_current, (
        "offline run reported packages as 'current' with no registry data: "
        f"{sorted(falsely_current)}"
    )
    for row in rows:
        assert row.status == "unknown", f"{row.name}: expected 'unknown', got {row.status!r}"
        assert row.latest_version is None, f"{row.name}: latest_version leaked {row.latest_version!r}"


def test_registry_error_degrades_to_unknown() -> None:
    """AC-SBOM-16.

    A source that RAISES (DNS failure, 503, timeout, malformed JSON) degrades
    to `unknown` with the reason recorded. It must not crash the run and must
    not fall back to `current`.
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-16",
        "a PublishedVersionSource that raises must degrade the affected rows to"
        " status='unknown' with `lookup_error` recorded — the run must not"
        " crash, and must not fall back to 'current'.",
    )
    require("sbom_survey.registry", "AC-SBOM-16", "the source protocol lives here.")

    class ExplodingSource:
        def latest(self, ecosystem: str, name: str) -> str | None:
            raise ConnectionError(f"registry unreachable for {ecosystem}/{name}")

    survey = survey_mod.run_survey(REPO_ROOT, published=ExplodingSource())
    rows = survey.staleness()
    assert rows, "vacuous-pass guard: the survey produced zero staleness rows"

    for row in rows:
        assert row.status == "unknown", f"{row.name}: expected 'unknown', got {row.status!r}"
        assert row.lookup_error, f"{row.name}: the failure reason was not recorded"


def test_static_source_classifies_current_outdated_ahead() -> None:
    """AC-SBOM-17.

    The positive path, with an injected source and no network: locked < latest
    is `outdated`, locked == latest is `current`, locked > latest is `ahead`.
    Both comparators are exercised (semver for cargo/npm, PEP 440 for pypi).

    TWO LEGS, and the second is the one that matters. Grading `classify_status()`
    alone left the **used-vs-published diff — the output this tool exists to
    produce — untested end to end** (codex §9 round 4): Slice 32 could ship a
    perfect comparator while `run_survey(…, published=…)` never called
    `published.latest()` at all, leaving every row `unknown`, and AC-SBOM-15/16
    would still pass because both of those inject a source that legitimately
    yields `unknown` for everything. Leg 2 therefore drives a POSITIVE
    `StaticSource` through `run_survey` and requires `current`, `outdated` and
    `ahead` to actually appear on the resulting staleness rows.

    Leg 1 survives because the comparator matrix — prereleases, PEP 440
    `.postN`, unparseable input, either side missing — cannot be provoked from
    whatever this repository happens to have locked.
    """
    staleness_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-17",
        "classify_status(ecosystem, locked, latest) must return 'outdated' when"
        " locked < latest, 'current' when equal, 'ahead' when locked > latest,"
        " and 'unknown' when either side is None or unparseable — using semver"
        " ordering for cargo/npm and PEP 440 ordering for pypi; AND"
        " run_survey(…, published=StaticSource(…)) must feed those verdicts"
        " onto the staleness rows.",
    )

    cases = [
        ("cargo", "1.0.0", "1.2.0", "outdated"),
        ("cargo", "1.2.0", "1.2.0", "current"),
        ("cargo", "1.3.0", "1.2.0", "ahead"),
        ("cargo", "1.2.3-rc.1", "1.2.3", "outdated"),
        ("npm", "0.22.1", "0.23.0", "outdated"),
        ("pypi", "1.6.1", "1.6.1", "current"),
        ("pypi", "1.6.1", "1.7.0", "outdated"),
        ("pypi", "2.0.0.post1", "2.0.0", "ahead"),
        ("cargo", None, "1.2.0", "unknown"),
        ("cargo", "1.2.0", None, "unknown"),
        ("cargo", "not-a-version", "1.2.0", "unknown"),
    ]
    for ecosystem, locked, latest, expected in cases:
        got = staleness_mod.classify_status(ecosystem, locked, latest)
        assert got == expected, (
            f"{ecosystem} locked={locked!r} latest={latest!r}: expected"
            f" {expected!r}, got {got!r}"
        )

    # --- LEG 2: the same three verdicts, THROUGH `run_survey` ----------------
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-17",
        "registry.StaticSource({(ecosystem, name): version}) must return the"
        " mapped version and None for anything absent (§5.4), and run_survey"
        " must call it once per surveyed package.",
    )

    # The probe versions are taken from a real offline run rather than hardcoded,
    # so the leg cannot rot when a lockfile moves.
    baseline_rows = staleness_mod.run_survey(
        REPO_ROOT, published=registry.OfflineSource()
    ).staleness()
    assert baseline_rows, "vacuous-pass guard: the survey produced zero staleness rows"

    occurrences = Counter((r.ecosystem, r.name) for r in baseline_rows)
    probes = [
        r
        for r in baseline_rows
        if occurrences[(r.ecosystem, r.name)] == 1
        and isinstance(r.locked_version, str)
        and _PROBE_VERSION.match(r.locked_version)
        and int(_PROBE_VERSION.match(r.locked_version).group(1)) >= 1
    ]
    assert len(probes) >= 3, (
        "vacuous-pass guard: fewer than three staleness rows carry a unique"
        " (ecosystem, name) and a simple `major.minor.patch` locked version with"
        f" major >= 1, so the three verdicts cannot be provoked; found"
        f" {len(probes)}"
    )

    current_row, outdated_row, ahead_row = probes[0], probes[1], probes[2]
    published = {
        (current_row.ecosystem, current_row.name): current_row.locked_version,
        (outdated_row.ecosystem, outdated_row.name): _HIGHER_SENTINEL,
        (ahead_row.ecosystem, ahead_row.name): _LOWER_SENTINEL,
    }
    expected_rows = {
        (current_row.ecosystem, current_row.name): (
            "current",
            current_row.locked_version,
        ),
        (outdated_row.ecosystem, outdated_row.name): ("outdated", _HIGHER_SENTINEL),
        (ahead_row.ecosystem, ahead_row.name): ("ahead", _LOWER_SENTINEL),
    }

    survey = staleness_mod.run_survey(
        REPO_ROOT, published=registry.StaticSource(published)
    )
    rows = survey.staleness()
    by_key = {(r.ecosystem, r.name): r for r in rows}

    for key, (expected_status, expected_latest) in expected_rows.items():
        row = by_key.get(key)
        assert row is not None, f"{key} vanished from the staleness rows"
        assert (row.status, row.latest_version) == (expected_status, expected_latest), (
            f"{key[0]}/{key[1]} locked={row.locked_version!r}, injected latest="
            f"{expected_latest!r}: expected status {expected_status!r} with"
            f" latest_version {expected_latest!r}, got {row.status!r} /"
            f" {row.latest_version!r}. run_survey() must ASK the injected"
            " published-version source and classify from its answer — the"
            " used-vs-published diff is the output this tool exists to produce,"
            " and a comparator that run_survey never calls produces none of it."
        )

    observed = {by_key[key].status for key in expected_rows}
    assert observed == {"current", "outdated", "ahead"}, (
        "ONE run with an injected StaticSource must exhibit ALL THREE positive"
        f" verdicts on the staleness rows; the survey produced {sorted(observed)}"
    )

    # A package the StaticSource knows NOTHING about must still degrade to
    # `unknown` — which is what stops a blanket fill from passing the rows above.
    unmapped = [r for r in rows if (r.ecosystem, r.name) not in published]
    assert unmapped, "vacuous-pass guard: every row was in the injected mapping"
    for row in unmapped:
        assert row.status == "unknown", (
            f"{row.ecosystem}/{row.name}: the StaticSource returns None for this"
            f" package, so the row must be 'unknown'; got {row.status!r} with"
            f" latest_version {row.latest_version!r}"
        )
        assert row.latest_version is None, (
            f"{row.ecosystem}/{row.name}: latest_version {row.latest_version!r}"
            " was invented for a package the source does not know"
        )
