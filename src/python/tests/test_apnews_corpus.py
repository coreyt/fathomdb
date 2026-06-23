"""Slice 5 — AP-News corpus + AutoQ loader tests (tiny synthetic fixtures only).

No real 1397-article payload is present in this worktree; every test authors a
handful of synthetic articles/questions into ``tmp_path`` and points the loader at
that root. Covers: the count + sha256 validity guard fires on a mismatch fixture;
AutoQ coverage spans every bucket (incl. ``data_linked``) with no empty bucket and
preserves v2 assertions.
"""

from __future__ import annotations

import hashlib
import json
import zipfile
from pathlib import Path

import pytest

from eval.apnews_corpus import (
    AUTOQ_BUCKETS,
    Article,
    CorpusValidityError,
    autoq_coverage,
    load_articles,
    load_autoq,
)

_V1_BUCKETS = ["activity_global", "activity_local", "data_global", "data_local"]
_V2_BUCKETS = ["data_global", "data_linked", "data_local"]


def _make_article(itemid: str, headline: str, body: str) -> dict:
    return {
        "altids": {"itemid": itemid},
        "headline": headline,
        "title": headline,
        "body_nitf": f"<p>{body}</p>",
    }


def _build_corpus(
    root: Path,
    *,
    manifest_n: int | None = None,
    sha: str | None = None,
    n_articles: int = 3,
) -> str:
    """Author a tiny synthetic corpus; return the REAL sha256 of the built zip."""
    root.mkdir(parents=True, exist_ok=True)
    articles = [
        _make_article(f"id{i:03d}", f"Headline {i}", f"Body text {i} about public health")
        for i in range(n_articles)
    ]
    zip_path = root / "raw_data.zip"
    with zipfile.ZipFile(zip_path, "w") as z:
        for i, a in enumerate(articles):
            z.writestr(f"2024/01/{i:02d}/{a['altids']['itemid']}.json", json.dumps(a))
    real_sha = hashlib.sha256(zip_path.read_bytes()).hexdigest()

    (root / "generated_questions_v1").mkdir(exist_ok=True)
    (root / "generated_questions_v2").mkdir(exist_ok=True)
    for b in _V1_BUCKETS:
        (root / "generated_questions_v1" / f"{b}_questions_text.json").write_text(
            json.dumps([f"{b} question one?", f"{b} question two?"]), encoding="utf-8"
        )
    for b in _V2_BUCKETS:
        items = [
            {
                "question_id": f"{b}-1",
                "question_text": f"{b} v2 question?",
                "assertions": [{"statement": f"{b} assertion alpha", "score": 9}],
                "claims": [],
            }
        ]
        (root / "generated_questions_v2" / f"{b}_questions_assertions.json").write_text(
            json.dumps(items), encoding="utf-8"
        )

    manifest = {
        "schema": "0.8.4-apnews-benchmarkqed-manifest-v1",
        "n_articles": manifest_n if manifest_n is not None else len(articles),
        "raw_data_zip_sha256": sha if sha is not None else real_sha,
        "autoq_questions": {
            "generated_questions_v1": [f"{b}_questions_text.json" for b in _V1_BUCKETS],
            "generated_questions_v2": [f"{b}_questions_assertions.json" for b in _V2_BUCKETS],
        },
    }
    (root / "MANIFEST.json").write_text(json.dumps(manifest), encoding="utf-8")
    return real_sha


# --------------------------------------------------------------------------- #
# Corpus loader
# --------------------------------------------------------------------------- #


def test_load_articles_ok(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root, n_articles=3)
    arts = load_articles(root)
    assert len(arts) == 3
    assert all(isinstance(a, Article) for a in arts)
    a0 = arts[0]
    assert a0.doc_id  # resolved from altids.itemid
    assert "<p>" not in a0.body and "</p>" not in a0.body  # html stripped
    assert "public health" in a0.body.lower()


def test_load_articles_deterministic(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root, n_articles=4)
    ids_a = [a.doc_id for a in load_articles(root)]
    ids_b = [a.doc_id for a in load_articles(root)]
    assert ids_a == ids_b == sorted(ids_a)  # stable, sorted order


def test_sha256_guard_fires(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root, sha="deadbeef" * 8)  # wrong sha, correct count
    with pytest.raises(CorpusValidityError, match="sha256"):
        load_articles(root)


def test_count_guard_fires(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root, manifest_n=999)  # correct sha, lying count
    with pytest.raises(CorpusValidityError, match="count"):
        load_articles(root)


def test_verify_false_skips_guard(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root, sha="deadbeef" * 8, manifest_n=999)
    arts = load_articles(root, verify=False)  # guards bypassed → loads anyway
    assert len(arts) == 3


def test_env_override(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root, n_articles=2)
    monkeypatch.setenv("FATHOMDB_APNEWS_CORPUS_ROOT", str(root))
    arts = load_articles()  # no explicit root → reads the env override
    assert len(arts) == 2


# --------------------------------------------------------------------------- #
# AutoQ loader
# --------------------------------------------------------------------------- #


def test_autoq_coverage_no_empty_bucket(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root)
    questions = load_autoq(root)
    cov = autoq_coverage(questions)
    assert set(cov) == set(AUTOQ_BUCKETS)
    assert all(count > 0 for count in cov.values()), cov  # no empty bucket
    assert cov["data_linked"] > 0  # data_linked present


def test_autoq_preserves_v2_assertions(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root)
    questions = load_autoq(root)
    linked = [q for q in questions if q.bucket == "data_linked"]
    assert linked
    q = linked[0]
    assert q.data_linked is True
    assert q.question_id == "data_linked-1"
    assert q.assertions and "assertion alpha" in q.assertions[0]


def test_autoq_v1_text_questions_have_no_assertions(tmp_path: Path) -> None:
    root = tmp_path / "corpus"
    _build_corpus(root)
    questions = load_autoq(root)
    ag = [q for q in questions if q.bucket == "activity_global"]
    assert ag
    assert all(q.assertions == () for q in ag)
    assert all(q.family == "activity" and q.scope == "global" for q in ag)
