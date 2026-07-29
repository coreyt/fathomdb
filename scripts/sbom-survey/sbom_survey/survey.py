"""The survey itself — `run_survey`, `Survey`, `classify_status` (design §5.4, §5.5, §5.8).

This module is the INTEGRATION BOUNDARY. Everything the requirements demand has
to be true *here*, not merely inside the helper this is supposed to call:

* the candidate manifests come from `discovery.discover_manifests()`, i.e. from
  `git ls-files`. There is no `os.walk`, no `glob`, no `rglob` and no literal
  `python/…` path anywhere in this package;
* exclusion and tiering come from the injected/loaded `TierMap` and from
  nothing else — there is no `startswith("dev/release/fixtures/")` in code;
* `published.latest()` is called once for EVERY surveyed component, so the
  used-versus-published diff is really produced rather than defaulted to
  `unknown`;
* the tier stamped on a component is the tier the rules assign to its declaring
  manifest.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable, Mapping

from packaging.utils import canonicalize_name

from . import TIER_VOCABULARY
from .constraints import matches as constraint_matches
from .cyclonedx import build_document
from .discovery import ManifestRef, discover_manifests
from .paths import DEFAULT_EPOCH_TIMESTAMP, DEFAULT_TIERS_FILE
from .parse import Declaration, LockPackage, ManifestParseError
from .parse import cargo as cargo_parse
from .parse import npm as npm_parse
from .parse import python as python_parse
from .registry import source_kind
from .tiers import TierMap, load_tier_map
from .util import make_purl

__all__ = [
    "ExcludedManifest",
    "Origin",
    "StalenessRow",
    "Survey",
    "SurveyComponent",
    "classify_status",
    "resolve_timestamp",
    "run_survey",
]

_TIER_RANK = {tier: rank for rank, tier in enumerate(TIER_VOCABULARY)}


# --------------------------------------------------------------------------- #
# version comparison (§5.4)
# --------------------------------------------------------------------------- #
def _parse_version(ecosystem: str, raw: str):
    """Parse with the comparator the ecosystem actually uses.

    Mixing the two is deliberate: PEP 440 rejects `1.2.3-rc.1` and semver
    rejects `1.2.3.post1`, and silently coercing either would produce wrong
    orderings — which lands back in the false-`current` failure mode this tool
    exists to avoid. Unparseable input yields `None` and therefore `unknown`.
    """
    try:
        if ecosystem == "pypi":
            from packaging.version import Version

            return Version(raw)
        import semver

        return semver.Version.parse(raw)
    except Exception:  # noqa: BLE001 - any parse failure means "we do not know"
        return None


def classify_status(ecosystem: str, locked: str | None, latest: str | None) -> str:
    """`outdated` / `current` / `ahead` / `unknown` — and `current` only honestly.

    `current` is reachable ONLY from a successfully parsed pair on both sides.
    There is no fallback, no default-to-current and no "assume current if we
    could not check": a false up-to-date would let a live advisory be closed as
    `CLOSE-satisfied` in LIBRARY-BUMP-STEWARD §2 triage.
    """
    if locked is None or latest is None:
        return "unknown"
    left = _parse_version(ecosystem, locked)
    right = _parse_version(ecosystem, latest)
    if left is None or right is None:
        return "unknown"
    if left == right:
        return "current"
    return "outdated" if left < right else "ahead"


# --------------------------------------------------------------------------- #
# the survey model
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class ExcludedManifest:
    """A tracked manifest deliberately kept out of the BOM, with its reason.

    Carried in the survey output (and mirrored into the CycloneDX
    `metadata.properties`) so the exclusion is AUDITABLE rather than invisible.
    """

    path: str
    reason: str
    note: str | None = None


@dataclass(frozen=True)
class Origin:
    """One tracked manifest that DECLARES a component, with the declared range."""

    path: str
    constraint: str
    kind: str

    def as_dict(self) -> dict[str, str]:
        return {"path": self.path, "constraint": self.constraint, "kind": self.kind}


@dataclass
class SurveyComponent:
    ecosystem: str
    name: str
    version: str | None
    purl: str
    tier: str
    depth: str
    origins: list[Origin] = field(default_factory=list)
    depends_on: list[str] = field(default_factory=list)
    lock_derived_edges: bool = False
    #: Present only on an unresolved component whose declaring constraint could
    #: not be matched to any locked version. Surfaced in the BOM and in the
    #: staleness row's `lookup_error`, so "why is this unknown" is answerable
    #: without re-deriving anything (REQ-14).
    resolution_note: str | None = None

    @property
    def edit_sites(self) -> list[str]:
        """The exact manifest paths a bump would have to touch (§5.8).

        This is Slice 33's input to "would a surgical ~1-5 SLOC change land it";
        the tool states no verdict, it supplies the sites.
        """
        return sorted({origin.path for origin in self.origins})


@dataclass(frozen=True)
class StalenessRow:
    """One row of the Slice-33 consumer contract (§5.8, REQ-14)."""

    ecosystem: str
    name: str
    tier: str
    depth: str
    locked_version: str | None
    latest_version: str | None
    status: str
    lookup_error: str | None
    declared_in: list[dict[str, str]]
    edit_sites: list[str]
    edit_site_count: int

    def as_dict(self) -> dict[str, Any]:
        return {
            "ecosystem": self.ecosystem,
            "name": self.name,
            "tier": self.tier,
            "depth": self.depth,
            "locked_version": self.locked_version,
            "latest_version": self.latest_version,
            "status": self.status,
            "lookup_error": self.lookup_error,
            "declared_in": [dict(entry) for entry in self.declared_in],
            "edit_sites": list(self.edit_sites),
            "edit_site_count": self.edit_site_count,
        }

    @property
    def sort_key(self) -> tuple[str, str, str, str]:
        return (self.tier, self.ecosystem, self.name, self.locked_version or "")


@dataclass
class Survey:
    """The result of one run: components, exclusions and staleness rows."""

    repo_root: Path
    timestamp: str
    source: str
    components: list[SurveyComponent]
    excluded: list[ExcludedManifest]
    manifests: list[ManifestRef]
    rows: list[StalenessRow]
    _document: tuple[dict[str, Any], str] | None = field(
        default=None, repr=False, compare=False
    )

    def staleness(self) -> list[StalenessRow]:
        """The staleness rows, in the ruled `(tier, ecosystem, name, locked)` order."""
        return list(self.rows)

    def summary(self) -> dict[str, int]:
        counts = {"components": len(self.rows)}
        for status in ("current", "outdated", "ahead", "unknown"):
            counts[status] = sum(1 for row in self.rows if row.status == status)
        counts["direct"] = sum(1 for row in self.rows if row.depth == "direct")
        counts["transitive"] = sum(1 for row in self.rows if row.depth == "transitive")
        counts["excluded_manifests"] = len(self.excluded)
        return counts

    def _built(self) -> tuple[dict[str, Any], str]:
        if self._document is None:
            self._document = build_document(self)
        return self._document

    def to_cyclonedx(self) -> dict[str, Any]:
        """The CycloneDX 1.6 document as a plain dict."""
        return self._built()[0]

    def to_cyclonedx_json(self) -> str:
        """The serialized CycloneDX 1.6 document, deterministic byte-for-byte."""
        return self._built()[1]


# --------------------------------------------------------------------------- #
# timestamps (§5.8)
# --------------------------------------------------------------------------- #
def resolve_timestamp(now: str | None) -> str:
    """The artifact timestamp: explicit `now`, else `SOURCE_DATE_EPOCH`, else the FIXED epoch.

    THIS IS THE ONLY PLACE A TIMESTAMP IS PRODUCED, and it never asks the
    operating system what time it is. `run_survey` and the CLI share it, so the
    CLI's default and the in-process default are the same value by
    construction — `argparse` cannot inject a wall-clock `--now` behind the
    survey's back (§5.8 leg 3).
    """
    if now:
        return _normalize_timestamp(now)
    epoch = os.environ.get("SOURCE_DATE_EPOCH")
    if epoch:
        try:
            return datetime.fromtimestamp(int(epoch), tz=timezone.utc).isoformat()
        except (TypeError, ValueError):
            pass
    return DEFAULT_EPOCH_TIMESTAMP


def _normalize_timestamp(raw: str) -> str:
    try:
        parsed = datetime.fromisoformat(raw.replace("Z", "+00:00"))
    except ValueError:
        return raw
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.isoformat()


# --------------------------------------------------------------------------- #
# component assembly
# --------------------------------------------------------------------------- #
@dataclass
class _Build:
    ecosystem: str
    name: str
    version: str | None
    purl: str
    lock_tiers: set[str] = field(default_factory=set)
    origins: dict[tuple[str, str], Origin] = field(default_factory=dict)
    depends_on: set[str] = field(default_factory=set)
    lock_derived_edges: bool = False
    #: Why this build carries no locked version, when that is not simply
    #: "nothing in any lockfile declares this name". Recorded rather than
    #: guessed — see `add_declarations`.
    resolution_notes: set[str] = field(default_factory=set)


def _match_key(ecosystem: str, name: str) -> tuple[str, str]:
    """The identity two spellings of the same package must share.

    pypi names are case- and separator-insensitive (PEP 503), so a
    `pyproject.toml` saying `PyYAML` and a lock saying `pyyaml` are one package.
    cargo and npm names are already canonical.
    """
    if ecosystem == "pypi":
        return (ecosystem, canonicalize_name(name))
    return (ecosystem, name)


class _Assembler:
    def __init__(self) -> None:
        self.builds: dict[str, _Build] = {}

    def ensure(self, ecosystem: str, name: str, version: str | None) -> _Build:
        purl = make_purl(ecosystem, name, version)
        build = self.builds.get(purl)
        if build is None:
            build = _Build(ecosystem=ecosystem, name=name, version=version, purl=purl)
            self.builds[purl] = build
        return build

    def add_lock(self, packages: Iterable[LockPackage], tier: str) -> None:
        key_to_purl: dict[str, str] = {}
        materialized: list[LockPackage] = []
        for package in packages:
            build = self.ensure(package.ecosystem, package.name, package.version)
            build.lock_tiers.add(tier)
            key_to_purl[package.key] = build.purl
            materialized.append(package)
        for package in materialized:
            build = self.builds[key_to_purl[package.key]]
            for target in package.depends_on:
                target_purl = key_to_purl.get(target)
                if target_purl is not None and target_purl != build.purl:
                    build.depends_on.add(target_purl)
                    # Lock `dependencies` lists are already feature-resolved and
                    # do not distinguish normal/dev/build edges, so lock-derived
                    # edges are tagged `resolved` rather than given a kind they
                    # do not carry (§5.5, honest limitation).
                    build.lock_derived_edges = True

    def by_match_key(self) -> dict[tuple[str, str], list[_Build]]:
        index: dict[tuple[str, str], list[_Build]] = {}
        for build in self.builds.values():
            index.setdefault(_match_key(build.ecosystem, build.name), []).append(build)
        return index

    def add_declarations(
        self,
        declarations: Iterable[Declaration],
        *,
        pinned: Mapping[tuple[str, str, str], str] | None = None,
    ) -> None:
        """Attach each declaration to the locked entries its constraint admits.

        A manifest declares a dependency by NAME and RANGE; a lockfile resolves
        it to one or more concrete versions. Attaching the declaration to EVERY
        locked version of that name is wrong the moment the lock carries more
        than one, and it produced provably false rows on this repository —
        `sha2 0.10.9` tagged `direct` under `constraint = "0.11"`, `thiserror
        2.0.18` under `constraint = "1"`, `tokenizers 0.22.2` under
        `constraint = "0.20"` (codex §9 round 1, fix-1). `depth` and
        `edit_sites` are the two fields Slice 33 decides on, so that corrupted
        the output the tool exists to produce.

        Three cases, and the third is the one that matters:

        1. **No lock entry for the name** — unchanged: materialize an unresolved
           component carrying the declaration (this is how a declared-but-never-
           locked package such as `numpy` reaches the BOM).
        2. **Exactly one candidate** — unchanged, and deliberately so: there is
           no ambiguity to resolve, so the constraint is not consulted and no
           imperfection in the range grammar can regress a correct attachment.
        3. **Several candidates** — consult `constraints.matches()` and attach
           ONLY to the entries the constraint admits.

        In case 3, when the constraint admits NONE of them — or when it could not
        be evaluated at all — the declaration is **not** widened back onto every
        candidate. That fallback IS the defect. Instead the declaration lands on
        an unresolved component that records WHY, so the manifest still appears
        in `edit_sites` (a real declaration is never silently dropped) while no
        locked version is falsely claimed to satisfy it.
        """
        index = self.by_match_key()
        pins = dict(pinned or {})
        for declaration in declarations:
            key = _match_key(declaration.ecosystem, declaration.name)
            candidates = index.get(key) or []
            origin = Origin(
                path=declaration.manifest_path,
                constraint=declaration.constraint,
                kind=declaration.kind,
            )

            if not candidates:
                pin = pins.get(
                    (declaration.ecosystem, declaration.name, declaration.manifest_path)
                )
                build = self.ensure(declaration.ecosystem, declaration.name, pin)
                index.setdefault(key, []).append(build)
                targets: list[_Build] = [build]
            elif len(candidates) == 1:
                targets = list(candidates)
            else:
                targets = self._resolve_ambiguous(declaration, candidates, index, key)

            for build in targets:
                build.origins[(origin.path, origin.kind)] = origin

    def _resolve_ambiguous(
        self,
        declaration: Declaration,
        candidates: list[_Build],
        index: dict[tuple[str, str], list[_Build]],
        key: tuple[str, str],
    ) -> list[_Build]:
        """The locked entries `declaration`'s constraint admits — or an honest miss."""
        locked = [build for build in candidates if build.version is not None]
        verdicts = [
            (
                build,
                constraint_matches(
                    declaration.ecosystem, declaration.constraint, build.version
                ),
            )
            for build in locked
        ]
        admitted = [build for build, verdict in verdicts if verdict is True]
        if admitted:
            return admitted

        # Nothing admitted. Say so in the data; NEVER widen back onto everything.
        unevaluable = any(verdict is None for _build, verdict in verdicts)
        versions = ", ".join(sorted(build.version or "" for build in locked))
        if unevaluable:
            note = (
                f"constraint {declaration.constraint!r} could not be evaluated"
                f" against the {len(locked)} locked versions of"
                f" {declaration.name!r} ({versions}), so no locked version is"
                " claimed to satisfy it"
            )
        else:
            note = (
                f"constraint {declaration.constraint!r} admits none of the"
                f" {len(locked)} locked versions of {declaration.name!r}"
                f" ({versions})"
            )
        build = self.ensure(declaration.ecosystem, declaration.name, None)
        build.resolution_notes.add(note)
        if build not in index.setdefault(key, []):
            index[key].append(build)
        return [build]


# --------------------------------------------------------------------------- #
# run_survey — the boundary
# --------------------------------------------------------------------------- #
def run_survey(
    repo_root: Path | str,
    *,
    published,
    tier_map: TierMap | None = None,
    now: str | None = None,
) -> Survey:
    """Survey `repo_root` and return the result.

    `published` is a REQUIRED keyword-only argument with NO default, so there is
    no code path that reaches the network implicitly (§5.4, REQ-9). `tier_map`
    and `now` default to the production behaviour and exist as seams because a
    rule and a timestamp that cannot be substituted cannot be *proved*
    load-bearing.

    Raises `UntieredManifestError` (naming the path) for a tracked manifest that
    matches no rule, and `ManifestParseError` for one that cannot be parsed.
    """
    root = Path(repo_root)
    rules = tier_map if tier_map is not None else load_tier_map(DEFAULT_TIERS_FILE)
    timestamp = resolve_timestamp(now)

    manifests = discover_manifests(root)

    included: list[tuple[ManifestRef, str]] = []
    excluded: list[ExcludedManifest] = []
    for ref in manifests:
        # Propagates UntieredManifestError naming the path: a manifest nobody
        # classified is a hard error, never a silent default (REQ-4).
        verdict = rules.classify(ref.path)
        if verdict.action == "exclude":
            excluded.append(
                ExcludedManifest(
                    path=ref.path,
                    reason=verdict.reason or "excluded",
                    note=verdict.rule.note,
                )
            )
            continue
        included.append((ref, verdict.tier or ""))

    manifest_tier = {ref.path: tier for ref, tier in included}
    assembler = _Assembler()

    # --- pass 1: lockfiles supply components and the library<->library edges --
    workspace_pins: dict[str, str] = {}
    for ref, tier in included:
        text = _read(root, ref)
        if ref.ecosystem == "cargo" and ref.kind == "lockfile":
            assembler.add_lock(cargo_parse.parse_lock(ref.path, text), tier)
        elif ref.ecosystem == "npm" and ref.kind == "lockfile":
            assembler.add_lock(npm_parse.parse_lock(ref.path, text), tier)
        elif ref.ecosystem == "pypi" and ref.path.endswith("uv.lock"):
            assembler.add_lock(python_parse.parse_uv_lock(ref.path, text), tier)
        elif ref.ecosystem == "cargo" and ref.kind == "manifest":
            workspace_pins.update(cargo_parse.workspace_dependencies(ref.path, text))

    # --- pass 2: manifests supply direct-ness and the declared constraints ----
    declarations: list[Declaration] = []
    pinned: dict[tuple[str, str, str], str] = {}
    for ref, _tier in included:
        if ref.kind != "manifest":
            continue
        text = _read(root, ref)
        name = ref.path.rsplit("/", 1)[-1]
        if ref.ecosystem == "cargo":
            declarations.extend(
                cargo_parse.parse_manifest(ref.path, text, workspace_pins=workspace_pins)
            )
        elif ref.ecosystem == "npm":
            declarations.extend(npm_parse.parse_manifest(ref.path, text))
        elif name == "pyproject.toml":
            declarations.extend(python_parse.parse_pyproject(ref.path, text))
        elif name.startswith("requirements") and name.endswith(".txt"):
            for declaration, pin in python_parse.parse_requirements(ref.path, text):
                declarations.append(declaration)
                if pin is not None:
                    pinned[(declaration.ecosystem, declaration.name, ref.path)] = pin
        # setup.py / setup.cfg / Pipfile are DISCOVERED and TIERED but never
        # parsed (§5.2): the only tracked ones are the excluded pip-skew
        # fixtures, so the most fragile parser in Python packaging is never
        # written. A real one would contribute no declarations rather than be
        # mis-parsed.

    assembler.add_declarations(declarations, pinned=pinned)

    # --- pass 3: tier + depth, then the staleness diff -----------------------
    components: list[SurveyComponent] = []
    for purl in sorted(assembler.builds):
        build = assembler.builds[purl]
        origins = [build.origins[key] for key in sorted(build.origins)]
        if origins:
            # The tier a component carries is the tier the RULES assign to its
            # declaring manifest — not a constant, and not the lockfile's.
            candidates = {manifest_tier[origin.path] for origin in origins}
        else:
            candidates = set(build.lock_tiers)
        if not candidates:  # pragma: no cover - impossible by construction
            raise RuntimeError(
                f"{build.purl}: no tier could be derived — the component reached"
                " the survey without a declaring manifest AND without a"
                " lockfile, which cannot happen while discovery is"
                " `git ls-files`-derived. Refusing to emit an untiered"
                " component (REQ-3/REQ-4)."
            )
        tier = min(candidates, key=lambda value: _TIER_RANK.get(value, len(_TIER_RANK)))
        components.append(
            SurveyComponent(
                ecosystem=build.ecosystem,
                name=build.name,
                version=build.version,
                purl=build.purl,
                tier=tier,
                # §5.5: `direct` IFF the name appears in a dependency table of a
                # tracked, non-excluded manifest — which is exactly "has an
                # origin". A direct component with no origin is therefore
                # impossible by construction, not merely unasserted.
                depth="direct" if origins else "transitive",
                origins=origins,
                depends_on=sorted(build.depends_on),
                lock_derived_edges=build.lock_derived_edges,
                resolution_note=(
                    "; ".join(sorted(build.resolution_notes))
                    if build.resolution_notes
                    else None
                ),
            )
        )

    rows = _staleness_rows(components, published)

    return Survey(
        repo_root=root,
        timestamp=timestamp,
        source=source_kind(published),
        components=components,
        excluded=sorted(excluded, key=lambda item: item.path),
        manifests=manifests,
        rows=rows,
    )


def _read(root: Path, ref: ManifestRef) -> str:
    try:
        return (root / ref.path).read_text(encoding="utf-8")
    except OSError as exc:
        raise ManifestParseError(ref.path, exc) from exc


def _staleness_rows(components: Iterable[SurveyComponent], published) -> list[StalenessRow]:
    """Ask the injected source about EVERY component, then classify honestly.

    The lookup is per component and unconditional: a survey that never called
    `published.latest()` would still pass every criterion that injects a source
    yielding `unknown` for everything, while producing none of the
    used-versus-published diff this tool exists to produce.
    """
    rows: list[StalenessRow] = []
    for component in components:
        latest: str | None = None
        # A row can be `unknown` for more than one reason at once, and Slice 33's
        # §5 "Unknowns" section needs all of them, so they ACCUMULATE rather than
        # overwrite. An unmatched constraint is recorded FIRST because it is a
        # property of the repository, not of the lookup.
        reasons: list[str] = []
        if component.resolution_note:
            reasons.append(component.resolution_note)
        # Tracked separately from `reasons`: a constraint that matched no locked
        # version says nothing about whether the REGISTRY answer is trustworthy,
        # so it must not suppress a perfectly good `latest`.
        lookup_failed = False
        try:
            latest = published.latest(component.ecosystem, component.name)
        except Exception as exc:  # noqa: BLE001 - any failure degrades to `unknown`
            latest = None
            lookup_failed = True
            reasons.append(f"{type(exc).__name__}: {exc}")
        if latest is not None and not isinstance(latest, str):
            reasons.append(f"published source returned a non-string latest: {latest!r}")
            latest = None
            lookup_failed = True

        status = classify_status(component.ecosystem, component.version, latest)
        if (
            status == "unknown"
            and not lookup_failed
            and component.version is not None
            and latest is not None
        ):
            reasons.append(
                "version comparison failed: could not parse"
                f" locked={component.version!r} / latest={latest!r} under the"
                f" {component.ecosystem} comparator"
            )
            lookup_failed = True
        if status == "unknown" and lookup_failed:
            # An unparseable or unavailable latest is never carried forward as a
            # version anybody could act on.
            latest = None

        lookup_error = "; ".join(reasons) if reasons else None

        edit_sites = component.edit_sites
        rows.append(
            StalenessRow(
                ecosystem=component.ecosystem,
                name=component.name,
                tier=component.tier,
                depth=component.depth,
                locked_version=component.version,
                latest_version=latest,
                status=status,
                lookup_error=lookup_error,
                declared_in=[origin.as_dict() for origin in component.origins],
                edit_sites=edit_sites,
                edit_site_count=len(edit_sites),
            )
        )
    return sorted(rows, key=lambda row: row.sort_key)
