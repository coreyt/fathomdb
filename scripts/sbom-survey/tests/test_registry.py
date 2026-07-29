"""AC-SBOM-14 .. AC-SBOM-17 — the published-version seam.

REQ-9 (injectable registry), REQ-10 (honest degradation).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.4.

A false "up-to-date" is the single worst output this tool can produce: it would
let a live advisory be closed as `CLOSE-satisfied` in the LIBRARY-BUMP-STEWARD
§2 triage. AC-SBOM-15 and AC-SBOM-16 are the two named guards against it.
"""

from __future__ import annotations

import socket

import pytest

from conftest import REPO_ROOT, require


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
    """
    staleness_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-17",
        "classify_status(ecosystem, locked, latest) must return 'outdated' when"
        " locked < latest, 'current' when equal, 'ahead' when locked > latest,"
        " and 'unknown' when either side is None or unparseable — using semver"
        " ordering for cargo/npm and PEP 440 ordering for pypi.",
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
