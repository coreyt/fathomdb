"""Does a declared constraint admit a particular locked version?

**Why this exists** (fix-1, codex §9 round 1 `[P2]`). A manifest declares a
dependency by NAME and RANGE; a lockfile resolves it to one or more concrete
versions. Attaching a declaration to *every* locked version of that name is
wrong whenever the lock carries more than one, and it was wrong on this
repository in a way that corrupts the two fields Slice 33 makes its
surgical/not-surgical call on:

```text
sha2       0.10.9  direct  constraint='0.11'   <- false: 0.11 cannot resolve to 0.10.9
thiserror  2.0.18  direct  constraint='1'      <- false: ^1 cannot resolve to 2.0.18
tokenizers 0.22.2  direct  constraint='0.20'   <- false
```

So the survey asks this module which locked entries a constraint can actually
resolve to, and attaches the declaration only to those.

**The honesty rule this module is built around.** `matches()` returns
`True`/`False` only when it genuinely evaluated the constraint, and `None` when
it could not — an unparseable range, an opaque specifier (`workspace:`, a git
URL, a filesystem path), an unparseable version. `None` is NOT "no", and the
caller must never turn it into "attach to everything": widening on uncertainty
is exactly the defect this module was written to remove. See
`survey._Assembler.add_declarations` for what the caller does instead.

**No new dependency was taken for this.** PEP 440 is evaluated by `packaging`,
already declared for exactly that purpose (design §5.7); semver ranges are
evaluated here against `semver.Version`, also already declared. The supported
range grammar is deliberately bounded and everything outside it degrades to
`None` rather than to a guess.
"""

from __future__ import annotations

import re

import semver
from packaging.specifiers import InvalidSpecifier, SpecifierSet
from packaging.version import InvalidVersion, Version

__all__ = ["matches"]

#: Placeholders the manifest parsers emit when a dependency carries no version
#: range at all (`foo.workspace = true` with no root pin, `{ path = … }`,
#: `{ git = … }`). These are not ranges and must never be evaluated as one.
_OPAQUE_CONSTRAINTS = frozenset({"workspace", "path", "git"})

#: A constraint containing any of these is a URL, an alias, a filesystem path or
#: a dist-tag (`npm:pkg@^1`, `workspace:*`, `file:../x`, `git+https://…`,
#: `latest`), none of which this grammar covers.
_OPAQUE_CHARS = (":", "/", "\\")

_ANY = frozenset({"*", "x", "X", ""})

_HYPHEN_RANGE = re.compile(r"^\s*(v?[0-9][^\s]*)\s+-\s+(v?[0-9][^\s]*)\s*$")

_COMPARATOR = re.compile(
    r"\s*(\^|~>|~|>=|<=|>|<|==|=)?\s*(v?[0-9xX*][0-9A-Za-z.\-+*xX]*)\s*"
)

_PARTIAL = re.compile(
    r"^v?(\d+|[xX*])"
    r"(?:\.(\d+|[xX*]))?"
    r"(?:\.(\d+|[xX*]))?"
    r"(?:-([0-9A-Za-z.\-]+))?"
    r"(?:\+[0-9A-Za-z.\-]+)?$"
)


def matches(ecosystem: str, constraint: str | None, version: str | None) -> bool | None:
    """`True` / `False` if the constraint was evaluated, `None` if it could not be.

    `None` means "unknown", never "no" and never "yes".
    """
    if constraint is None or version is None:
        return None
    text = constraint.strip()
    if text in _ANY:
        return True
    if ecosystem == "pypi":
        return _matches_pep440(text, version)
    if ecosystem in ("cargo", "npm"):
        return _matches_semver(ecosystem, text, version)
    return None


def _matches_pep440(constraint: str, version: str) -> bool | None:
    try:
        specifier = SpecifierSet(constraint)
    except InvalidSpecifier:
        return None
    try:
        parsed = Version(version)
    except InvalidVersion:
        return None
    # `prereleases=True` because the question is "could the lock have resolved
    # this constraint to this version", and a lock that pinned a prerelease is
    # evidence that it could.
    return specifier.contains(parsed, prereleases=True)


def _matches_semver(ecosystem: str, constraint: str, version: str) -> bool | None:
    if constraint in _OPAQUE_CONSTRAINTS:
        return None
    if any(char in constraint for char in _OPAQUE_CHARS):
        return None
    try:
        actual = semver.Version.parse(version)
    except (TypeError, ValueError):
        return None

    saw_true = False
    for alternative in constraint.split("||"):
        verdict = _alternative(ecosystem, alternative, actual)
        if verdict is None:
            return None  # one unparseable alternative makes the whole thing unknown
        saw_true = saw_true or verdict
    return saw_true


def _alternative(ecosystem: str, text: str, actual: semver.Version) -> bool | None:
    """A whitespace/comma-separated comparator set — every comparator must hold."""
    cleaned = text.replace(",", " ").strip()
    if not cleaned:
        return True

    hyphen = _HYPHEN_RANGE.match(cleaned)
    if hyphen:
        pairs: list[tuple[str | None, str]] = [
            (">=", hyphen.group(1)),
            ("<=", hyphen.group(2)),
        ]
    else:
        pairs = []
        position = 0
        while position < len(cleaned):
            match = _COMPARATOR.match(cleaned, position)
            if match is None or match.end() == position:
                return None  # something outside the grammar: unknown, not "no"
            pairs.append((match.group(1), match.group(2)))
            position = match.end()
        if not pairs:
            return None

    for operator, raw in pairs:
        verdict = _comparator(ecosystem, operator, raw, actual)
        if verdict is None:
            return None
        if not verdict:
            return False
    return True


def _segment(raw: str | None) -> int | None:
    if raw is None or raw in ("x", "X", "*"):
        return None
    return int(raw)


def _version(major: int, minor: int | None, patch: int | None, pre: str | None = None):
    return semver.Version(major, minor or 0, patch or 0, prerelease=pre)


def _comparator(
    ecosystem: str,
    operator: str | None,
    raw: str,
    actual: semver.Version,
) -> bool | None:
    parsed = _PARTIAL.match(raw)
    if parsed is None:
        return None
    major = _segment(parsed.group(1))
    if major is None:
        return True  # a bare `x` / `*` major admits everything
    minor = _segment(parsed.group(2))
    patch = _segment(parsed.group(3))
    pre = parsed.group(4)

    if operator is None:
        # Cargo reads a bare requirement as CARET (`serde = "1"` is `^1`); npm
        # reads a bare full version as EXACT and a bare partial as a prefix
        # range. Coercing one to the other would silently mis-match, which is
        # the failure class this module exists to remove.
        operator = "^" if ecosystem == "cargo" else "="

    low = _version(major, minor, patch, pre)

    if operator == "^":
        if major > 0 or minor is None:
            high = _version(major + 1, 0, 0)
        elif minor > 0 or patch is None:
            high = _version(0, minor + 1, 0)
        else:
            high = _version(0, 0, patch + 1)
        return low <= actual < high

    if operator in ("~", "~>"):
        high = _version(major + 1, 0, 0) if minor is None else _version(major, minor + 1, 0)
        return low <= actual < high

    if operator == ">=":
        return actual >= low
    if operator == ">":
        return actual > low
    if operator == "<=":
        return actual <= low
    if operator == "<":
        return actual < low

    # `=` / `==`: exact when fully specified, a prefix range when partial.
    if minor is None:
        return low <= actual < _version(major + 1, 0, 0)
    if patch is None:
        return low <= actual < _version(major, minor + 1, 0)
    return actual == low
