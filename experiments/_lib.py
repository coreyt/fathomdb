"""Durable experiment-tracking helper (fathomdb side).

Pure, typed, no network. The fathomdb mirror of memex's experiment index: a run
is a typed CONFIG + an index line. This module computes a config hash + a
deterministic ``run_id``, writes the canonical per-run ``record.json`` (+
``config.resolved.yaml`` + ``metrics.json``) under ``runs/<run_id>/``, and
appends ONE line to the append-only ``index.jsonl`` source-of-truth. ``INDEX.md``
is a generated, never-hand-edited human table regenerated FROM ``index.jsonl``.

The record + index-row schema is IDENTICAL to memex's so the two repos' indices
are cross-compatible; the index row's ``repo="fathomdb"`` field distinguishes the
two. The record schema is a typed dataclass, consumed-or-loudly-rejected — an
unknown or missing field raises rather than being silently dropped. Determinism:
the timestamp is passed IN by the caller (a live runner supplies
``datetime.now(UTC)``), never computed here, so the module stays pure/testable.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import subprocess
from dataclasses import MISSING, asdict, dataclass, fields, is_dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

try:  # PyYAML is present in the dev venv; JSON is a valid YAML subset so
    # config.resolved.yaml stays readable either way.
    import yaml

    _HAVE_YAML = True
except ImportError:  # pragma: no cover - exercised only where PyYAML is absent
    yaml = None
    _HAVE_YAML = False

#: experiments/ (this file's dir) and the repo root above it.
EXPERIMENTS_DIR = Path(__file__).resolve().parent
REPO_ROOT = EXPERIMENTS_DIR.parent
INDEX_PATH = EXPERIMENTS_DIR / "index.jsonl"
INDEX_MD_PATH = EXPERIMENTS_DIR / "INDEX.md"

#: The in-repo corpus manifest — the corpus-snapshot pin for a run (read if
#: present). Unlike the memex side (where fathomdb is a SIBLING repo), here the
#: manifest is local to this repo.
_MANIFEST_RELPATH = "tests/corpus/scripts/manifest.json"


def corpus_manifest_path() -> str:
    """Absolute path to the in-repo corpus manifest."""
    return str(REPO_ROOT / _MANIFEST_RELPATH)


# --- typed record schema ----------------------------------------------------


@dataclass
class Code:
    git_sha: str
    dirty: bool
    branch: str
    baseline_commit: str | None


@dataclass
class ConfigRef:
    sha256: str
    path: str | None
    resolved: dict


@dataclass
class Corpus:
    source: str | None
    manifest_sha256: str | None
    datasets: list


@dataclass
class Env:
    python: str
    lockfile_sha256: str | None
    gpu: Any
    key_deps: dict


@dataclass
class Record:
    run_id: str
    experiment: str
    title: str
    verdict: str
    read: str
    code: Code
    config: ConfigRef
    corpus: Corpus
    seeds: dict
    env: Env
    metrics: Any
    cost_usd: float
    tdd_evidence: dict
    tests: Any
    files_changed: list
    artifacts: list
    review: Any
    open_questions: list


_NESTED: dict[str, type] = {
    "code": Code,
    "config": ConfigRef,
    "corpus": Corpus,
    "env": Env,
}


def _build_nested(cls: type, value: Any, name: str) -> Any:
    if not isinstance(value, dict):
        raise ValueError(f"record section {name!r} must be a mapping")
    known = {f.name for f in fields(cls)}
    unknown = set(value) - known
    if unknown:
        raise ValueError(f"unknown {name} keys: {sorted(unknown)}")
    required = {
        f.name
        for f in fields(cls)
        if f.default is MISSING and f.default_factory is MISSING
    }
    missing = required - set(value)
    if missing:
        raise ValueError(f"missing {name} keys: {sorted(missing)}")
    return cls(**value)


def record_from_dict(data: dict[str, Any]) -> Record:
    """Build a typed ``Record`` from a mapping, rejecting unknown/missing keys
    (top-level AND nested) so a typo fails loudly instead of being dropped."""
    if not isinstance(data, dict):
        raise ValueError(f"record must be a mapping, got {type(data).__name__}")
    known = {f.name for f in fields(Record)}
    unknown = set(data) - known
    if unknown:
        raise ValueError(f"unknown record keys: {sorted(unknown)}")
    required = {
        f.name
        for f in fields(Record)
        if f.default is MISSING and f.default_factory is MISSING
    }
    missing = required - set(data)
    if missing:
        raise ValueError(f"missing record keys: {sorted(missing)}")
    kwargs: dict[str, Any] = {}
    for key, value in data.items():
        if key in _NESTED:
            kwargs[key] = _build_nested(_NESTED[key], value, key)
        else:
            kwargs[key] = value
    return Record(**kwargs)


# --- canonical JSON + hashing ----------------------------------------------


def canonical_json(obj: Any) -> str:
    """Deterministic, key-sorted, whitespace-free JSON (the hash preimage)."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _resolved_dict(config_obj: Any) -> dict:
    if is_dataclass(config_obj) and not isinstance(config_obj, type):
        return asdict(config_obj)
    if isinstance(config_obj, dict):
        return config_obj
    raise TypeError(
        f"config_obj must be a dataclass or dict, got {type(config_obj).__name__}"
    )


def config_sha256(config_obj: Any) -> str:
    """sha256 of the canonical-JSON of the (resolved) config."""
    resolved = _resolved_dict(config_obj)
    return hashlib.sha256(canonical_json(resolved).encode("utf-8")).hexdigest()


def _slug(text: str) -> str:
    s = re.sub(r"[^a-zA-Z0-9]+", "-", text.strip().lower()).strip("-")
    return s or "experiment"


def _ts_compact(ts: datetime) -> str:
    return ts.strftime("%Y%m%dT%H%MZ")


def _ts_iso(ts: datetime) -> str:
    return ts.strftime("%Y-%m-%dT%H:%M:%SZ")


def make_run_id(experiment: str, ts: datetime, sha256: str) -> str:
    """``<experiment-slug>-<UTC-ts:YYYYMMDDTHHMMZ>-<config_sha8>`` (deterministic
    given a fixed ``ts`` + config hash)."""
    return f"{_slug(experiment)}-{_ts_compact(ts)}-{sha256[:8]}"


# --- git / env / manifest helpers ------------------------------------------


#: Env vars that pin git's repo LOCATION. A git hook (the pre-push suite runs
#: one) exports these pointing at the CURRENT repo; if inherited by a subprocess
#: that means to operate on a different repo (a tmp fixture, or an explicit
#: ``repo_root``), git would silently target the hook's repo instead. Strip them
#: so ``cwd`` / ``-C`` is authoritative.
_GIT_LOCATION_ENV = (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_NAMESPACE",
)


def git_env(overrides: dict | None = None) -> dict:
    """A copy of the process env with git's repo-location vars stripped (so
    ``cwd``/``-C`` decides the repo), plus optional ``overrides``."""
    env = {k: v for k, v in os.environ.items() if k not in _GIT_LOCATION_ENV}
    if overrides:
        env.update(overrides)
    return env


def _git(args: list[str], repo_root: str | Path) -> str:
    out = subprocess.run(
        ["git", *args],
        cwd=str(repo_root),
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=15,
        env=git_env(),
    )
    return out.stdout.strip()


def git_info(repo_root: str | Path | None = None) -> dict:
    """Current commit sha + dirty flag + branch via ``git rev-parse`` /
    ``git status --porcelain`` (no network)."""
    root = Path(repo_root) if repo_root is not None else REPO_ROOT
    git_sha = _git(["rev-parse", "HEAD"], root)
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], root)
    dirty = bool(_git(["status", "--porcelain"], root))
    return {"git_sha": git_sha, "dirty": dirty, "branch": branch}


def _sha256_file(path: str | Path) -> str | None:
    p = Path(path)
    if not p.is_file():
        return None
    return hashlib.sha256(p.read_bytes()).hexdigest()


def env_info(
    repo_root: str | Path | None = None, *, key_deps: dict | None = None
) -> dict:
    """Python version, ``uv.lock`` sha256, gpu (None unless supplied), key deps."""
    root = Path(repo_root) if repo_root is not None else REPO_ROOT
    return {
        "python": platform.python_version(),
        "lockfile_sha256": _sha256_file(root / "uv.lock"),
        "gpu": None,
        "key_deps": dict(key_deps) if key_deps else {},
    }


def corpus_manifest_sha256(path: str | Path | None = None) -> str | None:
    """sha256 of the in-repo corpus manifest if present, else None (the
    corpus-snapshot pin). ``path=None`` resolves ``corpus_manifest_path()``."""
    if path is None:
        path = corpus_manifest_path()
    return _sha256_file(path)


# --- index + record writers -------------------------------------------------


def index_run_ids(index_path: str | Path | None = None) -> set[str]:
    """The set of ``run_id``s already present in ``index.jsonl`` (empty if the
    file is absent). Used to keep appends idempotent."""
    path = Path(index_path) if index_path is not None else INDEX_PATH
    ids: set[str] = set()
    if path.is_file():
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                rid = json.loads(line).get("run_id")
            except json.JSONDecodeError:
                continue
            if rid is not None:
                ids.add(rid)
    return ids


def append_index(record: dict, *, index_path: str | Path | None = None) -> None:
    """Append ONE JSON line to ``index.jsonl`` (append-only; never rewrites
    existing lines). Self-heals a truncated partial last line by writing a
    leading newline first."""
    path = Path(index_path) if index_path is not None else INDEX_PATH
    path.parent.mkdir(parents=True, exist_ok=True)
    line = json.dumps(record, sort_keys=True, ensure_ascii=False)
    prefix = ""
    if path.exists() and path.stat().st_size > 0:
        with path.open("rb") as fh:
            fh.seek(-1, os.SEEK_END)
            if fh.read(1) != b"\n":
                prefix = "\n"
    with path.open("a", encoding="utf-8") as fh:
        fh.write(prefix + line + "\n")


_MD_COLUMNS = [
    ("ts", "ts"),
    ("experiment", "experiment"),
    ("run_id", "run_id"),
    ("verdict", "verdict"),
    ("n", "n"),
    ("git_sha", "git_sha"),
    ("cost_usd", "cost_usd"),
    ("headline", "headline"),
    ("review", "review"),
]


def _md_cell(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, dict):
        return "; ".join(f"{k}={v}" for k, v in value.items())
    return str(value).replace("|", "\\|")


def regen_index_md(
    *,
    index_path: str | Path | None = None,
    md_path: str | Path | None = None,
) -> None:
    """Regenerate ``INDEX.md`` (a sortable Markdown table) FROM ``index.jsonl``
    (idempotent; never hand-edited)."""
    idx = Path(index_path) if index_path is not None else INDEX_PATH
    out = Path(md_path) if md_path is not None else INDEX_MD_PATH
    rows: list[dict] = []
    if idx.is_file():
        for line in idx.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    rows.sort(key=lambda r: (str(r.get("ts", "")), str(r.get("run_id", ""))))

    headers = [label for label, _ in _MD_COLUMNS]
    lines = [
        "# Experiment index",
        "",
        "GENERATED FROM `index.jsonl` — do NOT hand-edit. Regenerate with "
        "`experiments/_lib.regen_index_md()`.",
        "",
        "| " + " | ".join(headers) + " |",
        "| " + " | ".join("---" for _ in headers) + " |",
    ]
    for row in rows:
        cells = [_md_cell(row.get(key)) for _, key in _MD_COLUMNS]
        lines.append("| " + " | ".join(cells) + " |")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_record(
    experiment: str,
    *,
    ts: datetime,
    config_obj: Any,
    metrics: Any,
    verdict: str,
    read: str,
    code: dict,
    corpus: dict,
    seeds: dict,
    env: dict,
    cost_usd: float,
    title: str | None = None,
    headline: dict | None = None,
    n: int | None = None,
    config_path: str | None = None,
    tdd_evidence: dict | None = None,
    tests: Any = None,
    files_changed: list | None = None,
    artifacts: list | None = None,
    review: Any = None,
    open_questions: list | None = None,
    base_dir: str | Path | None = None,
    index_path: str | Path | None = None,
) -> tuple[str, Path]:
    """Compute the config hash + ``run_id``, write ``record.json`` +
    ``config.resolved.yaml`` + ``metrics.json`` under ``runs/<run_id>/``, and
    append the index line. Returns ``(run_id, run_dir)``.

    ``ts`` is supplied by the caller (a live runner passes ``datetime.now(UTC)``)
    so this stays pure/deterministic.
    """
    base = Path(base_dir) if base_dir is not None else EXPERIMENTS_DIR
    resolved = _resolved_dict(config_obj)
    sha = config_sha256(resolved)
    run_id = make_run_id(experiment, ts, sha)
    run_dir = base / "runs" / run_id

    record_dict = {
        "run_id": run_id,
        "experiment": experiment,
        "title": title if title is not None else experiment,
        "verdict": verdict,
        "read": read,
        "code": {
            "git_sha": code.get("git_sha"),
            "dirty": code.get("dirty"),
            "branch": code.get("branch"),
            "baseline_commit": code.get("baseline_commit"),
        },
        "config": {"sha256": sha, "path": config_path, "resolved": resolved},
        "corpus": {
            "source": corpus.get("source"),
            "manifest_sha256": corpus.get("manifest_sha256"),
            "datasets": list(corpus.get("datasets", [])),
        },
        "seeds": dict(seeds),
        "env": {
            "python": env.get("python"),
            "lockfile_sha256": env.get("lockfile_sha256"),
            "gpu": env.get("gpu"),
            "key_deps": dict(env.get("key_deps", {})),
        },
        "metrics": metrics,
        "cost_usd": cost_usd,
        "tdd_evidence": dict(tdd_evidence) if tdd_evidence else {},
        "tests": tests,
        "files_changed": list(files_changed) if files_changed else [],
        "artifacts": list(artifacts) if artifacts else [],
        "review": review,
        "open_questions": list(open_questions) if open_questions else [],
    }
    # Validate against the typed schema before writing (consumed-or-rejected).
    record = record_from_dict(record_dict)
    record_dict = asdict(record)

    run_dir.mkdir(parents=True, exist_ok=True)
    (run_dir / "record.json").write_text(
        json.dumps(record_dict, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    (run_dir / "config.resolved.yaml").write_text(
        _dump_yaml(resolved), encoding="utf-8"
    )
    (run_dir / "metrics.json").write_text(
        json.dumps(metrics, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    index_row = {
        "run_id": run_id,
        "repo": "fathomdb",
        "experiment": experiment,
        "ts": _ts_iso(ts),
        "git_sha": code.get("git_sha"),
        "dirty": code.get("dirty"),
        "config_sha256": sha,
        "corpus": record_dict["corpus"],
        "seeds": dict(seeds),
        "n": n,
        "headline": dict(headline) if headline else {},
        "cost_usd": cost_usd,
        "verdict": verdict,
        "review": review,
    }
    idx = (
        Path(index_path)
        if index_path is not None
        else (base / "index.jsonl" if base_dir is not None else INDEX_PATH)
    )
    # Idempotency guard: a re-write of the same run_id (identical config within
    # the same UTC minute) refreshes the run dir but must NOT double-append.
    if run_id not in index_run_ids(idx):
        append_index(index_row, index_path=idx)
    return run_id, run_dir


def _dump_yaml(obj: dict) -> str:
    if _HAVE_YAML:
        return yaml.safe_dump(obj, sort_keys=True, default_flow_style=False)
    return json.dumps(obj, indent=2, sort_keys=True) + "\n"
