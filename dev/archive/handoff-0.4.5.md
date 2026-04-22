# 0.4.5 Hand-off

## Status

**0.4.5 is complete and on main** (commit `5c89de7`). All 689 tests pass.
All 7 packs (A+G, D, B, C, E, F, H) merged. Version bumped to 0.4.5.

The plan file lives at: `/home/coreyt/.claude/plans/fathomdb-045.md`

---

## What 0.4.2 already delivers (do not re-implement)

- `fts_kind_table_name(kind)`, `fts_column_name(path, is_recursive)`,
  `resolve_fts_tokenizer(conn, kind)`, `DEFAULT_FTS_TOKENIZER`
  — exported from `crates/fathomdb-schema` and `crates/fathomdb-engine`
- `projection_profiles` table — schema v20, ships empty
- `create_or_replace_fts_kind_table(conn, kind, specs, tokenizer)`
  — already calls `resolve_fts_tokenizer` at registration time
- `FtsPropertyPathSpec::with_weight(f32)` + `#[non_exhaustive]`
- Per-kind `fts_props_<kind>` tables; `RebuildActor` + staging + atomic swap

**Phase 1 of the 0.4.5 roadmap is therefore already done.**

---

## What 0.4.5 needs to implement

### Pack A+G — Rust profile CRUD + tokenizer presets  *(no dependencies)*

**Files:** `crates/fathomdb-engine/src/admin.rs`, `crates/fathomdb-engine/src/lib.rs`

New structs (all `#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]`):
```rust
pub struct FtsProfile {
    pub kind: String,
    pub tokenizer: String,
    pub active_at: Option<i64>,
    pub created_at: i64,
}
pub struct VecProfile {
    pub model_identity: String,
    pub model_version: Option<String>,
    pub dimensions: u32,
    pub active_at: Option<i64>,
    pub created_at: i64,
}
pub struct ProjectionImpact {
    pub rows_to_rebuild: u64,
    pub estimated_seconds: u64,
    pub temp_db_size_bytes: u64,
    pub current_tokenizer: Option<String>,
    pub target_tokenizer: Option<String>,
}
```

Tokenizer presets constant and resolver:
```rust
pub const TOKENIZER_PRESETS: &[(&str, &str)] = &[
    ("recall-optimized-english", "porter unicode61 remove_diacritics 2"),
    ("precision-optimized",      "unicode61 remove_diacritics 2"),
    ("global-cjk",               "icu"),
    ("substring-trigram",        "trigram"),
    ("source-code",              "unicode61 tokenchars '._-$@'"),
];
pub fn resolve_tokenizer_preset(input: &str) -> &str { ... }
```

New `AdminService` methods:
- `set_fts_profile(kind, tokenizer_str) -> Result<FtsProfile>` — resolves preset, validates chars (same allowlist as `create_or_replace_fts_kind_table`), UPSERTs `projection_profiles`
- `get_fts_profile(kind) -> Result<Option<FtsProfile>>`
- `set_vec_profile_inner(conn, identity) -> Result<VecProfile, rusqlite::Error>` — **private**, wired into `regenerate_vector_embeddings` after `tx.commit()?` as best-effort
- `get_vec_profile() -> Result<Option<VecProfile>>`
- `preview_projection_impact(kind, facet) -> Result<ProjectionImpact>`
  - FTS: `SELECT count(*) FROM nodes WHERE kind=?1 AND superseded_at IS NULL`; `estimated_seconds = rows/5000`; `temp_db_size_bytes = rows*200`
  - Vec: `SELECT count(*) FROM chunks`; `estimated_seconds = rows/100`; `temp_db_size_bytes = rows*1536`

Re-export `FtsProfile`, `VecProfile`, `ProjectionImpact` from `lib.rs`.

**UPSERT SQL:**
```sql
INSERT INTO projection_profiles (kind, facet, config_json, active_at, created_at)
VALUES (?1, 'fts', json_object('tokenizer', ?2), unixepoch(), unixepoch())
ON CONFLICT(kind, facet) DO UPDATE SET
    config_json = json_object('tokenizer', ?2),
    active_at   = unixepoch()
```

**Vec profile JSON shape** stored in `projection_profiles(kind='*', facet='vec')`:
```json
{"model_identity":"bge-small-en-v1.5","model_version":"1.5","dimensions":384,"normalization_policy":"l2"}
```

**TDD tests (10):** set+get roundtrip, upsert, invalid tokenizer rejected, preset resolution (all 5), preview FTS row count, preview populates current_tokenizer, preview vec count.

---

### Pack D — Python embedding adapters  *(no dependencies)*

**New files:**
```
python/fathomdb/embedders/__init__.py
python/fathomdb/embedders/_base.py      # EmbedderIdentity dataclass, QueryEmbedder ABC
python/fathomdb/embedders/_openai.py    # OpenAIEmbedder (httpx, 300s TTL cache, 512 entries)
python/fathomdb/embedders/_jina.py      # JinaEmbedder (768d, jina-embeddings-v2-base-en)
python/fathomdb/embedders/_stella.py    # StellaEmbedder (Matryoshka, lazy sentence-transformers, L2-norm after truncation)
python/fathomdb/embedders/_subprocess.py # SubprocessEmbedder (persistent Popen, binary f32 LE)
python/tests/test_embedders.py
```

`SubprocessEmbedder` protocol: write UTF-8 line (text + `\n`) to stdin; read `dimensions * 4` bytes from stdout as `struct.unpack(f"{dimensions}f", ...)`.

Add to `python/pyproject.toml`:
```toml
[project.optional-dependencies]
openai = ["httpx>=0.25"]
jina   = ["httpx>=0.25"]
stella = ["sentence-transformers>=2.7"]
embedders = ["httpx>=0.25", "sentence-transformers>=2.7"]
```

**TDD tests (10):** identity fields, mocked HTTP (OpenAI + Jina), cache hit, Stella truncation + L2-norm, subprocess echo, import without optional deps.

---

### Pack B — Rust FFI exposure  *(depends on A+G)*

**Files:** `crates/fathomdb/src/admin_ffi.rs`, `crates/fathomdb/src/python.rs`

New functions in `admin_ffi.rs`:
```rust
pub fn set_fts_profile_json(engine: &Engine, request_json: &str) -> Result<String, AdminFfiError>
// request: {"kind":"K","tokenizer":"T"}  →  serialized FtsProfile

pub fn get_fts_profile_json(engine: &Engine, kind: &str) -> Result<String, AdminFfiError>
// response: serialized FtsProfile or "null"

pub fn set_vec_profile_json(engine: &Engine, request_json: &str) -> Result<String, AdminFfiError>
// request: {"model_identity":"...","model_version":"...","dimensions":384,"normalization_policy":"l2"}

pub fn get_vec_profile_json(engine: &Engine) -> Result<String, AdminFfiError>
// response: serialized VecProfile or "null"

pub fn preview_projection_impact_json(engine: &Engine, kind: &str, facet: &str) -> Result<String, AdminFfiError>
// response: serialized ProjectionImpact
```

New PyO3 methods on `EngineCore` (all via `py.allow_threads`):
```rust
fn set_fts_profile(&self, py: Python<'_>, request_json: &str) -> PyResult<String>
fn get_fts_profile(&self, py: Python<'_>, kind: &str) -> PyResult<String>
fn set_vec_profile(&self, py: Python<'_>, request_json: &str) -> PyResult<String>
fn get_vec_profile(&self, py: Python<'_>) -> PyResult<String>
fn preview_projection_impact(&self, py: Python<'_>, kind: &str, facet: &str) -> PyResult<String>
```

**TDD tests (5):** set+get FTS roundtrip, get returns null when unset, preview FTS count, set+get vec roundtrip.

---

### Pack C — Python types + AdminClient methods  *(depends on B)*

**Files:** `python/fathomdb/_types.py`, `python/fathomdb/_admin.py`, `python/fathomdb/errors.py`, `python/fathomdb/__init__.py`, `python/tests/test_profile_management.py`

New types:
```python
@dataclass(frozen=True)
class FtsProfile:
    kind: str; tokenizer: str; active_at: int | None; created_at: int
    @classmethod def from_wire(cls, d): ...

@dataclass(frozen=True)
class VecProfile:
    model_identity: str; model_version: str | None; dimensions: int
    active_at: int | None; created_at: int
    @classmethod def from_wire(cls, d): ...

@dataclass(frozen=True)
class ImpactReport:
    rows_to_rebuild: int; estimated_seconds: int; temp_db_size_bytes: int
    current_tokenizer: str | None; target_tokenizer: str | None
    @classmethod def from_wire(cls, d): ...

class RebuildMode(str, Enum):
    SYNC = "sync"; ASYNC = "async"

class RebuildImpactError(Exception):
    def __init__(self, report: ImpactReport): self.report = report; super().__init__(...)
```

TOKENIZER_PRESETS dict in `_admin.py` (mirrors Rust constant).

New `AdminClient` methods:
```python
def configure_fts(self, kind, tokenizer, mode=RebuildMode.ASYNC, *, agree_to_rebuild_impact=False) -> FtsProfile
# 1. resolve preset  2. preview_projection_impact  3. safety gate (raise RebuildImpactError if rows>0 and not agree)
# 4. set_fts_profile FFI  5. re-register schema with existing entries (triggers rebuild)  6. return FtsProfile

def configure_vec(self, embedder, mode=RebuildMode.ASYNC, *, agree_to_rebuild_impact=False) -> VecProfile
# 1. embedder.identity()  2. preview impact  3. safety gate
# 4. regenerate_vector_embeddings  5. return get_vec_profile()

def preview_projection_impact(self, kind, target: Literal["fts","vec"]) -> ImpactReport
def get_fts_profile(self, kind) -> FtsProfile | None
def get_vec_profile(self) -> VecProfile | None
```

Export `FtsProfile`, `VecProfile`, `ImpactReport`, `RebuildMode`, `RebuildImpactError` from `__init__.py`.

**TDD tests (10):** get returns None pre-configure, RebuildImpactError raised on rows>0, proceeds with agree flag, profile roundtrip, preset name resolution, async mode returns fast.

---

### Pack E — Vec identity lifecycle guard  *(depends on A+G)*

**File:** `crates/fathomdb-engine/src/coordinator.rs`

In `ExecutionCoordinator::open`, after bootstrap, if embedder is present:
```rust
check_vec_identity_at_open(&conn, embedder.as_ref())?;
```

New function (never returns Err):
```rust
fn check_vec_identity_at_open(conn: &Connection, embedder: &dyn QueryEmbedder) -> Result<(), EngineError> {
    // query projection_profiles WHERE kind='*' AND facet='vec'
    // if row: parse model_identity + dimensions from config_json
    // if dimensions differ: tracing::warn!(...)
    // if model_identity differs: tracing::warn!(...)
    Ok(())
}
```

**TDD tests (4):** no profile → no panic, matching identity → Ok, mismatched dimension → Ok (just warns), mismatched model → Ok.

---

### Pack F — Admin CLI  *(depends on B + C)*

**New file:** `python/fathomdb/_cli.py`

**pyproject.toml additions:**
```toml
[project.scripts]
fathomdb = "fathomdb._cli:main"

[project.optional-dependencies]
cli = ["click>=8.1"]
```

CLI structure:
```
fathomdb admin configure-fts --db PATH --kind KIND --tokenizer TOK [--agree-to-rebuild-impact]
fathomdb admin configure-vec --db PATH --embedder PRESET [--agree-to-rebuild-impact]
fathomdb admin preview-impact --db PATH --kind KIND --target {fts,vec}
fathomdb admin get-fts-profile --db PATH --kind KIND
fathomdb admin get-vec-profile --db PATH
```

`configure-fts` flow: open engine → preview impact → if rows>0 and no flag: interactive [y/N] (CI: abort) → configure_fts(agree_to_rebuild_impact=True).

**TDD tests (7 via click.testing.CliRunner):** abort without flag when rows>0, succeed with flag, no-prompt on zero rows, preview prints report, get-fts-profile no-profile message, preset name accepted.

---

### Pack H — Query-side tokenizer adaptations  *(depends on A+G)*

**Files:** `crates/fathomdb-engine/src/coordinator.rs`

At `open`: load `projection_profiles WHERE facet='fts'` into `HashMap<String, TokenizerStrategy>`.

`TokenizerStrategy` enum: `RecallOptimizedEnglish`, `PrecisionOptimized`, `SubstringTrigram`, `GlobalCjk`, `SourceCode`, `Custom(String)`.

At query time:
- `SubstringTrigram`: if query < 3 chars, skip FTS branch (return empty, not error)
- `SourceCode`: escape `.`, `-`, `_`, `$`, `@` in query tokens before FTS5 dispatch

**TDD tests (4):** strategy-from-string, trigram short-query returns empty, source-code dot escaping, custom passthrough.

---

## Orchestration protocol

Follow `dev/notes/agent-harness-runbook.md` in full. Key rules:
- `./scripts/preflight.sh` before every agent launch
- Max 3 concurrent worktrees; merge immediately after review
- Between-steps checklist (Section 7) after every merge
- TDD mandate: red-green-refactor, always commit failing test first

**Phase 1:** Launch Pack A+G and Pack D in parallel (disjoint files)
**Phase 2:** After A+G merges → launch Pack B
**Phase 3:** After B merges → launch Pack C; After A+G merges → also launch Pack E in parallel with C
**Phase 4:** After C merges → launch Pack F; after A+G merges → launch Pack H in parallel
**Final:** Version bump `0.4.1` → `0.4.5` in pyproject.toml + Cargo.toml files; changelog entry

---

## Resumption prompt

> Continue implementing FathomDB 0.4.5 per the plan at `/home/coreyt/.claude/plans/fathomdb-045.md`
> and the hand-off notes at `dev/notes/handoff-0.4.5.md`.
>
> State: 0.4.2 is on main (commit dcddfab, 666 tests passing). 0.4.5 has not started.
> No worktrees are open.
>
> Begin with Phase 1: launch Pack A+G (Rust profile CRUD) and Pack D (Python embedding
> adapters) as parallel worktree agents. Run preflight first, then the permission canary
> (Section 3 of the runbook), then the implementation canary. Follow the orchestrator
> runbook at `dev/notes/agent-harness-runbook.md` throughout.
