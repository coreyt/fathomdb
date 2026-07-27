#!/usr/bin/env bash
# check-c1-conformance.sh — the RUBRIC-H7 `can-i-deploy` contract-conformance
# gate (R-20-H7, plan-0.8.20.md §3).
#
# Shared by two callers, exactly like its siblings scripts/check-ledgers.sh,
# scripts/check-board-currency.sh and scripts/check-governed-surface-pin.sh:
#   * scripts/preflight.sh --landing              (PREVENT, land-time gate)
#   * .github/workflows/ci.yml c1-contract-conformance job
#                                                 (DETECT, always-on backstop)
# Reuse, not reimplementation: both callers invoke THIS script so the predicate
# cannot diverge between the two homes.
#
# WHAT THIS ENFORCES
#   R-20-H7: "a Pact-style can-i-deploy mechanical contract-conformance check:
#   as-built code still satisfies the ratified
#   dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md at the
#   co-land. NOT humans re-reading prose." Acceptance: "Gate exists and is GREEN.
#   An absent-or-failing gate HOLDS the breaking pair." It is a PUBLISH
#   PRECONDITION for the 0.8.20 ↔ Memex 0.5.x-successor breaking pair.
#
#   Every clause of that contract was read and classified exactly once into
#   CHECKABLE / CROSS-REPO / PROSE, and the whole classification is recorded, in
#   reviewable form, in scripts/c1-conformance-pin.json. This gate then does two
#   different jobs, and the difference matters:
#     1. it pins the CONTRACT'S BYTES, so the document cannot move underneath a
#        registry derived from it; and
#     2. it evaluates the CHECKABLE clauses against as-built code.
#
# ================= WHAT THIS GATE DOES *NOT* DO (read this) ==================
# This is a STATIC STRUCTURAL conformance check. It asserts:
#   * the PRESENCE of the symbols, tables, error variants and wiring the
#     contract names;
#   * the ABSENCE of the negative space the contract names (its Q6(b) amendment
#     asserts, as a matter of fact, that no `EntityTypeSpec` and no `id_prefix`
#     exists anywhere under src/ — that is a perfect falsifiable assertion and
#     it is checked as one);
#   * for CLOSED VOCABULARIES ("EXACTLY {ready, embedding}", "exactly three
#     roles", "total over exactly those three"), that the enum's variant SET and
#     COUNT and its string mappings are exactly the pinned ones — a structural
#     comparison, NOT a blacklist of the names somebody happened to fear; and
#   * for BEHAVIOURAL obligations, that THE NAMED TEST WHICH PROVES THAT
#     BEHAVIOUR STILL EXISTS IN THE TREE.
#
# It does NOT re-execute those behavioural suites, and it therefore does NOT
# prove the behaviour is still correct — only that the proof has not been
# deleted or renamed away. `cargo test --workspace` is what proves the
# behaviour; this gate is what notices when the proof disappears or when the
# contract's structural obligations are no longer met.
#
# So: it CATCHES a symbol/table/variant that was renamed or removed, a
# forbidden symbol that reappeared, a named test that was deleted, and any edit
# to the ratified contract. It DOES NOT catch a behavioural regression inside a
# still-present, still-named test, nor anything the Memex repo does. Claiming
# otherwise would be a false assurance — which is the TC-37 failure class in a
# different costume.
#
# ===================== PROBE SCOPE — WHY THE ASYMMETRY =======================
# fix-3, codex §9 round 3 findings #1 and #2 [P2]. Every NEGATIVE probe used to
# read ONE FILE — a crate's lib.rs. So a LITERAL, correctly-spelled, fully
# schema-qualified `CREATE VIRTUAL TABLE main.property_search_index ...` placed in
# a NEW module beside lib.rs (codex's `fathomdb-engine/src/extra_fts.rs`) exited
# 0, and so did an `INSERT INTO canonical_attributes ...` in a new schema-crate
# module. That is not the residual scope below and not a regex-shape problem: it
# is exactly what this gate claims to catch, in ordinary source text, missed
# because the check was pointed at the wrong subject. The engine crate ALREADY
# carries sibling modules (lifecycle.rs, pcache2.rs); "somebody adds a module" is
# how a crate grows, not an evasion.
#
# THE RULE, AND WHY IT IS NOT SYMMETRIC:
#
#   * NEGATIVE probes (`absent_tree`) are TREE-scoped, always. A negative
#     assertion scoped to one file is only as strong as the assumption that
#     nobody adds a module, and it fails SILENTLY GREEN when that assumption
#     breaks. There is no file-scoped negative probe kind in this script — the
#     `("absent", <file>, regex)` kind was DELETED, not left unused, so writing
#     one again is a deliberate, reviewed act rather than the path of least
#     effort. scripts/tests/test_check_c1_conformance.sh asserts its absence.
#
#   * POSITIVE probes (`present`, `doc_text`) and the STRUCTURAL readers
#     (`in_item`, `fn_sig`, `fn_defined`, `sql_ddl`, `fts_tokenizer_shared`,
#     `enum_exact`, `arms_exact`) legitimately stay FILE-scoped. Their failure
#     mode is the opposite one: a too-narrow positive scope cannot produce a
#     false green, it produces a false RED by construction. If the symbol, the
#     enum or the impl block MOVES to a sibling module, the probe finds 0 matches
#     (or "could not locate a parseable enum") and the gate fails loudly, which
#     is the safe side. Pinning them to a named file also states WHERE the
#     contract's structure is expected to live, which is information a tree-wide
#     search would throw away.
#
#   * A CRATE TREE IS THE COMPLETE SCOPE, not merely a wider one, for the Rust
#     identifier negatives: a variant cannot be added to `enum DenseReadiness` or
#     `enum ProjectionRole` from outside the crate that declares them.
#
# CURRENT INVENTORY (7 negative probes, all tree-scoped; 5 clauses):
#   src                                     Q4-NO-PROVISIONAL (*.rs);
#                                           Q6B-NO-ENTITYTYPESPEC ×3 (all text)
#   fathomdb-schema/src                     Q2-NO-DATA-MIGRATION ×2 (all text)
#   fathomdb-engine/src                     Q4-DENSE-READINESS ×1 (*.rs),
#                                           Q6A-THREE-ROLES ×2 (*.rs),
#                                           Q6B-ID-NON-NULL ×1 (*.rs),
#                                           TE-CUSTOM-TOKENIZER ×1 (all text)
# The SQL negatives use exts=None rather than *.rs on purpose: a migration or a
# DDL carried in a `.sql` file and pulled in with `include_str!` is the same
# violation. The Rust IDENTIFIER negatives use *.rs, because their subject is
# Rust syntax and a mention in a neighbouring note would be a pure false RED.
#
# ==================== RESIDUAL SCOPE — READ BEFORE ROUND 5 ===================
# This gate is a STATIC, LEXICAL check over Rust and SQL SOURCE TEXT. That
# approach has a boundary, and two review rounds were spent tightening regexes
# against it (fix-1: SQL case/whitespace, and blacklist ⇒ closed vocabulary;
# fix-2: schema-qualified table names, and spellings admitted outside a simple
# match arm). Rather than rediscover the boundary again, it is written down here.
#
# NOTE THAT FIX-3 AND FIX-4 ARE NOT ON THIS LIST AND DID NOT BELONG ON IT. Round
# 3 was a bounded SCOPE bug (one file vs the crate tree) and round 4 a bounded
# SUBJECT-BINDING bug (a file vs the named table/struct/enum/function the clause
# is about). Both had definite, complete fixes and both were CLOSED, not
# documented away. The list below is the set of things a different MECHANISM
# would be needed for; it is not a place to retire findings to.
#
# WHAT THIS GATE IS. A TRIPWIRE AGAINST DRIFT. Its threat model is a FUTURE
# CONTRIBUTOR who changes as-built code without noticing that a ratified
# cross-repo contract said otherwise — the ordinary way a `can-i-deploy`
# precondition rots. It is NOT a proof of conformance and NOT an adversarial
# control: it cannot stop someone deliberately smuggling a backfill past CI, and
# nothing assembled out of regexes over source text could.
#
# THE CLASSES IT DOES NOT AND CANNOT CATCH, concretely:
#
#   1. DYNAMICALLY COMPOSED SQL. The probes read literal source text. A
#      `format!("INSERT INTO {} SELECT ...", TABLE)`, a `push_str` built from
#      fragments, or a table name held in a `const`/`static` all reach the
#      forbidden table while the text this gate reads contains only `{}`.
#
#   2. ANOTHER ROUTE TO THE SAME TABLE. `qualified()` closes the SPELLINGS of
#      the PINNED NAME (`main.canonical_attributes`, `` `main` . x ``, `[main].x`,
#      any case, any whitespace). It does not close every ROUTE: an ATTACHed
#      database whose alias is chosen at runtime, a VIEW or an `INSTEAD OF`
#      TRIGGER that writes the table under a different name, or an
#      `ALTER TABLE ... RENAME` are all invisible to it.
#
#   3. IDENTIFIER INDIRECTION IN RUST. `const PENDING: &str = "pending";` used
#      in place of a literal defeats the string-literal vocabulary check;
#      `macro_rules!` / `paste!`-generated enum bodies and impl blocks are never
#      expanded; `concat_idents!`-style construction defeats the `EntityTypeSpec`
#      / `id_prefix` negative-space probes; a type alias or a re-export under
#      another name defeats the identifier probes.
#
#   4. CONVERSION SURFACES OUTSIDE THE NAMED FUNCTION. `arms_exact` reads exactly
#      `impl <Ty> { fn <fn> }`. An `impl FromStr for <Ty>`, a `From<&str>`, or a
#      free `fn parse_readiness(&str) -> Option<DenseReadiness>` can admit an
#      extra spelling without touching the block this gate reads. This is the
#      clearest proof the approach cannot be COMPLETED by widening: a free
#      function is ordinary Rust, and no impl-scoped check can reach it.
#
#   5. NORMALISING COMPARISONS. `value.to_lowercase()`, `.trim()`,
#      `eq_ignore_ascii_case` widen the accepted vocabulary WITHOUT introducing a
#      new string literal, so the vocabulary check has nothing to see.
#
#   6. CONDITIONAL COMPILATION. Nothing here models `#[cfg(...)]`: text under a
#      disabled feature reads exactly like shipped code, and a violation
#      introduced only behind a feature flag reads exactly like one that is
#      always on. (This class errs RED, not green.)
#
#   7. BEHAVIOUR, and THE MEMEX REPO. Stated above: the behavioural clauses
#      assert only that the NAMED TEST still exists, and 12 contract clauses are
#      CROSS-REPO by classification and unverifiable from this repo at all.
#
#   8. WHAT A NAMED TEST ACTUALLY ASSERTS. The strongest form of #7, and the one
#      worth naming separately after fix-4: `fn_defined` proves the named test
#      still EXISTS as a definition with a body. It cannot prove the body still
#      asserts anything — `fn atomic_flip_never_exposes_ready_without_the_vector
#      _under_concurrent_write() {}` (an empty body) satisfies it, and so does one
#      whose assertions were commented out. That is not a hole a lexical check can
#      close, and it is not one worth chasing here: `cargo test --workspace` is
#      the mechanism that executes those bodies, it is a required gate, and an
#      emptied test is visible in review as a code diff. See the RECOMMENDATION in
#      the closure JSON for the mechanism that WOULD close it.
#
# WHICH WAY THE ERRORS FALL. Every ambiguity is resolved toward RED: an
# unparseable enum body, an impl block the reader cannot find, an unreadable file
# and a missing tree are all failures (exit 1 / exit 2), never skips; and the
# text-scoped probes WILL trip on a doc comment or an error message that happens
# to spell the forbidden thing. Noisy false positives, and false negatives
# confined to the list above, is the correct bias for a gate that HOLDS A
# PUBLISH.
#
# WHY THIS IS THE RIGHT STOPPING POINT. A stronger mechanism exists and is
# deliberately NOT built here: asserting the shipped schema against
# `sqlite_master` at runtime (which sees the table however the SQL was spelled or
# composed), and/or parsing Rust with `syn` (which sees every conversion surface,
# macro-expanded). Both are real work with their own maintenance surface, both
# belong to a slice of their own, and neither is warranted by the threat model
# above. Recorded as a RECOMMENDATION in
# dev/plans/runs/0.8.20-slice-30-output.json, not as a defect being deferred.
#
# CONSEQUENCE FOR REVIEWERS. A finding that this gate misses something ON THE
# LIST ABOVE is TRUE and is NOT a defect in the patch — it is the documented
# scope, and the answer to it is a different mechanism, not another regex. A
# finding that it misses a spelling of a PINNED NAME in LITERAL source text, that
# it misses it because the probe was pointed at the wrong SCOPE, or that a probe
# is satisfied by something OTHER THAN THE SUBJECT ITS CLAUSE NAMES, is a real
# defect: fix it, and if the fix is another regex, add its class here. Round 3
# was the second kind (see PROBE SCOPE above) and round 4 the third (see A PROBE
# IS BOUND TO ITS SUBJECT, beside the assertion table); both were fixed.
#
# ============================ THE AMENDMENT TRAP =============================
# The contract was AMENDED at efa8d584 ("amend C-1 contract Q6(b) with the TC-11
# cancellation — unblocks the H7 gate"). Before that amendment, clause Q6(b)
# mandated minting an anonymous surrogate `logical_id` — behaviour the ratified
# TC-11 pin A FORBIDS ever implementing, and which as-built code does not
# implement. A gate authored from the un-amended text would have been
# PERMANENTLY RED and would have held the breaking-pair publish forever.
#
# That is precisely why this gate pins the contract's BYTES. The registry below
# describes ONE exact revision of the document. If the document moves, the
# honest answer is not "recompute the hash" — it is RE-DERIVE the clause
# registry from the new text, re-classify every clause, and re-pin under review.
#
# PREDICATE — all four must hold:
#   (a) CONTRACT PIN. sha256 AND git blob sha1 of the contract file's raw bytes
#       equal the pin. Both forms are recorded so a reviewer can reproduce the
#       pin either way (`git rev-parse <commit>:<path>` gives the blob sha1
#       directly). Divergence ⇒ exit 1.
#   (b) CLAUSE-REGISTRY INTEGRITY. The pin records every clause id with its
#       category. This script's implemented assertions and the pin's CHECKABLE
#       set must match IN BOTH DIRECTIONS: a pin id with no implemented
#       assertion (an ORPHAN — the pin over-states what is verified) and an
#       implemented assertion with no pin id (UNREGISTERED — the pinned check
#       set has shrunk) are both a MALFORMED PIN, exit 2.
#   (c) PINNED COUNTS. Per-category counts AND the grand total are pinned and
#       asserted SEPARATELY from the member lists, so an internally inconsistent
#       (botched) re-pin is caught rather than trusted. EVERY count must be
#       PRESENT and an integer: a missing or mistyped count is a MALFORMED PIN
#       (exit 2), never a skipped check. This is the exact hole
#       check-governed-surface-pin.sh had to close — an absent count read back
#       as None and silently disabled its own check, so a re-pin that DELETED a
#       count could buy a green. bool is excluded explicitly because
#       isinstance(True, int) is True in Python.
#   (d) CLAUSE ASSERTIONS. Each CHECKABLE clause's assertion is evaluated
#       against as-built code under --root. Any failure ⇒ exit 1, naming the
#       clause id, quoting the contract obligation, and citing the evidence.
#
# ============ VACUOUS-PASS GUARD — TC-37, five evaporation paths =============
# A gate that cannot see its subject must NEVER report green. ALL FIVE of these
# exit 2, never 0 and never 1:
#   1. python3 is absent.
#   2. the contract file is missing / unreadable.
#   3. the pin file is missing / unreadable / unparseable / MALFORMED.
#   4. a source file a clause's assertion must read is missing or unreadable, OR
#      a source TREE it must scan is absent, OR that tree yields ZERO candidate
#      files (fix-3: a negative assertion that examined nothing has not been
#      evaluated — reporting it satisfied is the vacuous pass in its purest
#      form). In every case the assertion could not be EVALUATED, which is
#      neither a pass nor a clause failure.
#   5. the executed-assertion count is less than the pinned CHECKABLE count —
#      the check set evaporated.
#
# Exit codes: 0 = as-built code conforms to the pinned contract;
#             1 = a REAL DIVERGENCE — the contract moved, or a clause failed.
#                 Route to the Steward / HITL;
#             2 = the gate could not run, or its pin is untrustworthy. This is
#                 deliberately NOT exit 1: exit 1 says "conformance broke, take
#                 it to the Steward", whereas exit 2 says "this gate has no
#                 trustworthy verdict at all".
#
# Usage:
#   scripts/check-c1-conformance.sh [--contract <path>] [--pin <path>]
#                                   [--root <dir>] [--list-sources] [--help]
#
# --contract/--pin/--root exist so the test fixtures can point at COPIES under
# mktemp -d; the real contract and the real src/ tree are NEVER written by the
# tests (mutating them is the exact thing this gate exists to catch). Both
# callers invoke the script with no arguments. --list-sources prints every
# root-relative path the assertions read, so the fixture roots are built from
# the gate's own manifest and cannot silently go stale.
#
# Requires python3. If it is absent this script exits 2 (env error) LOUDLY
# rather than skipping — a skip here would be the TC-37 hole.
set -euo pipefail

SELF="$(basename "${BASH_SOURCE[0]}")"

usage() {
  cat <<EOF
Usage: scripts/$SELF [--contract <path>] [--pin <path>] [--root <dir>]

The RUBRIC-H7 \`can-i-deploy\` gate (R-20-H7): fails when as-built code no longer
satisfies the ratified OPP-12 C-1 converged contract, or when that contract has
moved relative to the clause registry pinned in scripts/c1-conformance-pin.json.
See the header of this script for the full predicate, for what this gate does
NOT check, and for why re-pinning without re-deriving the registry is forbidden.

  --contract <path>  the ratified contract to check against
                     (default: dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md)
  --pin <path>       the clause-registry pin
                     (default: scripts/c1-conformance-pin.json)
  --root <dir>       the source tree the clause assertions read
                     (default: the repository root)
  --list-sources     print every path the assertions read, as
                     "file<TAB>path" / "tree<TAB>path" lines, and exit
  --help             show this text

Exit codes: 0 = conforms; 1 = divergence (contract moved, or a clause failed);
            2 = the gate could not run, or the pin is untrustworthy.
EOF
}

CONTRACT="dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md"
PIN="scripts/c1-conformance-pin.json"
ROOT=""
LIST_SOURCES=0

while [ $# -gt 0 ]; do
  case "$1" in
    --contract)     CONTRACT="${2:?--contract needs a path}"; shift 2 ;;
    --pin)          PIN="${2:?--pin needs a path}"; shift 2 ;;
    --root)         ROOT="${2:?--root needs a directory}"; shift 2 ;;
    --list-sources) LIST_SOURCES=1; shift ;;
    -h|--help)      usage; exit 0 ;;
    *) printf '%s: unknown arg %q\n' "${SELF%.sh}" "$1" >&2; usage >&2; exit 2 ;;
  esac
done

# Both callers run from anywhere in the repo; defaults are repo-relative. An
# absolute --contract/--pin/--root (what the fixtures pass) is unaffected by the
# cd.
if TOPLEVEL="$(git rev-parse --show-toplevel 2>/dev/null)"; then
  cd "$TOPLEVEL"
fi
ROOT="${ROOT:-${TOPLEVEL:-.}}"

if ! command -v python3 >/dev/null 2>&1; then
  printf 'check-c1-conformance: python3 is required to parse the pin, hash the contract and evaluate the clause assertions, and is not on PATH — refusing to report a pass it did not verify (TC-37 evaporation path #1)\n' >&2
  exit 2
fi

set +e
python3 - "$CONTRACT" "$PIN" "$ROOT" "$LIST_SOURCES" >&2 <<'PY'
import hashlib
import json
import os
import re
import sys

CONTRACT, PIN, ROOT, LIST_SOURCES = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4] == "1"

# ---------------------------------------------------------------------------
# Root-relative paths the assertions read. Everything the gate opens is named
# here, and --list-sources prints exactly this, so a fixture root can be built
# from the gate's own manifest rather than from a hand-maintained copy that
# silently rots (a stale fixture root would turn every source arm into a TC-37
# path-#4 evaporation and stop testing what it claims).
# ---------------------------------------------------------------------------
ENG = "src/rust/crates/fathomdb-engine/src/lib.rs"
SCH = "src/rust/crates/fathomdb-schema/src/lib.rs"
EMB = "src/rust/crates/fathomdb-embedder/src/candle_bge.rs"
T15 = "src/rust/crates/fathomdb-engine/tests/slice15d_projection_registry.rs"
T20 = "src/rust/crates/fathomdb-engine/tests/slice20_dense_readiness.rs"
T25 = "src/rust/crates/fathomdb-engine/tests/slice25_registration_identity_inert.rs"
PLAN = "dev/plans/plan-0.8.20.md"
SRC_TREE = "src"
# The CRATE SOURCE TREES the negative probes scan (fix-3). ENG and SCH above name
# the crates' lib.rs; these name the whole module tree each lib.rs is the root of,
# because that — not one file — is the scope in which a crate's obligations hold.
ENG_TREE = "src/rust/crates/fathomdb-engine/src"
SCH_TREE = "src/rust/crates/fathomdb-schema/src"

# Directories never worth walking (and, for node_modules/target, never OURS).
SKIP_DIRS = {
    ".git", "node_modules", "target", "__pycache__", ".venv", "venv",
    "dist", "build", ".pytest_cache", ".mypy_cache", ".ruff_cache",
}

# ---------------------------------------------------------------------------
# THE IMPLEMENTED ASSERTIONS. One entry per CHECKABLE clause id in the pin; the
# bijection between this dict's keys and the pin's CHECKABLE ids is predicate
# (b) and is enforced in BOTH directions below.
#
# Probe kinds. STRUCTURAL kinds extract THE NAMED SUBJECT and assert inside it;
# the two text kinds are for subjects whose name IS the text.
#   ("in_item", path, kw, name, regex) regex must match inside the brace body of
#                                      the Rust item `<kw> <name>` (kw ∈ struct /
#                                      enum / impl / fn) — the field, variant,
#                                      arm or statement is asserted OF THAT ITEM
#   ("fn_sig", path, fn, regex)        regex must match inside the SIGNATURE of
#                                      `fn fn` (params + return type)
#   ("fn_defined", path, fn)           `fn fn` exists as a DEFINITION with a
#                                      parseable body (not a comment, not a call)
#   ("sql_ddl", path, table, regex)    regex must match inside the parenthesised
#                                      body of every `CREATE [VIRTUAL] TABLE
#                                      [<schema>.]<table> ( ... )` in path, and
#                                      there must be at least one
#   ("fts_tokenizer_shared", path, subject, refs, pinned)
#                                      the `tokenize=` clause of fts5 table
#                                      `subject` equals that of every table in
#                                      `refs`, and equals the pinned default
#   ("enum_exact", path, ty, members)  the Rust `enum ty` in path has EXACTLY
#                                      these variants — set AND count
#   ("arms_exact", path, ty, fn, pairs) the match arms inside `impl ty { fn fn }`
#                                      map EXACTLY these (variant, string) pairs
#   ("absent_tree", tree, regex, exts) regex must not match in any text file
#                                      under tree (exts=None ⇒ every text file)
#   ("present", path, regex)           regex must match at least once in path —
#                                      ONLY where the regex names its subject
#                                      uniquely (a declaration by name, a const
#                                      and its value, a whole table row). In a
#                                      `.rs` file it reads the COMMENT-STRIPPED
#                                      text: a commented-out declaration is not a
#                                      declaration (fix-4c)
#   ("doc_text", path, regex)          the same predicate over RAW text, and
#                                      declaring what it is: a DOCUMENTED
#                                      STATEMENT is still in the file — its
#                                      subject IS a comment. Never the
#                                      load-bearing half of a clause; see the
#                                      fix-4 note below.
#
# THERE IS NO FILE-SCOPED NEGATIVE PROBE KIND, DELIBERATELY (fix-3). See "PROBE
# SCOPE" in this file's header: every NEGATIVE assertion is `absent_tree`, and
# `("absent", <file>, regex)` was DELETED rather than merely left unused, so
# re-introducing a single-file negative is a deliberate, reviewed act.
#
# AND THERE IS NO COUNT PROBE KIND, DELIBERATELY (fix-4). `("min", path, regex,
# n)` was DELETED for the same reason: a count of file-wide matches is the
# weakest possible relationship between a probe and its subject. See "SUBJECT
# BINDING" in this file's header. scripts/tests/test_check_c1_conformance.sh
# asserts the absence of both kinds.
#
# ------------ CLOSED VOCABULARIES ARE STRUCTURAL, NOT BLACKLISTED ------------
# fix-1, codex §9 round 1 finding #1 [P2]. Three clauses below assert that a
# vocabulary is EXACTLY some set: readiness is "EXACTLY {ready, embedding}",
# roles are "exactly {filterable, rankable, searchable}", the typed id space is
# "total over exactly those three". Those obligations were originally probed as
# "the members I want are PRESENT" plus, in one case, "one specific bad name is
# ABSENT" — a BLACKLIST OF ONE. A blacklist is not a closed vocabulary: any
# member added under a name nobody thought to forbid walks straight through, and
# the gate reports 0 on a tree that violates the contract. codex demonstrated it
# with `pub enum DenseReadiness { Ready, Embedding, Failed }`.
#
# `enum_exact` / `arms_exact` close that by CONSTRUCTION: they parse the enum
# body and the match arms, count the members, and compare the member SET against
# the pinned one. A third variant fails whatever it is called; a third accepted
# string spelling fails even when the enum is untouched. That is the difference
# between "the names I feared are absent" and "the vocabulary is closed".
#
# ------------------ SQL IS CASE-INSENSITIVE (AND WHITESPACE) -----------------
# fix-1, codex §9 round 1 finding #2 [P2]. An `absent` probe over SQL that
# anchors on ONE uppercase spelling with single spaces is a false negative:
# SQLite accepts `insert into t`, `INSERT OR REPLACE INTO t`, `INSERT OR IGNORE
# INTO   t` and a newline anywhere whitespace is legal. codex cleared the gate
# with a single lowercase line. Every negative-space probe whose subject is SQL
# now goes through `insert_into()` / an explicit `(?i)` pattern that tolerates
# the five SQLite conflict clauses and arbitrary whitespace.
#
# Rust IDENTIFIER probes (`ProjectionRole::Vector`, `EntityTypeSpec`,
# `id_prefix`) are deliberately left case-SENSITIVE: case is significant in Rust,
# so folding case there would create false positives, not close a hole.
#
# ----------------- A SQL TABLE NAME IS `[<schema>.]<table>` ------------------
# fix-2, codex §9 round 2 findings #1 and #2 [P2]. fix-1 widened these probes for
# CASE and WHITESPACE but still required the pinned table name IMMEDIATELY after
# `INTO` / `TABLE`. That is not how SQLite names a table: every table reference
# may carry a schema qualifier, and `main.` is ALWAYS valid for the main
# database. `INSERT INTO main.canonical_attributes ...` therefore writes exactly
# the forbidden table and cleared the gate with 0; so did `CREATE VIRTUAL TABLE
# main.property_search_index ...`. `qualified()` below tolerates one optional
# schema identifier — quoted, bracketed or bare, with whitespace around the dot,
# which SQLite also accepts.
#
# NOTE WHAT THIS DOES **NOT** REACH: an ATTACHed alias whose NAME is chosen at
# runtime, or a table reached through a view/trigger under a different name, is
# still invisible here — see RESIDUAL SCOPE in this file's header. `qualified()`
# closes the spellings of the PINNED name, not every route to the pinned table.
#
# ------------- A PROBE IS BOUND TO ITS SUBJECT, NOT TO A FILE ----------------
# fix-4, codex §9 round 4 [P2] and the sweep it triggered. The FOURTH distinct
# class, and the most general one: a probe asserted that some text existed
# SOMEWHERE IN A FILE, while the clause it implements is about a NAMED SUBJECT
# inside that file. An unrelated coincidence elsewhere in the same file then
# satisfied the probe while the named subject was broken.
#
# codex's demonstration: `C1-TE-DEFAULT-TOKENIZER` COUNTED file-wide occurrences
# of `tokenize = 'porter unicode61 remove_diacritics 2'` in the schema crate
# (`min`, n=2). That file already carries several — for `search_index`,
# `search_index_v2` and the edge index. Changing ONLY `property_search_index`'s
# tokenizer left the count satisfied by tables the clause is not about, and the
# gate exited 0 on a tree where the property-FTS no longer shares the engine's
# default tokenizer. A COUNT IS NOT A BINDING; the `min` kind is gone.
#
# THE SWEEP found the same shape in fifteen more clauses, each with a working
# demonstration in the test suite (arms 12v–12ac): a `pub <field>: <ty>,` probe
# satisfied by ANY struct rather than the one the contract names (seven clauses);
# an error-variant probe satisfied by the Display impl after the variant was
# deleted from the enum (two); the apply verb's three signature fragments probed
# independently, so they need not describe one function; a statement probed
# file-wide rather than inside the verb whose body the clause is about (two); a
# `fn <name>(` probe that cannot tell a DEFINITION from a COMMENT mentioning it
# (every behavioural clause); an index probed by NAME while the clause is about
# its COLUMNS; a registry column probed file-wide; and a plan requirement ROW
# probed as a bare `| id |` mention.
#
# THE RULE, in priority order:
#   1. STRUCTURAL EXTRACTION bound to the named subject (`in_item`, `fn_sig`,
#      `fn_defined`, `sql_ddl`, `fts_tokenizer_shared`, `enum_exact`,
#      `arms_exact`). Preferred always.
#   2. A `present` regex tight enough that it CANNOT be satisfied by a different
#      declaration — it must name its subject (a unique symbol/table/index name,
#      a const together with its value, a whole table row).
#   3. There is no 3. Counts are gone.
#
# WHICH WAY THE STRUCTURAL READERS ERR: a subject they cannot locate is a CLAUSE
# FAILURE (exit 1), never a skip, exactly like `enum_exact`. Where a name could
# denote several items the readers take the STRICT side — every `struct`/`enum`/
# `fn` body of that name must satisfy the probe — except for `impl`, where a
# type's inherent impl may legitimately be split across blocks and ANY block
# suffices (every one of them is still `impl <the named type>`, so the subject
# binding holds either way).
# ---------------------------------------------------------------------------

# One SQL identifier, optionally quoted `"x"` / 'x' / `x` / [x].
_SQL_IDENT_OPEN = r"[\"'`\[]?"
_SQL_IDENT_CLOSE = r"[\"'`\]]?"


def qualified(table):
    """`[<schema> . ]<table>` — an optional schema qualifier before the pinned
    table name, each part optionally quoted/bracketed, whitespace tolerated
    around the dot (all of which SQLite accepts)."""
    schema = (
        r"(?:" + _SQL_IDENT_OPEN + r"[A-Za-z_][A-Za-z0-9_]*" + _SQL_IDENT_CLOSE
        + r"\s*\.\s*)?"
    )
    return schema + _SQL_IDENT_OPEN + table + r"\b"


def insert_into(table):
    """A case-insensitive `INSERT [OR <conflict>] INTO [<schema>.]<table>` /
    `REPLACE INTO [<schema>.]<table>` pattern.

    Tolerates all five SQLite conflict clauses (the same five the schema crate's
    own accretion guard enumerates), arbitrary whitespace including newlines
    between every token, an optional schema qualifier, and an optionally
    quoted/bracketed table name.
    """
    return (
        r"(?i)\b(?:INSERT|REPLACE)\b"
        r"(?:\s+OR\s+(?:ABORT|FAIL|IGNORE|REPLACE|ROLLBACK))?"
        r"\s+INTO\s+" + qualified(table)
    )

ASSERTIONS = {
    # ---- Cohesion seam --------------------------------------------------
    # fix-4: the three fragments used to be three INDEPENDENT file-wide probes,
    # so nothing tied them to one another OR to the apply verb — a decoy function
    # carrying `drop: &[String],` and the delta return type satisfied them while
    # `configure_projections` took no drop list at all. They are now read out of
    # THAT FUNCTION'S SIGNATURE.
    "C1-SEAM-ENGINE-BUILD-DROP": [
        ("present", ENG, r"pub fn configure_projections\("),
        ("fn_sig", ENG, "configure_projections", r"specs:\s*&\[ProjectionSpec\]"),
        ("fn_sig", ENG, "configure_projections", r"drop:\s*&\[String\]"),
        ("fn_sig", ENG, "configure_projections",
         r"->\s*Result<\s*ProjectionDelta\s*,\s*EngineError\s*>"),
    ],
    # ---- Q1 --------------------------------------------------------------
    "C1-Q1-ROLE-SET": [
        ("present", ENG, r"pub enum ProjectionRole \{"),
        ("in_item", ENG, "struct", "ProjectionSpec",
         r"pub\s+roles\s*:\s*BTreeSet\s*<\s*ProjectionRole\s*>"),
    ],
    # ---- Q3 --------------------------------------------------------------
    # fix-4 SWEEP: "reporting the applied delta" is read as a FIELD of
    # ProjectionDelta (`dropped` — the half of the delta no other clause
    # asserts), not as a bare `pub struct ProjectionDelta {` line that a decoy or
    # a comment could carry.
    "C1-Q3-SOLE-AUTHORITY": [
        ("sql_ddl", SCH, "_fathomdb_projection_registry",
         r"(?i)\bname\s+TEXT\s+PRIMARY\s+KEY\b"),
        ("fn_defined", ENG, "apply_projection_config"),
        ("in_item", ENG, "struct", "ProjectionDelta",
         r"pub\s+dropped\s*:\s*Vec\s*<\s*String\s*>"),
    ],
    "C1-Q3-DESTRUCTIVE-DELTA": [
        ("in_item", ENG, "enum", "EngineError", r"\bProjectionDestructive\s*\{"),
        ("fn_defined", T15, "destructive_change_requires_explicit_drop"),
    ],
    "C1-Q3-OMISSION-NOT-DROP": [
        ("fn_defined", T15, "role_add_builds_and_explicit_drop_drops_exactly_one"),
        ("fn_defined", T15, "dropping_an_absent_name_is_a_clean_noop"),
    ],
    # ---- Q5 --------------------------------------------------------------
    "C1-Q5-DERIVED-CACHE-IDEMPOTENT": [
        ("in_item", ENG, "struct", "ProjectionDelta", r"pub\s+unchanged\s*:\s*bool"),
        ("fn_defined", T15, "idempotent_reregistration_is_a_noop"),
    ],
    # ---- Q2 --------------------------------------------------------------
    # fix-4: the EAV store and the property-FTS were probed by TABLE NAME only,
    # and the composite index by INDEX NAME only — but the clause is about the
    # SHAPE: an attribute store keyed by (name, value) and the index the cheap
    # filterable tier reads. Re-creating that index on ONE column passed. The
    # table probes are structural now (a `CREATE TABLE` whose body must carry the
    # columns), and the index probe names its COLUMNS.
    "C1-Q2-ENGINE-EAV-PROPERTY-FTS": [
        ("sql_ddl", SCH, "canonical_attributes", r"(?i)\bwrite_cursor\s+INTEGER\b"),
        ("sql_ddl", SCH, "canonical_attributes", r"(?i)\battr_name\s+TEXT\b"),
        ("sql_ddl", SCH, "canonical_attributes", r"(?i)\battr_value\s+TEXT\b"),
        ("sql_ddl", SCH, "property_search_index", r"(?i)\battr_value\b"),
        ("present", SCH,
         r"(?i)CREATE\s+INDEX\s+canonical_attributes_name_value_idx\s+ON\s+"
         r"canonical_attributes\s*\(\s*attr_name\s*,\s*attr_value\s*\)"),
    ],
    "C1-Q2-ENGINE-PROJECTS-VIA-CONFIGURE": [
        ("fn_defined", T15, "property_filter_returns_correct_rows"),
        ("fn_defined", T15, "property_fts_search_returns_correct_rows"),
    ],
    # SCOPE (load-bearing, fix-1 then fix-3). The clause forbids a
    # MIGRATION/BACKFILL of PRE-EXISTING rows — not projection writes as such.
    # The two negative probes are therefore scoped to THE SCHEMA CRATE, which IS
    # the migration ladder: any `INSERT ... INTO canonical_attributes` there runs
    # at migrate time over rows that already existed, which is exactly the
    # forbidden thing. The LEGITIMATE writes — the per-declaration,
    # same-transaction projection INSERTs — live in the ENGINE
    # (`configure_projections`) and are deliberately out of scope, so widening
    # these patterns cannot produce a false positive on them.
    #
    # fix-3, codex §9 round 3 finding #1 [P2]: the scope is the crate's SOURCE
    # TREE (SCH_TREE), not its lib.rs. A backfill added in ANY module of the
    # schema crate runs at migrate time exactly as one in lib.rs does; scoping
    # the check to one file only ever asserted "nobody added a module".
    # exts=None (every text file) rather than *.rs, so a migration carried in a
    # `.sql` file and pulled in with `include_str!` is in scope too.
    #
    # Residual, stated honestly: the probe reads the schema crate as TEXT, so a
    # doc comment that literally spells `insert into canonical_attributes` would
    # trip it. That is a false RED (reword the comment), which is the safe side
    # of this gate — unlike the false GREEN it replaces.
    #
    # fix-4 AUDIT: the marker probe is a `doc_text` probe and is DECLARED as one.
    # Its subject IS a documented statement, so there is nothing structural to
    # bind it to — and it is deliberately not what carries this clause: the two
    # TREE-scoped negatives are. Demoting the marker to a comment elsewhere would
    # not violate the obligation (no backfill would thereby appear), which is the
    # test for whether a text probe is load-bearing.
    "C1-Q2-NO-DATA-MIGRATION": [
        ("doc_text", SCH, r"NO DATA MIGRATION \(HITL 2026-07-21\): shape only, no backfill\."),
        ("absent_tree", SCH_TREE, insert_into("canonical_attributes"), None),
        ("absent_tree", SCH_TREE, insert_into("property_search_index"), None),
    ],
    # ---- Q4 --------------------------------------------------------------
    # fix-4: "IN THE SAME TRANSACTION AS THE APPLY" is a statement about the
    # apply verb's BODY, and was probed file-wide — a decoy call elsewhere
    # satisfied it while the real call ran outside the transaction.
    "C1-Q4-CHEAP-SAME-TRANSACTION": [
        ("in_item", ENG, "fn", "configure_projections", r"apply_projection_config\(&tx,"),
        ("in_item", ENG, "struct", "ProjectionDelta", r"pub\s+built\s*:\s*Vec\s*<\s*String\s*>"),
    ],
    # "EXACTLY {ready, embedding}" is a CLOSED vocabulary, so it is asserted
    # structurally (fix-1, codex finding #1): the enum has exactly two variants,
    # and each conversion fn maps exactly two (variant, string) pairs. The
    # `present` probes are kept as a spelling regression guard, and the negative
    # `::Pending` probe is kept because the clause names that token
    # specifically ("reserved for the orthogonal admission axis") — but neither
    # is what closes the vocabulary any more.
    #
    # fix-3 SWEEP: that negative probe is scoped to the ENGINE CRATE TREE. The
    # reserved token is just as much a violation in a sibling module as in
    # lib.rs, and since a variant cannot be added to `enum DenseReadiness` from
    # outside the crate that declares it, the crate tree is the COMPLETE scope
    # for this obligation, not merely a wider one.
    #
    # fix-4 SWEEP: the two spelling probes are read INSIDE `impl DenseReadiness`
    # rather than file-wide. They were never a false-green vector (the
    # `arms_exact` probes below strictly dominate them — any edit that could fool
    # a file-wide spelling probe fails the exact-pair check), but a probe that
    # states its subject is worth more than one that happens to be covered.
    "C1-Q4-DENSE-READINESS-TWO-MEMBERS": [
        ("present", ENG, r"pub enum DenseReadiness \{"),
        ("in_item", ENG, "impl", "DenseReadiness", r'DenseReadiness::Ready => "ready",'),
        ("in_item", ENG, "impl", "DenseReadiness", r'DenseReadiness::Embedding => "embedding",'),
        ("absent_tree", ENG_TREE, r"DenseReadiness::Pending", (".rs",)),
        ("enum_exact", ENG, "DenseReadiness", ("Ready", "Embedding")),
        ("arms_exact", ENG, "DenseReadiness", "as_str",
         (("Ready", "ready"), ("Embedding", "embedding"))),
        ("arms_exact", ENG, "DenseReadiness", "from_str_opt",
         (("Ready", "ready"), ("Embedding", "embedding"))),
    ],
    "C1-Q4-NO-PROVISIONAL-CONCEPT": [
        ("absent_tree", SRC_TREE, r"(?i)provisional", (".rs",)),
    ],
    # ---- Q6(a) -----------------------------------------------------------
    # Same CLASS as the readiness clause (fix-1 sweep): "exactly {filterable,
    # rankable, searchable}" was probed as three presents plus a blacklist of
    # two names, so a FOURTH role under any other name passed. Closed
    # structurally now; the two negative probes are kept because Vector/Fts are
    # the specific confusion the clause calls out (they are TIER LABELS on the
    # sub-objects, not roles). fix-3 SWEEP scopes both to the ENGINE CRATE TREE,
    # for the same reason as the readiness clause: the crate that declares
    # `enum ProjectionRole` is the complete scope in which a role can be named.
    "C1-Q6A-THREE-ROLES": [
        ("in_item", ENG, "impl", "ProjectionRole",
         r'"filterable" => Some\(ProjectionRole::Filterable\),'),
        ("in_item", ENG, "impl", "ProjectionRole",
         r'"rankable" => Some\(ProjectionRole::Rankable\),'),
        ("in_item", ENG, "impl", "ProjectionRole",
         r'"searchable" => Some\(ProjectionRole::Searchable\),'),
        ("absent_tree", ENG_TREE, r"ProjectionRole::Vector\b", (".rs",)),
        ("absent_tree", ENG_TREE, r"ProjectionRole::Fts\b", (".rs",)),
        ("enum_exact", ENG, "ProjectionRole", ("Filterable", "Rankable", "Searchable")),
        ("arms_exact", ENG, "ProjectionRole", "as_str",
         (("Filterable", "filterable"), ("Rankable", "rankable"),
          ("Searchable", "searchable"))),
        ("arms_exact", ENG, "ProjectionRole", "from_str_opt",
         (("Filterable", "filterable"), ("Rankable", "rankable"),
          ("Searchable", "searchable"))),
    ],
    "C1-Q6A-RANKABLE-GRACEFUL-DEFER": [
        ("in_item", ENG, "struct", "ProjectionDelta",
         r"pub\s+deferred\s*:\s*Vec\s*<\s*String\s*>"),
        ("fn_defined", T15, "rankable_is_graceful_deferred_never_blocking"),
        ("fn_defined", T15, "idempotent_reregistration_holds_for_deferred_rankable"),
    ],
    # ---- Q6(b), AS AMENDED at efa8d584 -----------------------------------
    "C1-Q6B-NO-ENTITYTYPESPEC-NO-IDPREFIX": [
        ("absent_tree", SRC_TREE, r"EntityTypeSpec", None),
        ("absent_tree", SRC_TREE, r"\bid_prefix\b", None),
        ("absent_tree", SRC_TREE, r"\bidPrefix\b", None),
    ],
    # Same CLASS again, and the worst instance (fix-1 sweep): "TOTAL over exactly
    # those three" was probed with FOUR `present` probes and NO negative at all,
    # so a fourth variant — the precise violation of totality-over-three — could
    # not be seen even in principle. Closed structurally over BOTH conversion
    # fns, so a fourth prefix or a fourth discriminant spelling fails too.
    "C1-Q6B-IDSPACE-TOTAL-THREE": [
        ("present", ENG, r"pub enum IdSpaceKind \{"),
        ("in_item", ENG, "impl", "IdSpaceKind", r'Self::Logical => "l:",'),
        ("in_item", ENG, "impl", "IdSpaceKind", r'Self::Content => "h:",'),
        ("in_item", ENG, "impl", "IdSpaceKind", r'Self::Passage => "p:",'),
        ("enum_exact", ENG, "IdSpaceKind", ("Logical", "Content", "Passage")),
        ("arms_exact", ENG, "IdSpaceKind", "prefix",
         (("Logical", "l:"), ("Content", "h:"), ("Passage", "p:"))),
        ("arms_exact", ENG, "IdSpaceKind", "as_str",
         (("Logical", "logical"), ("Content", "content"), ("Passage", "passage"))),
    ],
    # fix-2 SWEEP (not a codex finding): the same NARROW-REGEX class as findings
    # #1/#2, found by sweeping every remaining negative probe. The `absent` probe
    # was the literal string `pub id: Option<IdSpace>`, so any legal Rust
    # respacing (`pub id : Option < IdSpace >`) evaded it. The clause is not
    # trivially exploitable — the paired `present` probe fails on the same edit —
    # but a decoy struct carrying `pub id: IdSpace,` satisfies that probe and
    # leaves the narrow negative probe as the only thing standing (fixture 12p,
    # which exited 0 before that round). Whitespace-tolerant now.
    #
    # fix-3 SWEEP: and TREE-scoped. A second, NULLABLE typed-id carrier declared
    # in a sibling module (fixture 12s) is exactly the same violation, and the
    # paired `present` probe in lib.rs holds untouched while it happens.
    #
    # fix-4 SWEEP: the paired POSITIVE probe is read out of `struct SearchHit`.
    # File-wide, it was satisfied by any decoy struct declaring `pub id: IdSpace,`
    # while SearchHit carried no typed id at all — and note that renaming
    # SearchHit's field (rather than making it nullable) does not trip the
    # negative probe either, so the clause had a complete false-green path.
    "C1-Q6B-ID-NON-NULL": [
        ("in_item", ENG, "struct", "SearchHit", r"pub\s+id\s*:\s*IdSpace\s*,"),
        ("absent_tree", ENG_TREE, r"pub\s+id\s*:\s*Option\s*<\s*IdSpace", (".rs",)),
    ],
    # fix-4 SWEEP: the error variant is read out of `enum EngineError`. File-wide,
    # `NotLifecycleAddressable {` is spelled by the Display impl and by every
    # construction site, so deleting the VARIANT left the probe satisfied.
    "C1-Q6B-H-TERMINAL-NOT-LIFECYCLE-ADDRESSABLE": [
        ("in_item", ENG, "enum", "EngineError", r"\bNotLifecycleAddressable\s*\{"),
        ("fn_defined", T25,
         "an_anonymous_write_stays_anonymous_through_the_whole_durable_path"),
    ],
    "C1-Q6B-SURROGATE-GOVERNED-ONLY": [
        ("fn_defined", T25, "registering_projections_never_alters_a_pre_existing_row_id_space"),
        ("fn_defined", T25, "the_internal_structural_row_writer_mints_no_logical_id"),
    ],
    # ---- Apply atomicity -------------------------------------------------
    # fix-4 SWEEP: every "the named proof still exists" probe is `fn_defined`,
    # which reads the COMMENT-STRIPPED source and requires a parseable body. The
    # old `fn <name>(` regex could not tell a definition from a doc comment
    # mentioning the name — so deleting the proof and leaving a reference to it
    # behind (the most natural thing a contributor does) exited 0.
    "C1-AA-ATOMIC-FLIP": [
        ("fn_defined", ENG, "commit_projection_outcomes"),
        ("fn_defined", T20,
         "atomic_flip_never_exposes_ready_without_the_vector_under_concurrent_write"),
    ],
    # fix-4 SWEEP: "the apply ENQUEUES and returns" is a statement about the apply
    # verb. `notify_new_work()` file-wide is satisfied by the engine's three
    # unrelated wake sites, so removing the wake FROM THE APPLY was invisible.
    "C1-AA-NO-BLOCK-ON-EMBEDDING": [
        ("fn_defined", ENG, "notify_new_work"),
        ("in_item", ENG, "fn", "configure_projections", r"notify_new_work\(\)"),
        ("fn_defined", T20,
         "readiness_reads_embedding_while_embeds_are_outstanding_then_flips_to_ready"),
    ],
    "C1-AA-CRASH-HEAL-BOOT-REDERIVE": [
        ("fn_defined", ENG, "load_projection_registry"),
        ("fn_defined", T15, "boot_rederive_converges_after_simulated_crash"),
    ],
    # ---- Tokenizer / embedder defaults -----------------------------------
    "C1-TE-DEFAULT-EMBEDDER": [
        ("in_item", ENG, "struct", "ProjectionVector",
         r"pub\s+embedder\s*:\s*Option\s*<\s*String\s*>"),
        ("present", EMB,
         r'pub const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-bge-small-en-v1\.5";'),
    ],
    # fix-4, codex §9 round 4 [P2] — THE FINDING. The obligation is RELATIONAL:
    # "the default tokenizer is the ENGINE'S DEFAULT FTS5 TOKENIZER — THE ONE
    # body-FTS USES". It was probed as a COUNT of file-wide occurrences of the
    # default tokenizer string (n=2), and the schema file carries several for
    # UNRELATED FTS tables, so changing ONLY `property_search_index`'s tokenizer
    # left the count satisfied and the gate green.
    #
    # It is now what the clause says: extract `property_search_index`'s
    # `tokenize=` clause and the body-FTS tables' `tokenize=` clauses from their
    # own `CREATE VIRTUAL TABLE ... USING fts5(...)` definitions and COMPARE them,
    # then check the shared value against the pinned default. Deleting the clause
    # (fts5 would silently fall back to its own `unicode61`) fails too.
    #
    # THE REFERENCE SET is `search_index_v2` (the fielded body-FTS the pin's own
    # evidence cites as body-FTS, schema lib.rs:436) and `search_index` (the
    # retained legacy body index). `search_index_edges` is deliberately NOT in it:
    # the edge index is a different subject, and its equal tokenizer was part of
    # the coincidence that kept the old count satisfied.
    "C1-TE-DEFAULT-TOKENIZER": [
        ("in_item", ENG, "struct", "ProjectionFts",
         r"pub\s+tokenizer\s*:\s*Option\s*<\s*String\s*>"),
        ("fts_tokenizer_shared", SCH, "property_search_index",
         ("search_index_v2", "search_index"), "porter unicode61 remove_diacritics 2"),
    ],
    # The third probe is SQL, so it carries the SAME defect classes as
    # C1-Q2-NO-DATA-MIGRATION and has been fixed in every sweep alongside it:
    # fix-1 (a lowercase `create virtual table property_search_index using
    # fts5(...)` cleared the uppercase-anchored original), fix-2 (the same DDL
    # schema-qualified), and now fix-3 — codex §9 round 3 finding #2 [P2]: the
    # probe read ENGINE lib.rs alone, so the identical DDL in a NEW engine module
    # (codex's `src/rust/crates/fathomdb-engine/src/extra_fts.rs`) exited 0.
    #
    # The obligation is that THE ENGINE does not create the property-FTS table
    # itself (the schema migration owns it, with the default tokenizer; a
    # declared override is recorded and not honoured), so the scope is the ENGINE
    # CRATE TREE — the whole of it. exts=None for the same reason as the schema
    # clause: DDL carried in a `.sql` file and `include_str!`-ed is still the
    # engine creating that table.
    #
    # fix-4 SWEEP: the RECORDING column is read out of the projection registry's
    # own DDL — file-wide, `fts_tokenizer TEXT,` was satisfied by any table
    # declaring that column. The second probe is a `doc_text` probe and is
    # declared as one: the clause's enforceable content is (i) the registry
    # records the declaration, (ii) the engine builds no FTS table of its own, and
    # (iii) the property FTS carries the DEFAULT tokenizer — which is the sibling
    # clause C1-TE-DEFAULT-TOKENIZER, now a real comparison.
    "C1-TE-CUSTOM-TOKENIZER-DEFERRED": [
        ("sql_ddl", SCH, "_fathomdb_projection_registry", r"(?i)\bfts_tokenizer\s+TEXT\b"),
        ("doc_text", SCH, r"recorded in the registry but not honoured here"),
        ("absent_tree", ENG_TREE,
         r"(?i)CREATE\s+VIRTUAL\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?"
         + qualified("property_search_index"), None),
    ],
    # ---- Landing ---------------------------------------------------------
    # fix-4 SWEEP: the clause is "the 0.8.20 plan must actually CARRY the C-1
    # co-land requirements", i.e. four ROWS of its requirement table. `\| R-20-PR
    # \|` matched any mention between two pipes, including one inside a sentence
    # of running prose, so a requirement withdrawn from the table stayed green.
    # The pattern is now a whole three-cell table row, anchored at line start.
    "C1-LAND-0820-SLOT": [
        ("present", PLAN, r"(?m)^\| R-20-PR \| .+ \| .+ \|$"),
        ("present", PLAN, r"(?m)^\| R-20-EAV \| .+ \| .+ \|$"),
        ("present", PLAN, r"(?m)^\| R-20-DR \| .+ \| .+ \|$"),
        ("present", PLAN, r"(?m)^\| R-20-SUR \| .+ \| .+ \|$"),
    ],
}

CATEGORY_COUNT_KEY = {"CHECKABLE": "checkable", "CROSS-REPO": "cross_repo", "PROSE": "prose"}
COUNT_KEYS = ["checkable", "cross_repo", "prose", "total"]


def die_env(msg):
    print("check-c1-conformance: " + msg)
    sys.exit(2)


# --------------------------------------------------------- --list-sources ----
if LIST_SOURCES:
    files, trees = set(), set()
    for probes in ASSERTIONS.values():
        for probe in probes:
            if probe[0] == "absent_tree":
                trees.add(probe[1])
            else:
                files.add(probe[1])
    for path in sorted(files):
        print("file\t" + path)
    for path in sorted(trees):
        print("tree\t" + path)
    sys.exit(0)

failures = []


def fail(msg):
    failures.append(msg)
    print("FAIL  c1-contract-conformance: " + msg)


# ---------------------------------------------------------------- the pin ----
# An unusable pin is a broken GATE (exit 2), not a conformance divergence
# (exit 1): exit 1 would send the reader to the Steward to re-reconcile a
# contract that may not have moved at all.
try:
    with open(PIN, "rb") as fh:
        pin = json.loads(fh.read().decode("utf-8"))
except OSError as exc:
    die_env(
        f"cannot read the pin {PIN}: {exc} — the gate cannot run, so it refuses to pass "
        "(TC-37 evaporation path #3)"
    )
except Exception as exc:
    die_env(
        f"the pin {PIN} is not valid JSON: {exc} — the gate cannot run, so it refuses to pass "
        "(TC-37 evaporation path #3)"
    )

if not isinstance(pin, dict):
    die_env(f"the pin {PIN} is not a JSON object")

pin_errors = []


def pin_broken(msg):
    pin_errors.append(msg)


for key in ["sha256", "git_blob_sha1", "sha256_whitespace_normalized", "counts", "clauses"]:
    if key not in pin:
        die_env(f"the pin {PIN} has no {key!r} field — it cannot vouch for anything")

for key in ["sha256", "git_blob_sha1", "sha256_whitespace_normalized"]:
    if not isinstance(pin[key], str) or not pin[key]:
        die_env(
            f"the pin {PIN}: {key!r} is {pin[key]!r}, not a non-empty string — the pin is "
            "MALFORMED and cannot vouch for any content. Reported as a broken gate (exit 2), "
            "not as a contract divergence (exit 1): a hash that is not a hash never equals the "
            "contract's, so exit 1 would send the reader off to reconcile a document that may "
            "not have moved at all."
        )

if not isinstance(pin["counts"], dict):
    die_env(f"the pin {PIN}: 'counts' is not an object")
if not isinstance(pin["clauses"], list) or not pin["clauses"]:
    die_env(f"the pin {PIN}: 'clauses' is not a non-empty list — there is no clause registry")

# EVERY pinned count must be PRESENT and an integer. A count the gate cannot
# read is a MALFORMED PIN, never an implicit skip: read permissively, an absent
# entry comes back as None and the corresponding check silently disappears —
# which is how a green gets bought rather than earned (the hole
# check-governed-surface-pin.sh had to close). bool is excluded explicitly
# because isinstance(True, int) is True in Python and `True == 1` would let a
# count of `true` masquerade as 1.
for key in COUNT_KEYS:
    if key not in pin["counts"]:
        die_env(
            f"the pin {PIN}: 'counts' has no {key!r} entry — the pin is MALFORMED and cannot be "
            f"trusted. counts.{key} is the backstop that catches a re-pin which edits the clause "
            "registry but not the tally; if it is absent the gate would silently stop checking "
            "the size of that category altogether. DO NOT 'fix' this by regenerating the pin: a "
            "pin is only regenerated by RE-DERIVING the clause registry from the contract text. "
            "Restore the pin from git instead."
        )
    declared = pin["counts"][key]
    if isinstance(declared, bool) or not isinstance(declared, int):
        die_env(
            f"the pin {PIN}: counts.{key} is {declared!r}, which is not an integer — the pin is "
            "MALFORMED and cannot be trusted. A non-integer count can be compared neither "
            "against the clause registry nor against anything else, so it weakens this gate to "
            "exactly the same degree as deleting it. DO NOT 'fix' this by regenerating the pin; "
            "restore it from git."
        )

# ---- the clause registry itself --------------------------------------------
seen_ids = set()
by_category = {"CHECKABLE": [], "CROSS-REPO": [], "PROSE": []}
for index, clause in enumerate(pin["clauses"]):
    if not isinstance(clause, dict):
        die_env(f"the pin {PIN}: clauses[{index}] is not an object")
    for key in ["id", "category", "obligation"]:
        if key not in clause or not isinstance(clause[key], str) or not clause[key]:
            die_env(
                f"the pin {PIN}: clauses[{index}] has no usable {key!r} — every clause must "
                "carry an id, a category and a one-line obligation, or the registry is not "
                "reviewable and the pin cannot vouch for it."
            )
    if clause["category"] not in by_category:
        die_env(
            f"the pin {PIN}: clause {clause['id']!r} has category {clause['category']!r}, which "
            f"is not one of {sorted(by_category)} — the classification is the whole point of the "
            "registry, so an unknown category is a MALFORMED pin."
        )
    if clause["id"] in seen_ids:
        die_env(f"the pin {PIN}: clause id {clause['id']!r} appears more than once")
    seen_ids.add(clause["id"])
    by_category[clause["category"]].append(clause["id"])

# (c) counts vs the registry, asserted separately from the member lists.
for category, count_key in CATEGORY_COUNT_KEY.items():
    declared = pin["counts"][count_key]
    actual = len(by_category[category])
    if declared != actual:
        pin_broken(
            f"the pin {PIN} is internally inconsistent: counts.{count_key} says {declared} but "
            f"its clause registry holds {actual} {category} clause(s) — a botched re-pin, not a "
            "usable statement of what was classified."
        )
if pin["counts"]["total"] != len(pin["clauses"]):
    pin_broken(
        f"the pin {PIN} is internally inconsistent: counts.total says {pin['counts']['total']} "
        f"but its clause registry holds {len(pin['clauses'])} clause(s)."
    )

# (b) THE BIJECTION, both directions.
pinned_checkable = set(by_category["CHECKABLE"])
implemented = set(ASSERTIONS)
for orphan in sorted(pinned_checkable - implemented):
    pin_broken(
        f"the pin {PIN} registers CHECKABLE clause {orphan!r} for which this gate implements NO "
        "assertion. The pin therefore OVER-STATES what is verified: it claims a mechanical check "
        "that does not exist. Either implement the assertion or re-classify the clause honestly "
        "(CROSS-REPO / PROSE) with a written reason."
    )
for unregistered in sorted(implemented - pinned_checkable):
    where = "is not in the registry at all" if unregistered not in seen_ids \
        else f"is registered as {next(c['category'] for c in pin['clauses'] if c['id'] == unregistered)!r}"
    pin_broken(
        f"this gate implements an assertion for clause {unregistered!r}, which the pin does NOT "
        f"register as CHECKABLE (it {where}). The pinned check set has SHRUNK — the id VANISHED "
        "or was RECLASSIFIED out of the checked set, which is the quietest way to buy a green. "
        "A clause is demoted only by re-deriving the registry from the contract text, under "
        "review."
    )

if pin_errors:
    for msg in pin_errors:
        print("check-c1-conformance: " + msg)
    print(
        "\n"
        "  The pin is MALFORMED, so this gate has no trustworthy statement of what it is\n"
        "  supposed to check. That is reported as a BROKEN GATE (exit 2), not as a conformance\n"
        "  divergence (exit 1): nothing has been shown about the contract or the code either way.\n"
        "  Restore scripts/c1-conformance-pin.json from git, or re-derive the clause registry\n"
        "  from the contract text under review."
    )
    sys.exit(2)

WHERE = f"pinned at {pin.get('pinned_at_commit_short', '?')}"

# ----------------------------------------------------------- the contract ----
# TC-37 path #2: missing / unreadable is a HARD failure, and specifically a
# BROKEN GATE — the gate never saw the document it vouches for, so it has shown
# nothing about conformance at all.
try:
    with open(CONTRACT, "rb") as fh:
        raw = fh.read()
except OSError as exc:
    die_env(
        f"cannot read the pinned contract {CONTRACT}: {exc}. The gate cannot see the document it "
        "vouches for, so it refuses to report a verdict (TC-37 evaporation path #2). A gate that "
        "cannot see its subject and reports green is an active false assurance — and a ratified "
        "cross-repo contract that has moved or vanished is itself the largest possible change to "
        "the thing being checked."
    )

got_sha256 = hashlib.sha256(raw).hexdigest()
got_blob = hashlib.sha1(b"blob %d\0" % len(raw) + raw).hexdigest()
contract_moved = got_sha256 != pin["sha256"] or got_blob != pin["git_blob_sha1"]

formatting_only = False
if contract_moved:
    try:
        normalized = re.sub(r"\s+", " ", raw.decode("utf-8")).strip()
    except UnicodeDecodeError:
        normalized = None
    if normalized is not None:
        got_norm = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
        formatting_only = got_norm == pin["sha256_whitespace_normalized"]
    fail(
        f"the pinned contract {CONTRACT} has MOVED: its content differs from the pin ({WHERE}).\n"
        f"        pinned   sha256 {pin['sha256']}  git-blob {pin['git_blob_sha1']}\n"
        f"        on disk  sha256 {got_sha256}  git-blob {got_blob}"
    )
    if formatting_only:
        print(
            "NOTE  c1-contract-conformance: the contract's normalized text is IDENTICAL — this is "
            "a WHITESPACE/FORMATTING-ONLY change. It still fails, deliberately: the pin is a "
            "CONTENT hash over the document's raw bytes, because a 'harmless reformat' is the "
            "ideal cover for a clause quietly reworded on an adjacent line."
        )

# ------------------------------------------------- the clause assertions ----
_file_cache = {}


def read_source(rel):
    """Read a root-relative source file. Unreadable ⇒ TC-37 path #4 (exit 2)."""
    if rel in _file_cache:
        return _file_cache[rel]
    path = os.path.join(ROOT, rel)
    try:
        with open(path, "rb") as fh:
            text = fh.read().decode("utf-8", errors="replace")
    except OSError as exc:
        die_env(
            f"cannot read {rel} under --root {ROOT}: {exc} — a clause assertion could not be "
            "EVALUATED. That is neither a pass nor a clause failure (TC-37 evaporation path #4): "
            "the gate computed no verdict for that clause, so it refuses to report one. Point "
            "--root at a complete source tree, or restore the file."
        )
    _file_cache[rel] = text
    return text


_tree_cache = {}


def walk_tree(rel, exts):
    """Every (path, text) text file under a root-relative tree, as a LIST.

    TC-37 path #4 FOR A TREE-SCOPED SUBJECT (fix-3). Once a probe's subject is a
    TREE rather than a FILE, "missing" needs a definition, and there are two
    ways a tree can fail to be a subject:

      * THE DIRECTORY IS ABSENT. Unambiguous: the assertion could not be
        evaluated at all. exit 2, as it always has been.
      * THE DIRECTORY YIELDS NO CANDIDATE FILE (it is empty, or holds nothing
        matching `exts`, or only binaries). Judgement call, decided this way: a
        NEGATIVE assertion that examined ZERO files is trivially satisfiable and
        would report "no violation found" having looked at nothing. That is the
        vacuous pass in its purest form, and it is what a renamed/moved crate
        leaves behind. exit 2.

    A LIST rather than a generator, because "examined nothing" can only be known
    after the walk finishes, and a caller that returns early on the first match
    would never let a generator get there. Cached per (tree, exts): the three
    negative-space probes of C1-Q6B share one walk of src/, and the whole scan
    happens once per distinct scope rather than once per probe.
    """
    key = (rel, exts)
    if key in _tree_cache:
        return _tree_cache[key]
    base = os.path.join(ROOT, rel)
    if not os.path.isdir(base):
        die_env(
            f"the source tree {rel} does not exist under --root {ROOT} — a clause assertion could "
            "not be EVALUATED (TC-37 evaporation path #4). The gate computed no verdict for that "
            "clause, so it refuses to report one."
        )
    found = []
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS)
        for name in sorted(filenames):
            if exts is not None and not name.endswith(tuple(exts)):
                continue
            full = os.path.join(dirpath, name)
            try:
                with open(full, "rb") as fh:
                    blob = fh.read()
            except OSError as exc:
                die_env(
                    f"cannot read {os.path.relpath(full, ROOT)} while scanning {rel}: {exc} — a "
                    "clause assertion could not be EVALUATED (TC-37 evaporation path #4)."
                )
            if b"\0" in blob[:8192]:
                continue  # binary; the contract's negative space is about source text
            found.append((os.path.relpath(full, ROOT), blob.decode("utf-8", errors="replace")))
    if not found:
        scope = "text file" if exts is None else "/".join(exts) + " file"
        die_env(
            f"the source tree {rel} under --root {ROOT} exists but holds no {scope} — 0 files were "
            "scanned, so a NEGATIVE clause assertion would have reported 'no violation' having "
            "examined NOTHING. That is not a pass, it is TC-37 evaporation path #4: the assertion "
            "could not "
            "be EVALUATED. Point --root at a complete source tree, or restore the crate."
        )
    _tree_cache[key] = found
    return found


def line_of(text, match):
    return text.count("\n", 0, match.start()) + 1


# ---------------------------------------------------------------------------
# STRUCTURAL RUST READING for the CLOSED-VOCABULARY probes (fix-1).
#
# Deliberately a small, dumb reader, not a Rust parser: it strips comments,
# brace-matches ONE named block, and reads what is inside. Everything it can
# fail to find is reported as a CLAUSE DEFECT (exit 1) rather than being
# skipped, so a rename or a refactor that puts the enum somewhere this reader
# cannot see it goes RED and gets looked at — it never quietly stops checking.
#
# A STRING LITERAL IS NOT CODE (fix-4b). Every reader here LOCATES its subject in
# a view where comments AND string/char literal CONTENTS have been blanked, and
# then slices the body out of the comment-stripped view that still HAS the
# literals (`arms_exact` needs them; that is its whole subject). The blanking is
# LENGTH-PRESERVING precisely so one index means the same thing in both views.
#
# Why it matters: without it, `const S: &str = "fn foo() { }";` reads exactly
# like a definition and `"pub enum DenseReadiness { Ready, Embedding }"` exactly
# like a declaration — so a deleted test, or a THIRD enum variant with a decoy
# string ahead of the real declaration, exited 0. That is the fix-4 class (a
# probe satisfied by something that is not its subject) and it fails GREEN.
#
# Known limits, stated rather than papered over: this is a lexer, not a parser.
# `macro_rules!` bodies and `#[cfg]`-disabled code read like ordinary code; an
# UNTERMINATED literal blanks to end-of-file; brace-matching still does not
# understand macros. All of those surface as a clause FAILURE (the reader finds
# no parseable block, or finds a body it cannot match), never as a silent pass.
# ---------------------------------------------------------------------------
_view_cache = {}
_code_cache = {}

_IDENT_CHARS = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_")


def blank_literals(text):
    """`text` with the CONTENTS of every string/char literal replaced by spaces.

    LINE- and LENGTH-preserving (a newline inside a multi-line literal stays a
    newline), and the delimiters themselves are kept, so offsets and line numbers
    are identical to the input and `arms_exact` can still read the
    real literals out of the un-blanked view at the same indices. Handles raw
    (`r"..."`, `r#"..."#`), byte (`b"..."`, `br#"..."#`) and char literals, and
    leaves LIFETIMES (`'a`) alone — mistaking one for a char literal would blank
    real code.
    """
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        char = text[i]
        # Raw/byte-raw string: the escape rules differ, so it is matched first.
        if char in "rb" and (i == 0 or text[i - 1] not in _IDENT_CHARS):
            j = i + 1 if (char == "b" and i + 1 < n and text[i + 1] == "r") else i
            if text[j] == "r":
                k = j + 1
                while k < n and text[k] == "#":
                    k += 1
                if k < n and text[k] == '"':
                    close = '"' + "#" * (k - j - 1)
                    end = text.find(close, k + 1)
                    end = n if end == -1 else end
                    for p in range(k + 1, end):
                        out[p] = "\n" if text[p] == "\n" else " "
                    i = min(end + len(close), n)
                    continue
        if char == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == '"':
                    break
                j += 1
            for p in range(i + 1, min(j, n)):
                out[p] = "\n" if text[p] == "\n" else " "
            i = min(j, n) + 1
            continue
        if char == "'":
            if i + 1 < n and text[i + 1] == "\\":       # '\n', '\'', '\u{1F}'
                j = i + 2
                while j < n and text[j] != "'":
                    j += 1
                for p in range(i + 1, min(j, n)):
                    out[p] = "\n" if text[p] == "\n" else " "
                i = min(j, n) + 1
                continue
            if i + 2 < n and text[i + 2] == "'":        # 'x'
                out[i + 1] = " "
                i += 3
                continue
            i += 1                                      # a lifetime / loop label
            continue
        i += 1
    return "".join(out)


def rust_view(rel):
    """A comment-stripped view of a Rust source file (cached).

    STRING LITERALS ARE PRESENT here — `arms_exact` and `body_strings` read them
    as their subject. Use `rust_code(rel)` to LOCATE anything.
    """
    if rel not in _view_cache:
        text = read_source(rel)
        text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
        text = re.sub(r"//[^\n]*", "", text)
        _view_cache[rel] = text
    return _view_cache[rel]


def rust_code(rel):
    """`rust_view(rel)` with literal CONTENTS blanked — same length, same offsets."""
    if rel not in _code_cache:
        _code_cache[rel] = blank_literals(rust_view(rel))
    return _code_cache[rel]


def brace_body(text, open_index, source=None):
    """Body of the brace block whose '{' sits at open_index; None if unbalanced.

    Braces are counted in `text` (always the blanked CODE view) and the body is
    sliced out of `source` (the view that still has the literals), which is sound
    because blanking preserves length.
    """
    src = text if source is None else source
    depth = 0
    for i in range(open_index, len(text)):
        char = text[i]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return src[open_index + 1:i]
    return None


def enum_variants(rel, ty):
    """Every variant identifier of `enum ty`, in declaration order; None if the
    declaration is absent or unparseable."""
    code = rust_code(rel)
    match = re.search(r"\benum\s+" + re.escape(ty) + r"\b[^{;]*\{", code)
    if match is None:
        return None
    body = brace_body(code, match.end() - 1)
    if body is None:
        return None
    body = re.sub(r"#\[[^\]]*\]", " ", body)
    chunks, depth, current = [], 0, ""
    for char in body:
        if char in "({[":
            depth += 1
        elif char in ")}]":
            depth -= 1
        if char == "," and depth == 0:
            chunks.append(current)
            current = ""
        else:
            current += char
    chunks.append(current)
    variants = []
    for chunk in chunks:
        chunk = chunk.strip()
        if not chunk:
            continue
        name = re.match(r"([A-Za-z_][A-Za-z0-9_]*)", chunk)
        if name is None:
            return None
        variants.append(name.group(1))
    return variants


def fn_body(rel, ty, fn):
    """Body of `fn` inside `impl ty { ... }`; None if either is absent.

    Located in the blanked CODE view, returned FROM the view that still carries
    the string literals — `arms_exact`'s whole subject is those literals, and a
    blanked body would report every vocabulary as empty (a false green of the
    worst kind). The two views have identical offsets by construction.
    """
    code, raw = rust_code(rel), rust_view(rel)
    for match in re.finditer(r"\bimpl\s+" + re.escape(ty) + r"\s*\{", code):
        code_block = brace_body(code, match.end() - 1)
        raw_block = brace_body(code, match.end() - 1, source=raw)
        if code_block is None or raw_block is None:
            continue
        inner = re.search(r"\bfn\s+" + re.escape(fn) + r"\b[^{;]*\{", code_block)
        if inner is None:
            continue
        body = brace_body(code_block, inner.end() - 1, source=raw_block)
        if body is not None:
            return body
    return None


_STRING_LIT = r'"(?:[^"\\]|\\.)*"'
_STRING_CAP = r'"((?:[^"\\]|\\.)*)"'


def arm_pairs(body, ty):
    """Every (variant, string) match arm in `body`, both directions, sorted.

    A LIST, not a dict: two arms mapping the same variant to two different
    spellings is itself a vocabulary that is not closed, and collapsing them
    into a dict would hide exactly that.

    OR-PATTERNS are expanded (fix-2, codex §9 round 2 finding #3): in
    `"vector" | "searchable" => Some(Ty::Searchable)` only the LAST alternative
    sits immediately before the `=>`, so a single-literal pattern harvested the
    pinned pair and reported the vocabulary as exact while the arm accepted a
    second token. Each alternative now yields its own pair, in BOTH directions.
    """
    who = r"(?:Self|" + re.escape(ty) + r")"
    variant = who + r"::[A-Za-z_]\w*"
    pairs = []
    # [Ty::A |] Ty::B => "s"
    for match in re.finditer(
        r"((?:" + variant + r"\s*\|\s*)*" + variant + r")\s*=>\s*" + _STRING_CAP, body
    ):
        for name in re.findall(who + r"::([A-Za-z_]\w*)", match.group(1)):
            pairs.append((name, match.group(2)))
    # ["a" |] "b" => Some(Ty::A)
    for match in re.finditer(
        r"((?:" + _STRING_LIT + r"\s*\|\s*)*" + _STRING_LIT + r")\s*=>\s*Some\("
        + who + r"::([A-Za-z_]\w*)\)",
        body,
    ):
        for spelling in re.findall(_STRING_CAP, match.group(1)):
            pairs.append((match.group(2), spelling))
    return sorted(pairs)


def body_strings(body):
    """Every string literal appearing anywhere in `body`.

    The NEGATIVE half of the closed-vocabulary check (fix-2, codex §9 round 2
    finding #3). Expanding or-patterns closes ONE extra syntax; it does not close
    the general case, because a token can be admitted by syntax that never forms
    a match arm at all — an `if value == "pending" { return Some(..) }` guard
    before the match, a `matches!`, a `starts_with`. Rather than enumerate those
    forms (an open set — the same asymptote this gate has now been asked to chase
    twice), the check is inverted: inside a function whose whole job is to map a
    CLOSED vocabulary, EVERY string literal must be one of the pinned spellings.

    Consequence, stated plainly: a legitimate non-vocabulary literal added to one
    of these three tiny functions (an error message, say) turns the gate RED. It
    is a false RED — the safe side of a publish-precondition gate, and the same
    trade already documented for the SQL text probes. What it is NOT is the
    silent green it replaces.
    """
    return [m.group(1) for m in re.finditer(_STRING_CAP, body)]


# ---------------------------------------------------------------------------
# SUBJECT-BOUND READERS (fix-4). Each locates THE NAMED SUBJECT and hands back
# its text, so the clause assertion is evaluated against that subject rather than
# against the whole file. Everything they cannot locate is reported as a CLAUSE
# DEFECT (exit 1), never skipped — the same stance `enum_exact` has taken since
# fix-1.
# ---------------------------------------------------------------------------
def rust_items(rel, kw, name):
    """Brace bodies of every `<kw> <name>` item declared in a Rust source file.

    `kw` is one of struct / enum / impl / fn. LOCATED in the blanked CODE view
    (fix-4b — a `{ }` inside a string literal is not a declaration) and RETURNED
    from the view that still carries the literals, because some probes are about
    a literal inside the item (`DenseReadiness::Ready => "ready",`). A body that
    cannot be brace-matched is dropped, which surfaces as "could not locate" at
    the call site — i.e. RED, not a silent pass.
    """
    code, raw = rust_code(rel), rust_view(rel)
    bodies = []
    for match in re.finditer(r"\b" + kw + r"\s+" + re.escape(name) + r"\b[^{;]*\{", code):
        body = brace_body(code, match.end() - 1, source=raw)
        if body is not None:
            bodies.append(body)
    return bodies


def fn_signatures(rel, name):
    """The signature text (params + return type) of every `fn <name>`."""
    code = rust_code(rel)
    return [m.group(1) for m in
            re.finditer(r"\bfn\s+" + re.escape(name) + r"\b([^{;]*)\{", code)]


def paren_body(text, open_index):
    """Body of the parenthesised group whose '(' sits at open_index."""
    depth = 0
    for i in range(open_index, len(text)):
        char = text[i]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[open_index + 1:i]
    return None


def table_defs(text, table):
    """(line, body) for every `CREATE [VIRTUAL] TABLE [<schema>.]<table> ( ... )`.

    Case-insensitive, `IF NOT EXISTS`-tolerant and schema-qualifier-tolerant, for
    the same reasons the negative SQL probes are (fix-1/fix-2): this reads SQLite
    DDL, not one hand-picked spelling of it. A table legitimately created more
    than once by the forward-only migration ladder yields several definitions,
    and the caller decides what that means.
    """
    pattern = (
        r"(?is)\bCREATE\s+(?:VIRTUAL\s+)?TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?"
        + qualified(table) + _SQL_IDENT_CLOSE
        + r"\s*(?:USING\s+[A-Za-z_][A-Za-z0-9_]*\s*)?\("
    )
    defs = []
    for match in re.finditer(pattern, text):
        body = paren_body(text, match.end() - 1)
        if body is not None:
            defs.append((line_of(text, match), body))
    return defs


# FTS5 spells its tokenizer as `tokenize = <string>`, and SQLite accepts the
# string quoted with ' " ` or [ ]. The VALUE is a whitespace-separated argument
# list whose tokenizer name SQLite matches case-insensitively, so comparison is
# on the whitespace-collapsed, lowercased form: `PORTER  unicode61` and
# `porter unicode61` are the same tokenizer, and reporting them as different
# would be a false RED with no content.
_TOKENIZE = re.compile(
    r"(?is)\btokenize\s*=\s*(?:'([^']*)'|\"([^\"]*)\"|`([^`]*)`|\[([^\]]*)\])"
)


def tokenize_clauses(body):
    """Every `tokenize=` value declared in an fts5 definition body, normalised."""
    found = []
    for match in _TOKENIZE.finditer(body):
        value = next(g for g in match.groups() if g is not None)
        found.append(" ".join(value.split()).lower())
    return found


def run_probe(probe):
    """Return a human-readable defect string, or None when the probe holds."""
    kind = probe[0]
    if kind in ("present", "doc_text"):
        _, path, pattern = probe
        # fix-4c. A `present` probe names a DECLARATION, so in a Rust source it
        # reads the COMMENT-STRIPPED view: commenting a declaration out and
        # putting a replacement beside it used to leave the probe satisfied, and
        # for two clauses that probe is the only one carrying the obligation. A
        # `doc_text` probe is the exact opposite — its subject IS a written
        # statement, and two of them live in ordinary `//` comments — so it
        # always reads raw text. Non-Rust subjects (the plan's markdown table)
        # read raw too: there is no Rust comment syntax there to strip.
        if kind == "present" and path.endswith(".rs"):
            text = rust_view(path)
        else:
            text = read_source(path)
        if re.search(pattern, text) is None:
            if kind == "doc_text":
                return (
                    f"the DOCUMENTED STATEMENT /{pattern}/ is no longer present in {path}. This "
                    "probe asserts a written statement, not a structure — the clause's enforceable "
                    "content is carried by its other probes; see the note beside this clause"
                )
            return (
                f"expected /{pattern}/ in {path}, found 0 match(es)"
                + (" (comments are stripped before this probe runs: a commented-out declaration "
                   "is not a declaration)" if path.endswith(".rs") else "")
            )
        return None
    # ---- the fix-4 SUBJECT-BOUND kinds ------------------------------------
    if kind == "in_item":
        _, path, kw, name, pattern = probe
        bodies = rust_items(path, kw, name)
        if not bodies:
            return (
                f"could not locate a parseable `{kw} {name} {{ .. }}` in {path} — the pinned "
                f"obligation /{pattern}/ is about THAT item, so it cannot be checked, and this "
                "reads as a clause failure rather than a skipped probe"
            )
        # An inherent `impl` may legitimately be split across blocks, so ANY block
        # satisfies it — every one of them is still `impl <the named type>`. A
        # struct/enum/fn name denoting several items is ambiguous, so EVERY body
        # must satisfy the probe (the strict side).
        hits = [b for b in bodies if re.search(pattern, b) is not None]
        ok = bool(hits) if kw == "impl" else len(hits) == len(bodies)
        if not ok:
            return (
                f"expected /{pattern}/ INSIDE `{kw} {name}` in {path}; matched {len(hits)} of "
                f"{len(bodies)} such item(s). The contract makes this statement ABOUT "
                f"`{name}` — a match elsewhere in the file is a different subject"
            )
        return None
    if kind == "fn_sig":
        _, path, name, pattern = probe
        sigs = fn_signatures(path, name)
        if not sigs:
            return (
                f"could not locate `fn {name}(..)` in {path} — the pinned signature obligation "
                f"/{pattern}/ cannot be checked, so this reads as a clause failure rather than a "
                "skipped probe"
            )
        missing = [s for s in sigs if re.search(pattern, s) is None]
        if missing:
            return (
                f"expected /{pattern}/ in the SIGNATURE of `fn {name}` in {path}; "
                f"{len(missing)} of {len(sigs)} definition(s) do not carry it. The contract "
                "describes THIS verb's parameters and return type — the same text on another "
                "function is a different subject"
            )
        return None
    if kind == "fn_defined":
        _, path, name = probe
        if not rust_items(path, "fn", name):
            return (
                f"`fn {name}` has no DEFINITION in {path}. The contract's proof of this "
                "obligation is that named function/test; a comment or doc reference that "
                "mentions the name is not the proof (the source is read comment-stripped, and a "
                "parseable body is required)"
            )
        return None
    if kind == "sql_ddl":
        _, path, table, pattern = probe
        defs = table_defs(read_source(path), table)
        if not defs:
            return (
                f"no `CREATE [VIRTUAL] TABLE {table} ( .. )` definition found in {path} — the "
                f"pinned obligation /{pattern}/ is about that table's SHAPE, so it could not be "
                "checked; reported as a clause failure, not a skipped probe"
            )
        missing = [line for line, body in defs if re.search(pattern, body) is None]
        if missing:
            return (
                f"expected /{pattern}/ INSIDE the definition of table `{table}` in {path}; "
                f"{len(missing)} of {len(defs)} definition(s) (line(s) {missing}) do not carry "
                "it. The contract states this of THAT table — the same column text in another "
                "table is a different subject"
            )
        return None
    if kind == "fts_tokenizer_shared":
        _, path, subject, refs, pinned = probe
        text = read_source(path)
        want = " ".join(pinned.split()).lower()
        seen = {}
        for table in (subject,) + tuple(refs):
            defs = table_defs(text, table)
            if not defs:
                return (
                    f"no `CREATE VIRTUAL TABLE {table} ( .. )` definition found in {path} — this "
                    "clause COMPARES the property-FTS tokenizer against the body-FTS tokenizer, "
                    "and one side of the comparison is missing, so it could not be evaluated"
                )
            values = sorted({v for _, body in defs for v in tokenize_clauses(body)})
            if not values:
                return (
                    f"table `{table}` in {path} declares NO `tokenize=` clause in any of its "
                    f"{len(defs)} definition(s). FTS5 then silently uses its OWN default "
                    "(unicode61), which is not the engine default this clause names"
                )
            if len(values) > 1:
                return (
                    f"table `{table}` in {path} is created with CONFLICTING tokenizers {values} — "
                    "the shipped tokenizer of that table is ambiguous, so the comparison this "
                    "clause requires cannot be made"
                )
            seen[table] = values[0]
        distinct = sorted(set(seen.values()))
        if len(distinct) > 1:
            return (
                f"the property-FTS table `{subject}` does NOT share the body-FTS tokenizer: "
                + ", ".join(f"{t} = '{v}'" for t, v in seen.items())
                + ". The clause requires the ENGINE'S DEFAULT FTS5 TOKENIZER — the one body-FTS "
                "uses — for the property FTS"
            )
        if distinct[0] != want:
            return (
                f"`{subject}` and the body-FTS table(s) {sorted(refs)} in {path} agree on "
                f"tokenizer '{distinct[0]}', but the pinned engine default is '{want}'. The "
                "registry was derived from a contract that names that default: a global "
                "tokenizer change is a contract-relevant change, not a re-pin"
            )
        return None
    # NOTE (fix-3): there is deliberately NO file-scoped negative probe kind. A
    # negative assertion scoped to a single file asserts only "nobody added a
    # module", which a growing crate falsifies by default — codex §9 round 3.
    # `absent_tree` below is the only negative kind, and `("absent", <file>, ..)`
    # now falls through to the unknown-kind die_env at the bottom of this fn.
    #
    # NOTE (fix-4): and there is deliberately NO COUNT kind. `("min", path,
    # regex, n)` counted file-wide matches, which binds a probe to a file rather
    # than to the subject its clause is about — codex §9 round 4. It too falls
    # through to the unknown-kind die_env.
    if kind == "absent_tree":
        _, tree, pattern, exts = probe
        scope = "every text file" if exts is None else "files matching " + "/".join(exts)
        for path, text in walk_tree(tree, exts):
            match = re.search(pattern, text)
            if match is not None:
                return (
                    f"expected NO match for /{pattern}/ in {scope} under {tree}/, but found it at "
                    f"{path}:{line_of(text, match)}"
                )
        return None
    if kind == "enum_exact":
        _, path, ty, expected = probe
        want = list(expected)
        got = enum_variants(path, ty)
        if got is None:
            return (
                f"could not locate a parseable `enum {ty}` declaration in {path} — the pinned "
                f"vocabulary {want} cannot be checked, so this reads as a clause failure rather "
                "than a skipped probe"
            )
        if sorted(got) != sorted(want):
            extra = [v for v in got if v not in want]
            missing = [v for v in want if v not in got]
            return (
                f"enum {ty} in {path} must carry EXACTLY {len(want)} variant(s) {want}, found "
                f"{len(got)} {got}"
                + (f"; UNPINNED variant(s) {extra}" if extra else "")
                + (f"; MISSING variant(s) {missing}" if missing else "")
                + " — this vocabulary is CLOSED by the contract, not merely non-empty"
            )
        return None
    if kind == "arms_exact":
        _, path, ty, fn, expected = probe
        want = sorted(tuple(pair) for pair in expected)
        body = fn_body(path, ty, fn)
        if body is None:
            return (
                f"could not locate `impl {ty} {{ fn {fn}(..) }}` in {path} — the pinned string "
                f"vocabulary {want} cannot be checked, so this reads as a clause failure rather "
                "than a skipped probe"
            )
        defects = []
        got = arm_pairs(body, ty)
        if got != want:
            extra = [p for p in got if p not in want]
            missing = [p for p in want if p not in got]
            defects.append(
                f"{ty}::{fn} in {path} must map EXACTLY {len(want)} (variant, string) pair(s) "
                f"{want}, found {len(got)} {got}"
                + (f"; UNPINNED pair(s) {extra}" if extra else "")
                + (f"; MISSING pair(s) {missing}" if missing else "")
            )
        # The NEGATIVE half (fix-2): a token can be admitted by syntax that never
        # forms a match arm, so the arm harvest alone can report a vocabulary as
        # exact while the function accepts more. Every string literal in the body
        # must be a pinned spelling.
        pinned_spellings = {pair[1] for pair in want}
        unpinned = sorted({s for s in body_strings(body) if s not in pinned_spellings})
        if unpinned:
            defects.append(
                f"{ty}::{fn} in {path} contains string literal(s) {unpinned} that are NOT in the "
                f"pinned vocabulary {sorted(pinned_spellings)}. This vocabulary is CLOSED by the "
                "contract, and a token can be ADMITTED without ever forming a `\"s\" => Some(..)` "
                "arm — an `if value == \"...\"` guard before the match, an or-pattern, a "
                "`matches!`. Every literal inside this function is therefore treated as part of "
                "the vocabulary: if the extra literal is genuinely not a spelling (an error "
                "message, say), move it out of this function rather than widening this check."
            )
        if defects:
            return "\n        - ".join(defects)
        return None
    die_env(f"internal error: unknown probe kind {kind!r}")


obligations = {c["id"]: c["obligation"] for c in pin["clauses"]}
evidence = {c["id"]: c.get("evidence", []) for c in pin["clauses"]}

executed = 0
for clause_id in sorted(ASSERTIONS):
    defects = [d for d in (run_probe(p) for p in ASSERTIONS[clause_id]) if d is not None]
    executed += 1
    if defects:
        cited = ", ".join(evidence.get(clause_id, [])) or "(no evidence recorded)"
        fail(
            f"clause {clause_id} FAILS: as-built code no longer satisfies the pinned contract "
            f'obligation "{obligations[clause_id]}".\n'
            + "".join(f"        - {d}\n" for d in defects)
            + f"        pinned evidence: {cited}"
        )

# TC-37 path #5: the check set must not evaporate. `executed` counts assertions
# actually run; if fewer ran than the pin registers as CHECKABLE, this gate is
# reporting on less than it claims and must not produce a verdict.
if executed < pin["counts"]["checkable"]:
    die_env(
        f"executed {executed} clause assertion(s) but the pin registers "
        f"{pin['counts']['checkable']} CHECKABLE clause(s) — the check set EVAPORATED (TC-37 "
        "evaporation path #5). A gate that silently checks less than it claims is worse than no "
        "gate."
    )

if failures:
    print(
        "\n"
        "  R-20-H7 (`can-i-deploy`) is RED: as-built FathomDB code, or the ratified C-1 contract\n"
        "  itself, no longer matches what was pinned. An absent-or-failing gate HOLDS the 0.8.20 ↔\n"
        "  Memex breaking pair (plan-0.8.20.md §3, HITL-directed 2026-07-10) — so this blocks the\n"
        "  publish, by design.\n"
        "\n"
        "  DO NOT re-pin to make this pass.\n"
        "    * If a CLAUSE failed: as-built code drifted away from a ratified CROSS-REPO contract.\n"
        "      Fix the code, or take the contract back through amendment. Reclassifying the clause\n"
        "      to PROSE to clear the gate is the failure mode this gate exists to prevent.\n"
        "    * If the CONTRACT moved: the clause registry was derived from the EXACT text recorded\n"
        "      in the pin, so it is no longer known to describe the document. The registry must be\n"
        "      RE-DERIVED from the new text — every clause re-classified CHECKABLE / CROSS-REPO /\n"
        "      PROSE — and re-pinned under review. Recomputing the hash alone is a lie.\n"
        "\n"
        "  PRECEDENT (why this is not paranoia): efa8d584 amended Q6(b) because the un-amended\n"
        "  clause mandated an anonymous surrogate that the ratified TC-11 pin A forbids ever\n"
        "  implementing. A gate written against the un-amended text would have been PERMANENTLY\n"
        "  RED and would have held the publish forever. An un-reconciled contract edit is exactly\n"
        "  how this gate becomes either permanently red or silently vacuous.\n"
        "\n"
        "  Take it to the Steward / HITL."
    )
    sys.exit(1)

print(
    f"ok    c1-contract-conformance: {CONTRACT} matches the pin and all "
    f"{pin['counts']['checkable']} CHECKABLE clause(s) hold "
    f"({pin['counts']['checkable']} checkable / {pin['counts']['cross_repo']} cross-repo / "
    f"{pin['counts']['prose']} prose, {pin['counts']['total']} total, {WHERE})"
)
PY
RC=$?
set -e

exit "$RC"
