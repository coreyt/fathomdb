"""Shared primitives: component identity (§5.5) and the ONE timestamp door (§5.8)."""

from __future__ import annotations

from datetime import datetime, timezone

from packageurl import PackageURL

__all__ = [
    "TimestampFormatError",
    "make_purl",
    "normalize_timestamp",
    "parse_timestamp",
]


class TimestampFormatError(ValueError):
    """A timestamp input that is not ISO 8601. Never silently substituted."""

    #: What a well-formed value looks like. Parameterized because the two inputs
    #: that reach this error have DIFFERENT formats — `--now` is ISO 8601 and
    #: `SOURCE_DATE_EPOCH` is an integer second count — and an error message that
    #: names the wrong format is worse than terse.
    ISO_8601 = "an ISO 8601 timestamp, e.g. 1970-01-01T00:00:00Z or 2026-07-29T12:00:00+00:00"
    UNIX_SECONDS = "an integer number of seconds since the Unix epoch, e.g. 1700000000"

    def __init__(
        self,
        value: object,
        source: str,
        cause: BaseException | None = None,
        expected: str | None = None,
    ) -> None:
        self.value = value
        self.source = source
        detail = f" ({type(cause).__name__}: {cause})" if cause is not None else ""
        super().__init__(
            f"{source}: {value!r} is not a usable timestamp{detail}.\n"
            f"  Expected {expected or self.ISO_8601}.\n"
            "  It is REJECTED rather than substituted: this value is the"
            " provenance of the whole run, and design §5.6 makes the findings"
            " doc's provenance header load-bearing — 'without this the numbers"
            " are unfalsifiable'. Quietly falling back to a default would make a"
            " typo produce a plausible-looking artifact stamped with a time the"
            " run did not happen at."
        )


def parse_timestamp(value: str, *, source: str) -> datetime:
    """THE ONLY PLACE A TIMESTAMP STRING IS INTERPRETED.

    Every artifact's stamp flows through here, so `staleness.json`'s `generated`
    and the CycloneDX `metadata.timestamp` cannot disagree about when a run
    happened. They used to: a malformed `--now` was passed through verbatim into
    `survey.timestamp` (and so into `staleness.json`), while the CycloneDX
    builder failed to parse the same string and SILENTLY fell back to the fixed
    1980 epoch — one run, two artifacts, two different answers (codex §9 round 3
    `[P3]`).

    There is deliberately NO fallback here. A caller that cannot supply a
    parseable timestamp gets an error naming the value and the input it came
    from; it does not get a substitute.
    """
    if not isinstance(value, str):
        raise TimestampFormatError(value, source)
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise TimestampFormatError(value, source, exc) from exc
    if parsed.tzinfo is None:
        # Naive input is read as UTC rather than as local time: local time would
        # make the artifacts depend on the operating machine, which REQ-13
        # forbids.
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed


def normalize_timestamp(value: str, *, source: str) -> str:
    """`parse_timestamp` rendered back to a canonical ISO 8601 string."""
    return parse_timestamp(value, source=source).isoformat()

_PURL_TYPE = {"cargo": "cargo", "npm": "npm", "pypi": "pypi"}


def make_purl(ecosystem: str, name: str, version: str | None) -> str:
    """`pkg:<type>/<name>[@<version>]`, unique per (ecosystem, name, version).

    Built with `packageurl-python` rather than by string concatenation: purl has
    non-obvious encoding rules (namespaces, qualifiers, percent-encoding) — an
    npm scope becomes a purl NAMESPACE and its `@` is percent-encoded, so
    `@napi-rs/cli` is `pkg:npm/%40napi-rs/cli@2.18.4` and the version segment
    stays unambiguous.

    A component with no locked version carries a version-less purl. It is still
    a stable identity, and the component is separately marked
    `fathomdb:resolution = "unresolved"` so no consumer mistakes it for a
    resolved package at an unknown version.
    """
    purl_type = _PURL_TYPE.get(ecosystem)
    if purl_type is None:  # pragma: no cover - ecosystems are closed at discovery
        raise ValueError(f"unknown ecosystem {ecosystem!r}")

    namespace: str | None = None
    base = name
    if purl_type == "npm" and name.startswith("@") and "/" in name:
        namespace, _, base = name.partition("/")

    return PackageURL(
        type=purl_type,
        namespace=namespace,
        name=base,
        version=version or None,
    ).to_string()
