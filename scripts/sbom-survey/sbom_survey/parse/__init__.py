"""Lockfile + manifest parsers (design §5.5).

Edges come from LOCKFILES; constraints and direct-ness come from MANIFESTS.

Why parse the tracked lockfile rather than shell out to `cargo metadata` /
`npm ls`, in order of weight:

1. **Hermeticity.** A subprocess needs the toolchain, a populated registry cache
   and frequently the network, which would make the acceptance suite
   non-hermetic and violate REQ-9 at its root.
2. **Fidelity to the question.** "What version are we *using*?" is answered by
   the tracked lockfile by definition. A subprocess can resolve differently from
   the committed lock and would then report something the repository does not
   contain.
3. **Cost.** A cold `cargo metadata` on this workspace is minutes; the whole
   point of the re-scope is a fast mechanical run.
"""

from __future__ import annotations

from dataclasses import dataclass, field

__all__ = ["Declaration", "LockPackage", "ManifestParseError"]


class ManifestParseError(Exception):
    """A tracked manifest or lockfile could not be parsed. CLI exit 3 (§5.9)."""

    def __init__(self, path: str, cause: BaseException) -> None:
        self.path = path
        self.cause = cause
        super().__init__(f"could not parse {path}: {cause!r}")


@dataclass(frozen=True)
class Declaration:
    """A dependency DECLARED by a tracked manifest.

    This is what makes a package `direct` (§5.5) and what supplies both the
    manifest constraint and the `edit_sites` a bump would have to touch.
    """

    ecosystem: str
    name: str
    constraint: str
    kind: str
    manifest_path: str


@dataclass
class LockPackage:
    """A package RESOLVED by a tracked lockfile, with its resolved edges."""

    ecosystem: str
    name: str
    version: str
    #: Opaque, per-lockfile identity — the key edges are resolved against.
    key: str
    #: Keys of the packages this one depends on, within the same lockfile.
    depends_on: list[str] = field(default_factory=list)
