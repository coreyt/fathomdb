"""Unit tests for the durable experiment-index helper (experiments/_lib.py).

The fathomdb mirror of memex's scheme: pure/typed, no network. Covers
canonical-JSON config hashing + run_id determinism, append-only index,
INDEX.md regeneration idempotence, the consumed-or-loudly-rejected record
schema, and the git/env/manifest-hash helpers (on tmp fixtures). The index row
carries repo="fathomdb" so the two repos' indices are cross-compatible.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from experiments import _lib  # noqa: E402

UTC = timezone.utc
TS = datetime(2026, 7, 2, 12, 30, 0, tzinfo=UTC)


def _valid_record_dict() -> dict:
    return {
        "run_id": "demo-20260702T1230Z-deadbeef",
        "experiment": "demo",
        "title": "demo run",
        "verdict": "pass",
        "read": "it worked",
        "code": {
            "git_sha": "abc1234",
            "dirty": False,
            "branch": "feat/x",
            "baseline_commit": "0000000",
        },
        "config": {"sha256": "deadbeef", "path": None, "resolved": {"a": 1}},
        "corpus": {
            "source": "fathomdb",
            "manifest_sha256": None,
            "datasets": ["wec_eng"],
        },
        "seeds": {"seed": 7},
        "env": {
            "python": "3.12.3",
            "lockfile_sha256": None,
            "gpu": None,
            "key_deps": {},
        },
        "metrics": {"score": 1.0},
        "cost_usd": 0.0,
        "tdd_evidence": {},
        "tests": None,
        "files_changed": [],
        "artifacts": [],
        "review": None,
        "open_questions": [],
    }


# --- canonical JSON + config hashing ---------------------------------------


def test_canonical_json_is_key_order_independent():
    a = _lib.canonical_json({"b": 1, "a": 2})
    b = _lib.canonical_json({"a": 2, "b": 1})
    assert a == b == '{"a":2,"b":1}'


def test_config_sha256_matches_manual_sha_of_canonical_json():
    cfg = {"z": 1, "a": {"n": 2}}
    expected = hashlib.sha256(_lib.canonical_json(cfg).encode("utf-8")).hexdigest()
    assert _lib.config_sha256(cfg) == expected


def test_config_sha256_changes_when_config_changes():
    assert _lib.config_sha256({"a": 1}) != _lib.config_sha256({"a": 2})


# --- run_id determinism -----------------------------------------------------


def test_make_run_id_is_deterministic_given_fixed_ts_and_config():
    sha = _lib.config_sha256({"a": 1})
    r1 = _lib.make_run_id("Cross Source Probe", TS, sha)
    r2 = _lib.make_run_id("Cross Source Probe", TS, sha)
    assert r1 == r2
    assert r1 == f"cross-source-probe-20260702T1230Z-{sha[:8]}"


# --- write_record -----------------------------------------------------------


def test_write_record_writes_the_three_files_and_appends_index(tmp_path):
    run_id, run_dir = _lib.write_record(
        "demo",
        ts=TS,
        config_obj={"a": 1, "b": 2},
        metrics={"score": 0.5},
        verdict="pass",
        read="ok",
        code={"git_sha": "abc", "dirty": False, "branch": "b", "baseline_commit": None},
        corpus={"source": "fathomdb", "manifest_sha256": None, "datasets": ["wec_eng"]},
        seeds={"seed": 7},
        env={"python": "3.12", "lockfile_sha256": None, "gpu": None, "key_deps": {}},
        cost_usd=0.0,
        base_dir=tmp_path,
    )
    assert run_dir == tmp_path / "runs" / run_id
    assert (run_dir / "record.json").is_file()
    assert (run_dir / "config.resolved.yaml").is_file()
    assert (run_dir / "metrics.json").is_file()

    record = json.loads((run_dir / "record.json").read_text())
    assert record["run_id"] == run_id
    assert record["config"]["sha256"] == _lib.config_sha256({"a": 1, "b": 2})
    assert record["config"]["resolved"] == {"a": 1, "b": 2}
    assert json.loads((run_dir / "metrics.json").read_text()) == {"score": 0.5}

    lines = (tmp_path / "index.jsonl").read_text().splitlines()
    assert len(lines) == 1
    row = json.loads(lines[0])
    assert row["run_id"] == run_id
    assert row["repo"] == "fathomdb"
    assert row["verdict"] == "pass"
    assert row["config_sha256"] == record["config"]["sha256"]


def test_write_record_run_id_stable_across_calls(tmp_path):
    kwargs = dict(
        ts=TS,
        config_obj={"a": 1},
        metrics={},
        verdict="pass",
        read="ok",
        code={"git_sha": "abc", "dirty": False, "branch": "b", "baseline_commit": None},
        corpus={"source": None, "manifest_sha256": None, "datasets": []},
        seeds={},
        env={"python": "3.12", "lockfile_sha256": None, "gpu": None, "key_deps": {}},
        cost_usd=0.0,
    )
    run_id_a, _ = _lib.write_record("demo", base_dir=tmp_path / "a", **kwargs)
    run_id_b, _ = _lib.write_record("demo", base_dir=tmp_path / "b", **kwargs)
    assert run_id_a == run_id_b


def test_write_record_does_not_double_append_same_run_id(tmp_path):
    kwargs = dict(
        ts=TS,
        config_obj={"a": 1},
        metrics={},
        verdict="pass",
        read="ok",
        code={"git_sha": "abc", "dirty": False, "branch": "b", "baseline_commit": None},
        corpus={"source": None, "manifest_sha256": None, "datasets": []},
        seeds={},
        env={"python": "3.12", "lockfile_sha256": None, "gpu": None, "key_deps": {}},
        cost_usd=0.0,
        base_dir=tmp_path,
    )
    rid1, _ = _lib.write_record("demo", **kwargs)
    rid2, _ = _lib.write_record("demo", **kwargs)
    assert rid1 == rid2
    lines = (tmp_path / "index.jsonl").read_text().splitlines()
    assert len(lines) == 1


# --- append_index is append-only -------------------------------------------


def test_append_index_never_rewrites_existing_lines(tmp_path):
    idx = tmp_path / "index.jsonl"
    _lib.append_index({"run_id": "one", "verdict": "a"}, index_path=idx)
    first = idx.read_text()
    _lib.append_index({"run_id": "two", "verdict": "b"}, index_path=idx)
    lines = idx.read_text().splitlines()
    assert len(lines) == 2
    assert idx.read_text().startswith(first)
    assert json.loads(lines[0])["run_id"] == "one"
    assert json.loads(lines[1])["run_id"] == "two"


def test_append_index_self_heals_missing_trailing_newline(tmp_path):
    idx = tmp_path / "index.jsonl"
    idx.write_text('{"run_id": "partial"}', encoding="utf-8")  # no trailing newline
    _lib.append_index({"run_id": "next"}, index_path=idx)
    lines = idx.read_text().splitlines()
    assert len(lines) == 2
    assert json.loads(lines[0])["run_id"] == "partial"
    assert json.loads(lines[1])["run_id"] == "next"


# --- regen_index_md idempotence --------------------------------------------


def test_regen_index_md_is_idempotent_and_lists_runs(tmp_path):
    idx = tmp_path / "index.jsonl"
    md = tmp_path / "INDEX.md"
    _lib.append_index(
        {
            "run_id": "r1",
            "experiment": "e1",
            "ts": "2026-07-02T12:30:00Z",
            "verdict": "pass",
            "git_sha": "abc1234",
            "n": 5,
            "cost_usd": 0.0,
            "headline": {"score": 1},
            "review": None,
        },
        index_path=idx,
    )
    _lib.append_index(
        {
            "run_id": "r2",
            "experiment": "e2",
            "ts": "2026-07-02T13:30:00Z",
            "verdict": "no-go",
            "git_sha": "def5678",
            "n": 9,
            "cost_usd": 0.5,
            "headline": {"score": 0},
            "review": None,
        },
        index_path=idx,
    )
    _lib.regen_index_md(index_path=idx, md_path=md)
    once = md.read_text()
    _lib.regen_index_md(index_path=idx, md_path=md)
    twice = md.read_text()
    assert once == twice
    assert "r1" in once and "r2" in once
    assert once.lstrip().startswith("#")


# --- record schema: consumed-or-loudly-rejected ----------------------------


def test_record_from_dict_accepts_a_valid_record():
    rec = _lib.record_from_dict(_valid_record_dict())
    assert rec.run_id == "demo-20260702T1230Z-deadbeef"
    assert rec.code.git_sha == "abc1234"
    assert rec.corpus.datasets == ["wec_eng"]


def test_record_from_dict_rejects_unknown_top_level_key():
    bad = _valid_record_dict()
    bad["bogus"] = 1
    with pytest.raises(ValueError, match="unknown"):
        _lib.record_from_dict(bad)


def test_record_from_dict_rejects_missing_required_key():
    bad = _valid_record_dict()
    del bad["verdict"]
    with pytest.raises(ValueError, match="missing"):
        _lib.record_from_dict(bad)


def test_record_from_dict_rejects_unknown_nested_key():
    bad = _valid_record_dict()
    bad["code"]["extra"] = 1
    with pytest.raises(ValueError, match="unknown"):
        _lib.record_from_dict(bad)


# --- git / env / manifest hash helpers (tmp fixtures) ----------------------


def _init_git_repo(root):
    """Create an isolated git repo at ``root`` using a SANITISED env, so the
    fixture can never touch a parent repo even if the test process inherited a
    hook's GIT_DIR/GIT_WORK_TREE."""

    def run(*args):
        subprocess.run(
            ["git", *args],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=_lib.git_env(),
        )

    run("init")
    run("config", "user.email", "t@t")
    run("config", "user.name", "t")
    return run


def test_git_info_reports_sha_and_dirty_flag(tmp_path):
    run = _init_git_repo(tmp_path)
    (tmp_path / "f.txt").write_text("hi")
    run("add", "f.txt")
    run("commit", "-m", "init")
    info = _lib.git_info(repo_root=tmp_path)
    assert isinstance(info["git_sha"], str) and len(info["git_sha"]) == 40
    assert info["dirty"] is False
    assert isinstance(info["branch"], str) and info["branch"]

    (tmp_path / "g.txt").write_text("new")
    assert _lib.git_info(repo_root=tmp_path)["dirty"] is True


def test_git_info_ignores_inherited_git_dir(tmp_path, monkeypatch):
    # Regression: a git hook (the pre-push suite) exports GIT_DIR/GIT_WORK_TREE.
    # git_info(repo_root=...) MUST honour repo_root, not the inherited hook dir.
    real = tmp_path / "real"
    real.mkdir()
    run = _init_git_repo(real)
    (real / "a").write_text("x")
    run("add", "a")
    run("commit", "-m", "c")
    real_sha = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=real,
        capture_output=True,
        text=True,
        env=_lib.git_env(),
    ).stdout.strip()

    decoy = tmp_path / "decoy"
    monkeypatch.setenv("GIT_DIR", str(decoy / ".git"))
    monkeypatch.setenv("GIT_WORK_TREE", str(decoy))
    info = _lib.git_info(repo_root=real)
    assert info["git_sha"] == real_sha


def test_env_info_hashes_the_lockfile(tmp_path):
    lock = tmp_path / "uv.lock"
    lock.write_text("lock-contents")
    info = _lib.env_info(repo_root=tmp_path)
    assert info["python"]
    assert info["gpu"] is None
    assert info["lockfile_sha256"] == hashlib.sha256(b"lock-contents").hexdigest()


def test_env_info_lockfile_none_when_absent(tmp_path):
    assert _lib.env_info(repo_root=tmp_path)["lockfile_sha256"] is None


def test_corpus_manifest_sha256_hashes_present_file(tmp_path):
    man = tmp_path / "manifest.json"
    man.write_text('{"sources": {}}')
    got = _lib.corpus_manifest_sha256(path=man)
    assert got == hashlib.sha256(b'{"sources": {}}').hexdigest()


def test_corpus_manifest_sha256_none_when_absent(tmp_path):
    assert _lib.corpus_manifest_sha256(path=tmp_path / "nope.json") is None


def test_corpus_manifest_path_points_at_in_repo_manifest():
    p = Path(_lib.corpus_manifest_path())
    assert p.name == "manifest.json"
    assert p.parts[-4:] == ("tests", "corpus", "scripts", "manifest.json")
