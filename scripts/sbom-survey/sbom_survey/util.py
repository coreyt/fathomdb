"""Component identity — the `purl` that is also the `bom-ref` (design §5.5)."""

from __future__ import annotations

from packageurl import PackageURL

__all__ = ["make_purl"]

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
