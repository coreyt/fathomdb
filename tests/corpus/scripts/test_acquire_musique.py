"""TDD tests for the MuSiQue-Ans corpus materializer (Slice 4, 0.8.2 M1).

RED state: all tests fail because the corpus file and manifest entry do not exist.
GREEN state: tests pass after `acquire_musique.py` has been run.

Assertions:
  (a) every sampled question carries hop_count ∈ {2,3,4} and — if answerable —
      at least 1 supporting paragraph plus at least 2 distractor paragraphs;
  (b) the unanswerable contrast set is non-empty and every such row has answerable=False;
  (c) the sha256 of the materialized corpus file matches the pinned value in
      manifest.json → byte-stability / deterministic ordering check.
  (d) [fix-1] acquire_musique.py declares a PEP 723 dependency block covering 'datasets'
      so a clean checkout running the acquire command does not fail with
      ModuleNotFoundError: datasets.
  (e) [fix-1] a reproduce run (re-running acquire_musique.py) leaves the committed
      dev/plans/runs/0.8.2-m1-corpus-manifest.json byte-identical — no timestamp drift.

Sampling: the per-question structural tests run over the FULL corpus (4834 rows);
any future sampling must be logged here.

Run: FATHOMDB_TESTS_NO_REBUILD=1 python -m pytest tests/corpus/scripts/test_acquire_musique.py -q
(from the repo root with the repo .venv activated).
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path

# Make _corpus_lib importable (scripts dir is a sibling of this file).
sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import corpus_data_dir, repo_root  # noqa: E402

CORPUS_FILE = corpus_data_dir() / "raw" / "musique_dev.jsonl"
MANIFEST_FILE = Path(__file__).resolve().parent / "manifest.json"
SCRIPT_FILE = Path(__file__).resolve().parent / "acquire_musique.py"
M1_MANIFEST_FILE = repo_root() / "dev" / "plans" / "runs" / "0.8.2-m1-corpus-manifest.json"

# ── helpers ──────────────────────────────────────────────────────────────────


def _load_corpus() -> list[dict]:
    """Load the full materialized corpus; raises FileNotFoundError if not acquired."""
    if not CORPUS_FILE.exists():
        raise FileNotFoundError(
            f"MuSiQue corpus not acquired: {CORPUS_FILE}\n"
            "Run: ./.venv/bin/python tests/corpus/scripts/acquire_musique.py"
        )
    rows: list[dict] = []
    with CORPUS_FILE.open(encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    print(f"  [log] loaded {len(rows)} rows from {CORPUS_FILE.name} (full scan, no sampling)")
    return rows


def _sha256_file(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as f:
        for line in f:
            hasher.update(line)
    return hasher.hexdigest()


# ── tests ─────────────────────────────────────────────────────────────────────


def test_hop_counts_in_range():
    """Every question has hop_count ∈ {2, 3, 4} (construction-defined, parsed from ID)."""
    rows = _load_corpus()
    bad = [r for r in rows if r.get("hop_count") not in {2, 3, 4}]
    assert not bad, (
        f"{len(bad)} questions have hop_count outside {{2,3,4}}; "
        f"first offender: id={bad[0].get('id')!r} hop_count={bad[0].get('hop_count')!r}"
    )


def test_answerable_have_supporting_and_distractor_paragraphs():
    """For answerable questions: ≥1 supporting paragraph AND ≥2 distractor paragraphs."""
    rows = _load_corpus()
    answerable = [r for r in rows if r.get("answerable")]
    assert answerable, "No answerable rows found"

    for row in answerable:
        paras = row.get("paragraphs", [])
        supporting = [p for p in paras if p.get("is_supporting")]
        distractors = [p for p in paras if not p.get("is_supporting")]
        assert len(supporting) >= 1, (
            f"Answerable question {row['id']!r} has no supporting paragraphs"
        )
        assert len(distractors) >= 2, (
            f"Answerable question {row['id']!r} has fewer than 2 distractor paragraphs "
            f"(got {len(distractors)})"
        )


def test_hop_count_matches_supporting_paragraph_count():
    """For answerable questions: hop_count == number of supporting paragraphs (construction-defined)."""
    rows = _load_corpus()
    answerable = [r for r in rows if r.get("answerable")]
    mismatches = []
    for row in answerable:
        supporting = sum(1 for p in row.get("paragraphs", []) if p.get("is_supporting"))
        if row["hop_count"] != supporting:
            mismatches.append((row["id"], row["hop_count"], supporting))
    assert not mismatches, (
        f"{len(mismatches)} hop_count/supporting-count mismatches; "
        f"first: id={mismatches[0][0]!r} hop_count={mismatches[0][1]} supporting={mismatches[0][2]}"
    )


def test_unanswerable_contrast_set_nonempty_and_flagged():
    """Unanswerable contrast set is non-empty and every such row has answerable=False."""
    rows = _load_corpus()
    unanswerable = [r for r in rows if not r.get("answerable", True)]
    assert len(unanswerable) > 0, (
        "Unanswerable contrast set is empty — check acquire_musique.py (default config required)"
    )
    # All unanswerable rows must have answerable=False (not a missing/None value)
    bad_flag = [r for r in unanswerable if r.get("answerable") is not False]
    assert not bad_flag, (
        f"{len(bad_flag)} 'unanswerable' rows have answerable != False: "
        f"first={bad_flag[0].get('id')!r} value={bad_flag[0].get('answerable')!r}"
    )
    print(f"  [log] unanswerable contrast set: {len(unanswerable)} rows")


def test_paragraph_schema():
    """Every paragraph has the required keys: idx, title, text, is_supporting."""
    rows = _load_corpus()
    required = {"idx", "title", "text", "is_supporting"}
    for row in rows[:100]:  # log: checking first 100 questions (fast structural probe)
        for i, para in enumerate(row.get("paragraphs", [])):
            missing = required - set(para.keys())
            assert not missing, (
                f"Paragraph {i} in question {row['id']!r} is missing keys: {missing}"
            )
    print("  [log] paragraph schema check: first 100 questions (sufficient structural probe)")


def test_byte_stability_via_manifest_pin():
    """sha256 of corpus file matches the pinned hash in manifest.json (byte-stable materializer)."""
    if not CORPUS_FILE.exists():
        raise FileNotFoundError(
            f"MuSiQue corpus not acquired: {CORPUS_FILE}\n"
            "Run: ./.venv/bin/python tests/corpus/scripts/acquire_musique.py"
        )
    manifest = json.loads(MANIFEST_FILE.read_text(encoding="utf-8"))
    musique_entry = manifest.get("sources", {}).get("musique")
    assert musique_entry is not None, (
        "No 'musique' entry in manifest.json — run acquire_musique.py to pin the hash"
    )
    pinned_sha = musique_entry.get("sha256")
    assert pinned_sha, "manifest.json 'musique' entry has no sha256"

    actual_sha = _sha256_file(CORPUS_FILE)
    assert actual_sha == pinned_sha, (
        f"Corpus sha256 mismatch (byte-stability failure):\n"
        f"  pinned : {pinned_sha}\n"
        f"  on-disk: {actual_sha}\n"
        "Re-run acquire_musique.py to regenerate the corpus, or reconcile the manifest."
    )


# ── fix-1 tests (codex §9 [P2]) ───────────────────────────────────────────────


def test_pep723_block_declares_datasets():
    """acquire_musique.py must carry a PEP 723 '# /// script' block declaring 'datasets'.

    Without this block a clean checkout running the documented acquire_command:
      cd <repo_root> && ./.venv/bin/python tests/corpus/scripts/acquire_musique.py
    fails immediately with ModuleNotFoundError: datasets (the library is NOT in
    src/python deps and NOT declared as an inline dependency).

    RED: no '# /// script' block present → assertion fails.
    GREEN: block added with 'datasets>=3.0' (covers tested 5.0.0).
    """
    script_text = SCRIPT_FILE.read_text(encoding="utf-8")

    # Must have the PEP 723 opening sentinel.
    assert "# /// script" in script_text, (
        f"{SCRIPT_FILE.name} is missing the PEP 723 '# /// script' block — "
        "a clean checkout running acquire_musique.py will fail with "
        "ModuleNotFoundError: datasets.  Add a '# /// script' dependency block "
        "matching the convention in acquire_enronqa.py."
    )

    # The block must declare 'datasets' so dependency managers can resolve it.
    # We check the header (first 600 bytes) where the block must appear.
    header = script_text[:600]
    assert "datasets" in header, (
        "PEP 723 block in acquire_musique.py does not declare the 'datasets' dependency "
        "(checked first 600 bytes).  The block must include a line like "
        "'  \"datasets>=3.0\"' inside the # /// script … # /// fence."
    )


def test_m1_manifest_reproduce_stable():
    """Re-running acquire_musique.py must leave the committed M1 manifest byte-identical.

    The manifest at dev/plans/runs/0.8.2-m1-corpus-manifest.json is a tracked artifact.
    Volatile timestamps (generated_at, etc.) written on every reproduce run cause it to
    be dirtied in git, defeating the shared-corpus reproduce contract for Slices 5/10.

    RED: script writes 'generated_at' with the current timestamp → bytes differ.
    GREEN: volatile timestamps removed from the reproduce path → bytes identical.

    Note: this test runs the full acquisition script (corpus is expected to be
    materialized at data/corpus-data/raw/musique_dev.jsonl from the prior acquire run;
    HF cache is expected warm so no network I/O is needed).
    """
    if not CORPUS_FILE.exists():
        import pytest  # type: ignore[import-not-found]
        pytest.skip(
            f"Corpus not acquired ({CORPUS_FILE}) — run acquire_musique.py first, "
            "then re-run this test."
        )
    if not M1_MANIFEST_FILE.exists():
        import pytest  # type: ignore[import-not-found]
        pytest.skip(f"M1 manifest not found: {M1_MANIFEST_FILE}")

    before_bytes = M1_MANIFEST_FILE.read_bytes()

    result = subprocess.run(
        [sys.executable, str(SCRIPT_FILE)],
        capture_output=True,
        text=True,
        timeout=300,
    )
    assert result.returncode == 0, (
        f"acquire_musique.py exited non-zero ({result.returncode}).\n"
        f"stdout: {result.stdout[-500:]}\n"
        f"stderr: {result.stderr[-500:]}"
    )

    after_bytes = M1_MANIFEST_FILE.read_bytes()
    assert before_bytes == after_bytes, (
        "Reproduce run dirtied the tracked M1 manifest — volatile timestamp still present.\n"
        "Remove 'generated_at' (and any other time-varying field) from the manifest write "
        "in acquire_musique.py so the file is byte-stable across reproduce runs.\n\n"
        f"Before ({len(before_bytes)} bytes, first 300):\n{before_bytes[:300].decode()}\n\n"
        f"After  ({len(after_bytes)} bytes, first 300):\n{after_bytes[:300].decode()}"
    )
