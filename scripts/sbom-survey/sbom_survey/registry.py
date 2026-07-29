"""The published-version seam — injectable, and honest when offline (REQ-9, REQ-10).

Design §5.4. `run_survey` takes `published` as a REQUIRED keyword-only argument
with no default, so there is no code path that reaches the network implicitly:
an online run is an explicit act of construction by the caller.
"""

from __future__ import annotations

import json
import urllib.parse
import urllib.request
from typing import Mapping, Protocol, runtime_checkable

__all__ = [
    "HttpRegistrySource",
    "OfflineSource",
    "PublishedVersionSource",
    "StaticSource",
    "source_kind",
]


@runtime_checkable
class PublishedVersionSource(Protocol):
    """`latest(ecosystem, name)` -> the published version, or None if unknown."""

    def latest(self, ecosystem: str, name: str) -> str | None: ...


class OfflineSource:
    """Always `None`. The default posture, `--offline`, and the whole test suite.

    `None` becomes `status="unknown"` — never `current`. Reporting an unchecked
    package as up-to-date is the single worst output this tool can produce: it
    would let a live advisory be closed as `CLOSE-satisfied` in the
    LIBRARY-BUMP-STEWARD §2 triage.
    """

    source_kind = "offline"

    def latest(self, ecosystem: str, name: str) -> str | None:
        return None


class StaticSource:
    """A fixed `{(ecosystem, name): version}` mapping; `None` for anything absent."""

    source_kind = "static"

    def __init__(self, mapping: Mapping[tuple[str, str], str]) -> None:
        self._mapping = dict(mapping)

    def latest(self, ecosystem: str, name: str) -> str | None:
        return self._mapping.get((ecosystem, name))


class HttpRegistrySource:
    """crates.io / npm registry / PyPI JSON, via stdlib `urllib.request`.

    Used by Slice 33's online run ONLY — never by the acceptance suite, which is
    hermetic by construction. Any exception degrades to `None`, which the survey
    turns into `status="unknown"` with the reason recorded; it never degrades to
    `current`.

    No HTTP client library is taken for three GET-JSON endpoints (§5.7): adding
    an HTTP stack to a dependency-hygiene tool is exactly the bloat this tool
    exists to expose.
    """

    source_kind = "http"

    _ENDPOINTS = {
        "cargo": "https://crates.io/api/v1/crates/{name}",
        "npm": "https://registry.npmjs.org/{name}",
        "pypi": "https://pypi.org/pypi/{name}/json",
    }

    def __init__(self, timeout: float = 10.0, user_agent: str = "fathomdb-sbom-survey") -> None:
        self.timeout = timeout
        self.user_agent = user_agent

    def latest(self, ecosystem: str, name: str) -> str | None:
        template = self._ENDPOINTS.get(ecosystem)
        if template is None:
            return None
        url = template.format(name=urllib.parse.quote(name, safe="@/"))
        request = urllib.request.Request(url, headers={"User-Agent": self.user_agent})
        with urllib.request.urlopen(request, timeout=self.timeout) as response:  # noqa: S310
            payload = json.load(response)
        if ecosystem == "cargo":
            return payload["crate"]["max_stable_version"]
        if ecosystem == "npm":
            return payload["dist-tags"]["latest"]
        return payload["info"]["version"]


def source_kind(published: object) -> str:
    """The `source` string recorded in `staleness.json` (§5.8).

    Read off the source object rather than guessed, so an `--offline` run is
    recorded as `offline` and an online one as `http`. Slice 33 uses it to tell
    a checked run from an unchecked one.
    """
    kind = getattr(published, "source_kind", None)
    return kind if isinstance(kind, str) else "custom"
