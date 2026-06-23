"""0.8.4 AP-News BenchmarkQED corpus + AutoQ loader (Slice 5, $0 eval-infra).

Pure-stdlib (no ``fathomdb`` / no network / no LLM): reads the gitignored,
EVAL-ONLY corpus snapshot under ``data/corpus-data/raw/apnews_benchmarkqed`` —
1397 AP health-news articles packed in ``raw_data.zip`` plus the bundled Microsoft
AutoQ question sets. The payload itself is NEVER committed; this module imports and
unit-tests cleanly against tiny synthetic fixtures with no real corpus present.

Two responsibilities:

* :func:`load_articles` — unpack ``raw_data.zip`` into :class:`Article` records,
  guarded by a hard count + sha256 validity check against ``MANIFEST.json`` (a wrong
  / truncated / swapped payload fails loudly, never silently yields eval numbers).
* :func:`load_autoq` — load the bundled v1 (text) + v2 (assertion) question sets into
  a typed :class:`AutoQQuestion` list tagged by **bucket** (activity/data × global/
  local, plus ``data_linked``), preserving v2 assertions for later length-bias work.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Union

#: Env override for the corpus root (so a test can point at a tiny fixture and the
#: orchestrator can relocate the gitignored payload without code edits).
CORPUS_ROOT_ENV = "FATHOMDB_APNEWS_CORPUS_ROOT"

MANIFEST_NAME = "MANIFEST.json"
RAW_ZIP_NAME = "raw_data.zip"
MANIFEST_SCHEMA = "0.8.4-apnews-benchmarkqed-manifest-v1"

#: The canonical AutoQ buckets (activity/data × global/local, plus data_linked).
AUTOQ_BUCKETS = (
    "activity_global",
    "activity_local",
    "data_global",
    "data_local",
    "data_linked",
)

_PathLike = Union[str, "os.PathLike[str]"]


class CorpusValidityError(RuntimeError):
    """Raised when the on-disk corpus fails its count / sha256 validity guard."""


@dataclass(frozen=True)
class Article:
    """A single AP-News article. ``doc_id`` is the corpus join key (the AP item id);
    ``body`` is the headline + de-HTML'd article text (the retrieval surface)."""

    doc_id: str
    title: str
    body: str


@dataclass(frozen=True)
class AutoQQuestion:
    """A bundled Microsoft AutoQ question tagged by bucket.

    ``family`` is ``activity``/``data``; ``scope`` is ``global``/``local``/``linked``;
    ``assertions`` carries the v2 assertion statements (empty for v1 text-only
    questions). ``data_linked`` is the multi-hop cross-document bucket flag."""

    bucket: str
    family: str
    scope: str
    question_text: str
    question_id: Optional[str] = None
    assertions: tuple[str, ...] = ()
    data_linked: bool = False


# --------------------------------------------------------------------------- #
# Corpus root resolution
# --------------------------------------------------------------------------- #


def default_corpus_root() -> Path:
    """Default location of the gitignored corpus snapshot in the main checkout.

    ``eval/`` -> ``src/python`` -> ``src`` -> repo root, then into the EVAL-ONLY
    ``data/corpus-data`` tree (mirrors :func:`eval.r2_parity_eval._default_corpus_dir`)."""
    return (
        Path(__file__).resolve().parents[3] / "data" / "corpus-data" / "raw" / "apnews_benchmarkqed"
    )


def corpus_root(override: Optional[_PathLike] = None) -> Path:
    """Resolve the corpus root: explicit ``override`` > ``$FATHOMDB_APNEWS_CORPUS_ROOT``
    > :func:`default_corpus_root`."""
    if override is not None:
        return Path(override)
    env = os.environ.get(CORPUS_ROOT_ENV)
    if env:
        return Path(env)
    return default_corpus_root()


def load_manifest(root: Optional[_PathLike] = None) -> dict:
    """Read and return the parsed ``MANIFEST.json`` for the corpus root."""
    path = corpus_root(root) / MANIFEST_NAME
    if not path.exists():
        raise CorpusValidityError(f"MANIFEST.json missing at {path}")
    return json.loads(path.read_text(encoding="utf-8"))


# --------------------------------------------------------------------------- #
# Article loading + validity guard
# --------------------------------------------------------------------------- #

_TAG_RE = re.compile(r"<[^>]+>")
_WS_RE = re.compile(r"\s+")


def _strip_html(text: str) -> str:
    """Collapse the NITF body markup to plain text (tags -> space, runs -> single)."""
    return _WS_RE.sub(" ", _TAG_RE.sub(" ", text)).strip()


def _article_from_record(name: str, rec: dict) -> Article:
    altids = rec.get("altids") or {}
    doc_id = str(altids.get("itemid") or Path(name).stem)
    title = str(rec.get("headline") or rec.get("title") or "")
    body_html = str(rec.get("body_nitf") or rec.get("body") or "")
    body_text = _strip_html(body_html)
    body = f"{title}\n\n{body_text}".strip() if title else body_text
    return Article(doc_id=doc_id, title=title, body=body)


def _sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_articles(root: Optional[_PathLike] = None, *, verify: bool = True) -> list[Article]:
    """Unpack ``raw_data.zip`` into :class:`Article` records (sorted by archive name).

    When ``verify`` (default), enforce the corpus-validity guard against the manifest
    BEFORE returning any record:

    1. ``raw_data.zip`` sha256 must equal ``manifest["raw_data_zip_sha256"]``;
    2. the loaded article count must equal ``manifest["n_articles"]``.

    Either mismatch raises :class:`CorpusValidityError` (loud, never a silent eval on
    a wrong / truncated / swapped payload). ``verify=False`` is the escape hatch for
    fixture authoring / debugging only.
    """
    base = corpus_root(root)
    manifest = load_manifest(base)
    zip_path = base / RAW_ZIP_NAME
    if not zip_path.exists():
        raise CorpusValidityError(f"{RAW_ZIP_NAME} missing at {zip_path}")

    if verify:
        expected_sha = str(manifest.get("raw_data_zip_sha256", "")).strip().lower()
        actual_sha = _sha256_file(zip_path)
        if not expected_sha:
            raise CorpusValidityError(
                f"manifest at {base} has no raw_data_zip_sha256 to verify against"
            )
        if actual_sha != expected_sha:
            raise CorpusValidityError(
                f"{RAW_ZIP_NAME} sha256 mismatch at {zip_path}: "
                f"manifest={expected_sha!r} actual={actual_sha!r}"
            )

    articles: list[Article] = []
    with zipfile.ZipFile(zip_path) as zf:
        for name in sorted(n for n in zf.namelist() if n.endswith(".json")):
            rec = json.loads(zf.read(name))
            articles.append(_article_from_record(name, rec))

    if verify:
        n_expected = manifest.get("n_articles")
        if n_expected is not None and len(articles) != int(n_expected):
            raise CorpusValidityError(
                f"article count mismatch at {zip_path}: loaded {len(articles)} != "
                f"manifest n_articles {n_expected}"
            )
    return articles


# --------------------------------------------------------------------------- #
# AutoQ loading
# --------------------------------------------------------------------------- #

_BUCKET_RE = re.compile(r"(activity|data)_(global|local|linked)_questions_")


def _bucket_from_filename(filename: str) -> str:
    """Derive the canonical bucket (e.g. ``data_linked``) from an AutoQ filename."""
    m = _BUCKET_RE.match(Path(filename).name)
    if not m:
        raise CorpusValidityError(f"unrecognized AutoQ question filename: {filename!r}")
    bucket = f"{m.group(1)}_{m.group(2)}"
    if bucket not in AUTOQ_BUCKETS:
        raise CorpusValidityError(f"AutoQ filename {filename!r} maps to unknown bucket {bucket!r}")
    return bucket


def _question_from_item(item: object, bucket: str, family: str, scope: str) -> AutoQQuestion:
    data_linked = scope == "linked"
    if isinstance(item, str):  # v1 text-only question
        return AutoQQuestion(
            bucket=bucket,
            family=family,
            scope=scope,
            question_text=item,
            data_linked=data_linked,
        )
    if isinstance(item, dict):  # v2 question + assertions
        assertions = tuple(
            str(a.get("statement", ""))
            for a in (item.get("assertions") or [])
            if isinstance(a, dict)
        )
        qid = item.get("question_id")
        return AutoQQuestion(
            bucket=bucket,
            family=family,
            scope=scope,
            question_text=str(item.get("question_text") or item.get("question") or ""),
            question_id=str(qid) if qid is not None else None,
            assertions=assertions,
            data_linked=data_linked,
        )
    raise CorpusValidityError(f"unrecognized AutoQ item type {type(item)!r} in bucket {bucket!r}")


def load_autoq(root: Optional[_PathLike] = None) -> list[AutoQQuestion]:
    """Load the bundled Microsoft AutoQ question sets named by the manifest.

    Iterates ``manifest["autoq_questions"]`` (the v1 ``*_text.json`` and v2
    ``*_assertions.json`` files), tagging every question by bucket and preserving v2
    assertions where present. The v1 and v2 files for the same bucket are BOTH loaded
    (v1 contributes text-only questions, v2 contributes assertion-bearing ones).
    """
    base = corpus_root(root)
    manifest = load_manifest(base)
    autoq = manifest.get("autoq_questions") or {}
    questions: list[AutoQQuestion] = []
    for subdir, filenames in autoq.items():
        for filename in filenames:
            path = base / subdir / filename
            if not path.exists():
                raise CorpusValidityError(f"AutoQ question file missing: {path}")
            bucket = _bucket_from_filename(filename)
            family, scope = bucket.split("_", 1)
            data = json.loads(path.read_text(encoding="utf-8"))
            for item in data:
                questions.append(_question_from_item(item, bucket, family, scope))
    return questions


def autoq_coverage(questions: list[AutoQQuestion]) -> dict[str, int]:
    """Per-bucket question counts over all canonical buckets (0 for any absent bucket).

    The coverage accessor the design's local↔global span check reads: every bucket in
    :data:`AUTOQ_BUCKETS` is a key (so an empty bucket is visibly 0, not missing)."""
    cov: dict[str, int] = {b: 0 for b in AUTOQ_BUCKETS}
    for q in questions:
        cov[q.bucket] = cov.get(q.bucket, 0) + 1
    return cov
