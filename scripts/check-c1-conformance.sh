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
#   * for CLOSED VOCABULARIES ("EXACTLY {unavailable, embedding, ready}", "exactly three
#     roles", "total over exactly those three"), that the enum's variant SET and
#     COUNT and its string mappings are exactly the pinned ones — a structural
#     comparison, NOT a blacklist of the names somebody happened to fear; and
#   * for BEHAVIOURAL obligations, that THE NAMED TEST WHICH PROVES THAT
#     BEHAVIOUR STILL EXISTS IN THE TREE **AND IS STILL AN ACTIVE TEST** —
#     carrying a `#[test]`-family attribute, and not disabled by `#[ignore]` or
#     `#[cfg(..)]` (fix-5).
#
# It does NOT re-execute those behavioural suites, and it therefore does NOT
# prove the behaviour is still correct — only that the proof has not been
# deleted, renamed away, or SWITCHED OFF. `cargo test --workspace` is what
# proves the behaviour; this gate is what notices when the proof disappears,
# stops running, or when the contract's structural obligations are no longer met.
#
# So: it CATCHES a symbol/table/variant that was renamed or removed, a
# forbidden symbol that reappeared, a named test that was deleted OR quietly
# disabled, and any edit to the ratified contract. It DOES NOT catch a
# behavioural regression inside a still-present, still-named, still-running
# test, nor anything the Memex repo does. Claiming otherwise would be a false
# assurance — which is the TC-37 failure class in a different costume.
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
# ==================== RESIDUAL SCOPE — READ BEFORE ROUND 6 ===================
# This gate is a STATIC, LEXICAL check over Rust and SQL SOURCE TEXT. That
# approach has a boundary, and two review rounds were spent tightening regexes
# against it (fix-1: SQL case/whitespace, and blacklist ⇒ closed vocabulary;
# fix-2: schema-qualified table names, and spellings admitted outside a simple
# match arm). Rather than rediscover the boundary again, it is written down here.
#
# NOTE THAT FIX-3, FIX-4 AND FIX-5 ARE NOT ON THIS LIST AND DID NOT BELONG ON IT.
# Round 3 was a bounded SCOPE bug (one file vs the crate tree), round 4 a bounded
# SUBJECT-BINDING bug (a file vs the named table/struct/enum/function the clause
# is about), and round 5 two more bounded ones — the WRONG CRATE'S same-named
# symbol, and a test function that is no longer a test. All three had definite,
# complete fixes and all three were CLOSED, not documented away. The list below
# is the set of things a different MECHANISM would be needed for; it is not a
# place to retire findings to.
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
#      assert only that the NAMED TEST still exists and is still ENABLED, not
#      that it still proves anything, and 12 contract clauses are CROSS-REPO by
#      classification and unverifiable from this repo at all.
#
#   8. WHAT A NAMED TEST'S BODY ACTUALLY ASSERTS. The strongest form of #7, and
#      REWRITTEN AT FIX-5 because the version written at fix-4 claimed more
#      residual than is now true. An over-broad residual is its own dishonesty:
#      it retires a defect that the gate does in fact catch.
#
#      WHAT IS NOW ENFORCED, by `test_defined`, at all four levels a proof can be
#      switched off at:
#        THE FUNCTION   it EXISTS as a definition with a parseable body (not a
#                       comment, not a string literal, not a call); it CARRIES a
#                       `#[test]`-family attribute; it is NOT disabled by
#                       `#[ignore]` / `#[cfg(..)]` / `#[cfg_attr(..)]`.
#        ITS POSITION   it sits at the TOP LEVEL of the integration-test file, so
#                       no enclosing (possibly `#[cfg]`-gated) item can silence it
#                       out of this reader's view.
#        THE FILE       the file's own LEADING HEADER carries no `#![cfg(..)]`
#                       inner attribute, which would switch every proof in it off
#                       at once. Read at the header and nowhere else (fix-6): an
#                       inner attribute inside a nested `mod` belongs to THAT
#                       module, and reading it as the file's was a false RED.
#        THE TARGET     the owning crate's Cargo.toml neither sets
#                       `autotests = false` nor declares a `[[test]]` block for
#                       that file with `required-features` / `test = false`.
#
#      So "delete the proof", "rename it away", "leave only a comment mentioning
#      it", "drop the `#[test]`", "add an `#[ignore]`", "gate the fn", "gate the
#      module", "gate the file" and "gate the target" are ALL closed. The middle
#      two were codex §9 round 5 finding #2; the last four came out of turning
#      that same question on this patch. The TARGET one is not hypothetical:
#      fathomdb-engine/Cargo.toml already carries nineteen `[[test]]` blocks with
#      `required-features`, so it is an established pattern in the very file a
#      contributor would edit.
#
#      WHAT REMAINS, precisely, is three things and no more:
#        (a) THE BODY'S ASSERTIONS. `fn atomic_flip_never_exposes_ready_without
#            _the_vector_under_concurrent_write() { assert!(true); }` satisfies
#            every check above, and so does one whose assertions were commented
#            out. No lexical check can decide whether a body proves the obligation
#            its name claims — `cargo test --workspace` EXECUTING it is what does,
#            it is a required gate, and an emptied body is a visible code diff.
#        (b) THE CRATE, AND THE INVOCATION. This gate reads source and manifest
#            TEXT. It does not know whether the crate is still a workspace member,
#            nor how CI actually invokes cargo (a `--exclude`, a filter argument,
#            a job that stops running).
#        (c) `#[cfg]` INSIDE the body — an assertion compiled out while the test
#            still runs and passes vacuously. This is class #6 again.
#      ALL THREE need the same different mechanism, and it is one mechanism: ask
#      the TOOLCHAIN what the built test binaries actually contain and what they
#      did (`cargo test -- --list`, or `--no-run --message-format=json`), instead
#      of reading text. See the RECOMMENDATION in the closure JSON.
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
# it misses it because the probe was pointed at the wrong SCOPE, that a probe is
# satisfied by something OTHER THAN THE SUBJECT ITS CLAUSE NAMES, that it is
# pointed at a DIFFERENT CRATE'S same-named symbol, or that a "proof still
# exists" probe is satisfied by a proof that no longer RUNS, is a real defect:
# fix it, and if the fix is another regex, add its class here. Round 3 was the
# second kind (see PROBE SCOPE above), round 4 the third and round 5 the fourth
# and fifth (see A PROBE IS BOUND TO ITS SUBJECT and THE SUBJECT IS A CRATE, NOT
# A SYMBOL NAME, beside the assertion table); all were fixed.
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


def crate_manifest_for(test_path):
    """The `Cargo.toml` of the crate owning a `<crate>/tests/<file>.rs` path.

    Defined here rather than beside its reader because `--list-sources` needs it:
    a `test_defined` probe reads that manifest too, and a path missing from the
    manifest is a fixture root missing a file, i.e. a silent TC-37 #4.
    """
    marker = "/tests/"
    index = test_path.find(marker)
    if index < 0:
        return None
    return test_path[:index] + "/Cargo.toml"

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
#                                      parseable body (not a comment, not a call).
#                                      PRODUCTION code only — the gate REFUSES to
#                                      run this kind against a `tests/` path
#                                      (fix-5); use `test_defined` there
#   ("test_defined", path, fn)         the same, PLUS `fn` is an ACTIVE TEST at
#                                      all four levels it could be switched off
#                                      at: a `#[test]`-family attribute with no
#                                      `#[ignore]`/`#[cfg]`; at the file's top
#                                      level; in a file whose own LEADING HEADER
#                                      carries no `#![cfg(..)]` (a nested module's
#                                      inner attribute is that module's, fix-6);
#                                      in a crate whose Cargo.toml does not gate
#                                      that `[[test]]` target. "The proof still
#                                      exists" is worth nothing if the proof no
#                                      longer RUNS (fix-5)
#   ("const_str_agree", ((path, const), ..), pinned)
#                                      every named `const/static <const>: &str`
#                                      is DECLARED in its own file, all of them
#                                      carry the SAME string literal, and that
#                                      value equals `pinned`. For an obligation
#                                      whose subject is a value TWO CRATES must
#                                      agree on (fix-5)
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
# vocabulary is EXACTLY some set: readiness is
# "EXACTLY {unavailable, embedding, ready}",
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
#
# ------------ THE SUBJECT IS A CRATE, NOT A SYMBOL NAME (fix-5) --------------
# codex §9 round 5 finding #1 [P2]. A REFINEMENT of the fix-4 class, and a
# different question from the one fix-4's own audit asked. fix-4 asked "is this
# probe bound to a SUBJECT at all, or merely to a file?". Round 5 asks the next
# question: "is it bound to the subject THE CLAUSE NAMES, or to a DIFFERENT FILE
# THAT HAPPENS TO CONTAIN A SIMILAR SYMBOL?" — and a probe can be perfectly,
# structurally bound and still be bound to the wrong thing.
#
# The instance: `C1-TE-DEFAULT-EMBEDDER`'s obligation is "the default embedder is
# THE ENGINE'S SHIPPED DEFAULT". The probe read `DEFAULT_EMBEDDER_NAME` out of
# the EMBEDDER crate's `candle_bge.rs`. Both crates declare a constant of that
# exact name; only the engine's is the engine's shipped default. codex changed
# `fathomdb-engine/src/lib.rs`'s `DEFAULT_EMBEDDER_NAME` to `"some-other-model"`
# and the gate exited 0.
#
# THE FIX is not to swap one crate for the other — that would drop a real half of
# the obligation. The two constants MUST AGREE: the engine fails closed on an
# embedder identity mismatch (ADR-0.6.0-vector-identity-embedder-owned), so a
# divergence between "the name the engine expects by default" and "the name the
# pinned embedder reports" is a broken shipped default, not a cosmetic drift.
# `const_str_agree` therefore asserts the RELATION, exactly as
# `fts_tokenizer_shared` does for the sibling tokenizer clause: both constants
# must exist as declarations, must carry the same string, and that string must be
# the pinned one.
#
# THE FIX-5 RE-AUDIT of all 26 CHECKABLE clauses against this question found NO
# second instance — every other probe's file is the crate the clause names (see
# the closure JSON for the clause-by-clause table). One clause is deliberately
# WIDER than its own wording: `C1-Q4-NO-PROVISIONAL-CONCEPT` says "in engine
# code" and is scanned over the whole of `src/`. That is the SAFE direction for a
# negative (it can only produce a false RED), and it is what the pin's own
# recorded evidence asserts, so it stays.
#
# ---------------- A DISABLED TEST IS NOT A PROOF (fix-5) --------------------
# codex §9 round 5 finding #2 [P2], and the TC-37 evaporation shape this whole
# gate exists to close. Ten clauses are carried by "the named test that proves
# this still exists in the tree". `fn_defined` proved a DEFINITION existed — so
# DELETING `#[test]`, or ADDING `#[ignore]`, left a plain function of the same
# name, `fn_defined` still passed, and `cargo test` silently stopped running the
# proof. Nothing anywhere went red.
#
# That is strictly worse than residual class #8 as fix-4 stated it. #8's claim
# was "an emptied body is review-visible" — true, and an emptied body genuinely
# is a diff a human reads. But a deleted attribute is a ONE-TOKEN edit that looks
# like tidy-up, and unlike an emptied body it IS closeable lexically. So it was
# closed, for ALL FOURTEEN test probes across the ten behavioural clauses.
# `fn_defined` survives ONLY for the four PRODUCTION functions, and the gate now
# REFUSES (exit 2, internal error) to evaluate `fn_defined` against a path under
# `tests/` so the weaker kind cannot drift back onto a proof.
#
# AND THEN THE SAME QUESTION, TURNED ON THIS PATCH: `test_defined` reads the
# attributes ON THE FUNCTION, so a proof can carry a pristine `#[test]` and still
# never run because something ABOVE it was switched off. Three more routes, all
# lexical, all of which exited 0 against the first cut of this fix:
#   * the proof NESTED inside a `#[cfg]`-gated `mod` (attributes on the module,
#     not on the fn);
#   * a `#![cfg(..)]` INNER attribute switching the whole FILE off;
#   * a `[[test]]` block in the crate's Cargo.toml carrying `required-features`
#     (or `test = false`, or `autotests = false`), which gates the whole TARGET.
# The last is the important one, and it is NOT hypothetical: fathomdb-engine's
# Cargo.toml ALREADY declares nineteen such blocks, because that is how the crate
# legitimately keeps operator-only and reranker-only suites out of the default
# build. "Somebody adds a required-features to a test target" is an established
# pattern in the exact file a contributor would edit — the fix-3 sibling-module
# argument, one level up. All four levels are checked now.
#
# `#[cfg(..)]` is rejected on a proof, on its module and on its file
# deliberately, and it is the judgement call here: a legitimately feature-gated
# proof goes RED. That is the safe side (a false RED a human resolves), and it
# matches the bias stated under WHICH WAY THE ERRORS FALL. What it must NOT be
# mistaken for is completeness — see residual class #8 for exactly the three
# things that remain.
#
# NO PYTHON OR TYPESCRIPT TEST IS CITED BY ANY CLAUSE. `--list-sources` is the
# authority: the gate reads seven files (four Rust, one markdown plan, and the
# two crate lib.rs) and three trees, and not one `.py` or `.ts` among them. The
# same evaporation exists in those languages (`@pytest.mark.skip`, `xfail`,
# `describe.skip` / `it.skip` / `.only`, a renamed-away `test_` prefix), so if a
# future clause ever cites one, it needs its own `test_defined` equivalent — not
# a `present` probe on a function name.
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
    #
    # fix-6 (codex §9 round 6 finding #1): reading them out of the signature is
    # still not enough, because a signature is not a method. Turn
    # `configure_projections` into a free or associated function — delete the
    # `&self,` line, nothing else — and all three fragments are still there
    # character for character while `engine.configure_projections(..)`, the
    # instance verb the pin's evidence records and every call site in this repo
    # uses, stops compiling. So the RECEIVER is probed too.
    "C1-SEAM-ENGINE-BUILD-DROP": [
        ("present", ENG, r"pub fn configure_projections\("),
        # A `self` receiver is legal ONLY inside an `impl`/trait block, so
        # requiring one excludes both shapes the finding names (a free function
        # and a receiver-less associated function) without this gate having to
        # decide which `impl` it landed in. Every legal receiver spelling is
        # admitted — `&self`, `& self`, `&mut self`, `&'a self`, `self`,
        # `mut self` — because a formatting-only edit must not manufacture a
        # false RED (`self` may only ever be the FIRST parameter, so the open
        # paren this anchors on can only be the parameter list's).
        ("fn_sig", ENG, "configure_projections",
         r"\(\s*(?:&\s*(?:'[A-Za-z_]\w*\s+)?)?(?:mut\s+)?self\b"),
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
        ("test_defined", T15, "destructive_change_requires_explicit_drop"),
    ],
    "C1-Q3-OMISSION-NOT-DROP": [
        ("test_defined", T15, "role_add_builds_and_explicit_drop_drops_exactly_one"),
        ("test_defined", T15, "dropping_an_absent_name_is_a_clean_noop"),
    ],
    # ---- Q5 --------------------------------------------------------------
    "C1-Q5-DERIVED-CACHE-IDEMPOTENT": [
        ("in_item", ENG, "struct", "ProjectionDelta", r"pub\s+unchanged\s*:\s*bool"),
        ("test_defined", T15, "idempotent_reregistration_is_a_noop"),
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
        ("test_defined", T15, "property_filter_returns_correct_rows"),
        ("test_defined", T15, "property_fts_search_returns_correct_rows"),
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
    # "EXACTLY {unavailable, embedding, ready}" is a CLOSED vocabulary, so it
    # is asserted structurally (fix-1, codex finding #1): the enum has exactly
    # three variants, and each conversion fn maps exactly three (variant, string)
    # pairs. The
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
        ("in_item", ENG, "impl", "DenseReadiness", r'DenseReadiness::Unavailable => "unavailable",'),
        ("in_item", ENG, "impl", "DenseReadiness", r'DenseReadiness::Ready => "ready",'),
        ("in_item", ENG, "impl", "DenseReadiness", r'DenseReadiness::Embedding => "embedding",'),
        ("absent_tree", ENG_TREE, r"DenseReadiness::Pending", (".rs",)),
        ("enum_exact", ENG, "DenseReadiness", ("Unavailable", "Embedding", "Ready")),
        ("arms_exact", ENG, "DenseReadiness", "as_str",
         (("Unavailable", "unavailable"), ("Embedding", "embedding"), ("Ready", "ready"))),
        ("arms_exact", ENG, "DenseReadiness", "from_str_opt",
         (("Unavailable", "unavailable"), ("Embedding", "embedding"), ("Ready", "ready"))),
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
        ("test_defined", T15, "rankable_is_graceful_deferred_never_blocking"),
        ("test_defined", T15, "idempotent_reregistration_holds_for_deferred_rankable"),
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
        ("test_defined", T25,
         "an_anonymous_write_stays_anonymous_through_the_whole_durable_path"),
    ],
    "C1-Q6B-SURROGATE-GOVERNED-ONLY": [
        ("test_defined", T25, "registering_projections_never_alters_a_pre_existing_row_id_space"),
        ("test_defined", T25, "the_internal_structural_row_writer_mints_no_logical_id"),
    ],
    # ---- Apply atomicity -------------------------------------------------
    # fix-4 SWEEP: every "the named thing still exists" probe reads the
    # COMMENT-STRIPPED source and requires a parseable body. The old `fn <name>(`
    # regex could not tell a definition from a doc comment mentioning the name —
    # so deleting the proof and leaving a reference to it behind (the most natural
    # thing a contributor does) exited 0.
    #
    # fix-5: and the PROOFS use `test_defined`, not `fn_defined` — a definition
    # that is no longer an ACTIVE test proves nothing (codex §9 round 5 #2). The
    # four `fn_defined` probes that remain in this table are all PRODUCTION
    # functions in the engine; the gate refuses to evaluate `fn_defined` against a
    # `tests/` path at all.
    "C1-AA-ATOMIC-FLIP": [
        ("fn_defined", ENG, "commit_projection_outcomes"),
        ("test_defined", T20,
         "atomic_flip_never_exposes_ready_without_the_vector_under_concurrent_write"),
    ],
    # fix-4 SWEEP: "the apply ENQUEUES and returns" is a statement about the apply
    # verb. `notify_new_work()` file-wide is satisfied by the engine's three
    # unrelated wake sites, so removing the wake FROM THE APPLY was invisible.
    "C1-AA-NO-BLOCK-ON-EMBEDDING": [
        ("fn_defined", ENG, "notify_new_work"),
        ("in_item", ENG, "fn", "configure_projections", r"notify_new_work\(\)"),
        ("test_defined", T20,
         "readiness_reads_embedding_while_embeds_are_outstanding_then_flips_to_ready"),
    ],
    "C1-AA-CRASH-HEAL-BOOT-REDERIVE": [
        ("fn_defined", ENG, "load_projection_registry"),
        ("test_defined", T15, "boot_rederive_converges_after_simulated_crash"),
    ],
    # ---- Tokenizer / embedder defaults -----------------------------------
    # fix-5, codex §9 round 5 finding #1 [P2] — THE FINDING. The obligation names
    # "the ENGINE'S SHIPPED DEFAULT", and the second probe read the EMBEDDER
    # crate's `candle_bge.rs` constant. Both crates declare a `DEFAULT_EMBEDDER_
    # NAME`; only the engine's is the shipped default. Flipping the ENGINE's to
    # `"some-other-model"` exited 0 — a probe perfectly bound to a subject, and to
    # the wrong one. See THE SUBJECT IS A CRATE, NOT A SYMBOL NAME above.
    #
    # Both constants are asserted, and asserted RELATIONALLY, because the
    # obligation really is about the pair: the engine fails closed on an embedder
    # identity mismatch, so engine-default != embedder-identity is a broken
    # shipped default. Same shape as the sibling tokenizer clause.
    "C1-TE-DEFAULT-EMBEDDER": [
        ("in_item", ENG, "struct", "ProjectionVector",
         r"pub\s+embedder\s*:\s*Option\s*<\s*String\s*>"),
        ("const_str_agree",
         ((ENG, "DEFAULT_EMBEDDER_NAME"), (EMB, "DEFAULT_EMBEDDER_NAME")),
         "fathomdb-bge-small-en-v1.5"),
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


# ------------------------- PROBE-TABLE SELF-CHECK (fix-5) --------------------
# Runs BEFORE anything else, --list-sources included, so it cannot be reached
# around. Its only job today is the one rule that a comment could not enforce:
# `fn_defined` must never be pointed at a TEST FILE. `fn_defined` accepts a
# function whose `#[test]` was deleted or whose `#[ignore]` was added — the exact
# round-5 #2 evaporation — so on a test path it is not a weaker probe, it is a
# broken one. `test_defined` is the probe for those, and this makes reaching for
# the wrong one a hard error rather than a code-review catch.
#
# Belt AND braces: `run_probe` refuses the same combination again at evaluation
# time. Two independent guards is the right cost for a gate that holds a publish;
# a future refactor that drops one still leaves the other standing.
def _is_test_path(path):
    return isinstance(path, str) and ("/tests/" in path or path.startswith("tests/"))


for _clause_id, _probes in sorted(ASSERTIONS.items()):
    for _probe in _probes:
        if _probe[0] == "fn_defined" and _is_test_path(_probe[1]):
            die_env(
                f"internal error: clause {_clause_id} probes the TEST file {_probe[1]} with "
                "`fn_defined`. A test's DEFINITION existing is not proof that the test still "
                "RUNS — deleting `#[test]` or adding `#[ignore]` leaves the definition untouched "
                "(codex §9 round 5 finding #2). Use `test_defined`."
            )


# --------------------------------------------------------- --list-sources ----
# The manifest is what BUILDS every fixture root in
# scripts/tests/test_check_c1_conformance.sh and the preflight seeder, so a probe
# kind whose path lives somewhere other than probe[1] must be handled EXPLICITLY
# here. Falling through would put a non-path (a tuple, say) in the file list, the
# fixture roots would lack a file the assertions read, and every source arm would
# quietly become a TC-37 path-#4 evaporation instead of a test. Anything that is
# not a string path is therefore an internal error, not a shrug (fix-5, which
# added the first multi-file probe kind).
if LIST_SOURCES:
    files, trees = set(), set()
    for probes in ASSERTIONS.values():
        for probe in probes:
            if probe[0] == "absent_tree":
                trees.add(probe[1])
            elif probe[0] == "const_str_agree":
                for path, _const in probe[1]:
                    files.add(path)
            else:
                files.add(probe[1])
                if probe[0] == "test_defined":
                    # `test_defined` also reads the owning crate's Cargo.toml, to
                    # see whether the test TARGET is gated out of a default
                    # `cargo test`. It must be in the manifest or every fixture
                    # root would lack it and the check would evaporate.
                    manifest = crate_manifest_for(probe[1])
                    if manifest is not None:
                        files.add(manifest)
    bad = sorted(p for p in (files | trees) if not isinstance(p, str))
    if bad:
        print(
            "check-c1-conformance: internal error: --list-sources produced "
            f"non-path entries {bad} — a probe kind reads its source from somewhere "
            "other than probe[1] and was not taught to this manifest. Fixture roots "
            "are built FROM this manifest, so shipping it would silently evaporate "
            "every arm that reads that file (TC-37 path #4)."
        )
        sys.exit(2)
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

    LENGTH- AND LINE-PRESERVING (fix-5c, found by turning the round-5 question on
    this patch). Comments are BLANKED, not deleted: the old version collapsed a
    multi-line `/* .. */` to a single space, so every offset and every LINE NUMBER
    downstream of one drifted. Nothing depended on that until fix-5 added a reader
    that CITES A LINE (`const_str_values`), and a failure message that names the
    wrong line in a gate whose whole value is actionable failures is a small lie.
    Blanking also means one index now means the same thing in the raw file, in
    this view and in `rust_code`.
    """
    if rel not in _view_cache:
        text = read_source(rel)
        text = re.sub(r"/\*.*?\*/",
                      lambda m: re.sub(r"[^\n]", " ", m.group(0)), text, flags=re.S)
        text = re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)
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


# ---------------------------------------------------------------------------
# ATTRIBUTE READING (fix-5). "The named proof still exists" is worth nothing if
# the proof no longer RUNS: deleting `#[test]` or adding `#[ignore]` leaves a
# plain function of the same name, which every `fn <name>` reader accepts while
# `cargo test` quietly stops executing it. That is the TC-37 evaporation shape
# this gate exists to close, so the ATTRIBUTES are read too.
#
# Everything below reads the blanked CODE view, so an attribute spelled inside a
# string literal or a comment is not an attribute — the same rule as every other
# reader here.
# ---------------------------------------------------------------------------
_FN_MODIFIERS = {"pub", "async", "unsafe", "const", "extern", "default"}


def _attrs_before(code, index):
    """Every `#[..]` attribute attached to the item whose keyword sits at `index`.

    Walks BACKWARDS over the item's modifiers (`pub`, `pub(crate)`, `async`,
    `unsafe`, `const`, `extern`) and then over any run of outer attributes.
    Returns them OUTERMOST-LAST; order does not matter to the caller. `#![..]`
    (an INNER attribute) terminates the walk: it belongs to the enclosing module,
    not to this item.

    Erring: anything this cannot parse simply ends the walk, which yields FEWER
    attributes and therefore a RED verdict — the safe side.
    """
    i = index
    # 1. back up over the fn's modifiers.
    while True:
        j = i
        while j > 0 and code[j - 1].isspace():
            j -= 1
        if j == 0:
            i = j
            break
        if code[j - 1] == ")":                      # pub(crate) / pub(in ..)
            depth, k = 0, j - 1
            while k >= 0:
                if code[k] == ")":
                    depth += 1
                elif code[k] == "(":
                    depth -= 1
                    if depth == 0:
                        break
                k -= 1
            word = re.search(r"([A-Za-z_]\w*)\s*$", code[:k]) if k >= 0 else None
            if word is not None and word.group(1) == "pub":
                i = word.start(1)
                continue
            i = j
            break
        word = re.search(r"([A-Za-z_]\w*)\s*$", code[:j])
        if word is not None and word.group(1) in _FN_MODIFIERS:
            i = word.start(1)
            continue
        i = j
        break
    # 2. consume the run of outer attributes above it.
    attrs = []
    while True:
        j = i
        while j > 0 and code[j - 1].isspace():
            j -= 1
        if j == 0 or code[j - 1] != "]":
            break
        depth, k = 0, j - 1
        while k >= 0:
            if code[k] == "]":
                depth += 1
            elif code[k] == "[":
                depth -= 1
                if depth == 0:
                    break
            k -= 1
        if k < 1 or code[k - 1] != "#":
            break
        if code[k + 1:k + 2] == "!":                # inner attribute: not ours
            break
        attrs.append(code[k - 1:j])
        i = k - 1
    return attrs


def attr_path(attr):
    """The dotted path an attribute names: `#[tokio::test]` -> 'tokio::test',
    `#[ignore = "why"]` -> 'ignore', `#[cfg(feature = "x")]` -> 'cfg'. None when
    the attribute is not a simple path (a bare literal, say)."""
    inner = attr[2:-1].strip()
    match = re.match(r"([A-Za-z_]\w*(?:\s*::\s*[A-Za-z_]\w*)*)", inner)
    if match is None:
        return None
    return "::".join(part.strip() for part in match.group(1).split("::"))


def file_inner_attrs(rel):
    """The `#![..]` INNER attribute paths in a Rust file's OWN LEADING HEADER.

    A `#![cfg(..)]` there switches the WHOLE FILE off, which for an integration
    test means every proof in it stops running while each one still reads as a
    perfectly ordinary `#[test]`.

    "IN THE HEADER" IS LOAD-BEARING (fix-6, codex §9 round 6 finding #2). This
    used to scan the whole file for `#![`, but Rust permits an inner attribute at
    the head of ANY braced item, and `mod helpers { #![cfg(feature = "x")] .. }`
    is ordinary, legal, and switches off THAT MODULE alone. Reading one of those
    as "the whole test file is conditionally compiled" turns this gate RED and
    blocks CI and `preflight.sh --landing` while every top-level pinned proof
    still builds and runs — a FALSE RED, which on a publish precondition is its
    own failure mode, not the safe side.

    So the walk starts at the top of the file and stops at the first thing that
    is neither whitespace nor an inner attribute — i.e. at the first item. That
    leading run IS the file header, and it is the only place a FILE-level inner
    attribute may legally appear, so narrowing to it gives up nothing: a genuine
    `#![cfg(..)]` is still found when it sits alone at the head, after other
    leading inner attributes, or after a `//!` doc header (comments, `//!`
    included, are already blanked to spaces by `rust_view`, so a doc header does
    not end the run).

    A LEADING SHEBANG IS PART OF THAT HEADER AND IS SKIPPED FIRST (fix-6b, codex
    §9 round 6 RE-REVIEW of the fix-6 patch — a regression the narrowing itself
    opened). A Rust source file may legally begin with `#!/usr/bin/env ..`, which
    is not `#![`, so the walk ended at byte 0 and never saw a file-level
    `#![cfg(..)]` on the NEXT line: the whole test file compiled out, gate green.
    `rustc --test` on shebang + `#![cfg(feature = "nope")]` + `#[test] fn t() {}`
    builds and `--list` reports `0 tests`. rustc's own rule is followed exactly:
    ONLY the very first line can be a shebang, and it is one only when the file
    starts with `#!` whose next character is NOT `[` — `#![..]` at byte 0 is an
    inner attribute, not a shebang. The rest of that first line is skipped and
    the leading-attribute walk resumes, so an attribute run after a shebang is
    still read (and the nested-module false RED above stays closed).

    A LEADING UTF-8 BOM IS ALSO PART OF THAT HEADER AND IS SKIPPED FIRST OF ALL
    (fix-6c, the COMPLETION of that same finding #2 — its scope is the file's own
    leading inner attributes, and a BOM-prefixed one is one of them). `read_source`
    decodes with `errors="replace"`, which does NOT strip a byte-order mark, so a
    BOM arrives here as U+FEFF at index 0 and defeated the walk three ways at once:
    it is not `#!`, so the shebang branch was skipped; `"﻿".isspace()` is False in
    Python 3 — U+FEFF is category Cf, a FORMAT character, not whitespace — so the
    whitespace skip did not advance past it; and it is not `#![`, so the walk broke
    at byte 0 and never saw a file-level `#![cfg(..)]` on that very line. Same false
    green as the shebang hole, through a different first byte: `rustc --test` on
    bytes `ef bb bf` + `#![cfg(feature = "nope")]` + `#[test] fn t() {}` builds and
    `--list` reports `0 tests`, while the same file without the cfg reports `1 test`.
    rustc's ordering is followed exactly — it strips ONE leading BOM, and only THEN
    may the first line be a shebang — so this skip runs BEFORE the shebang branch
    and BOM + shebang + `#![cfg(..)]` is read too. The strip is local to this walk;
    `read_source` is deliberately left alone so no other probe's view changes. Only
    ONE BOM is stripped, as rustc does: a second U+FEFF is not a header byte.
    """
    code = rust_code(rel)
    if code[:1] == "﻿":                     # ONE leading BOM, stripped as rustc does
        code = code[1:]
    found, i, n = [], 0, len(code)
    if code[:2] == "#!" and code[2:3] != "[":       # a shebang line, not an attr
        end = code.find("\n")
        i = n if end == -1 else end + 1
    while True:
        while i < n and code[i].isspace():
            i += 1
        if code[i:i + 3] != "#![":
            break
        depth, j = 0, i + 2
        while j < n:
            if code[j] == "[":
                depth += 1
            elif code[j] == "]":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        if j >= n:                       # unbalanced: unreadable, so the run ends
            break
        found.append(attr_path("#[" + code[i + 3:j] + "]"))
        i = j + 1
    return found


def fn_definitions(rel, name):
    """One entry per DEFINITION of `fn <name>`: (attribute paths, brace depth).

    A definition is a `fn <name> ... { .. }` with a brace-matchable body, located
    in the blanked CODE view. Same subject-binding rule as `rust_items`, but it
    keeps the ATTRIBUTES rather than the body, and the DEPTH of the enclosing
    braces — an integration test nested inside anything is not a top-level test
    target, and whatever encloses it may carry its own `#[cfg]` that this reader
    would never look at (fix-5 SWEEP).
    """
    code, raw = rust_code(rel), rust_view(rel)
    found = []
    for match in re.finditer(r"\bfn\s+" + re.escape(name) + r"\b[^{;]*\{", code):
        if brace_body(code, match.end() - 1, source=raw) is None:
            continue
        before = code[:match.start()]
        depth = before.count("{") - before.count("}")
        found.append(([attr_path(a) for a in _attrs_before(code, match.start())], depth))
    return found


# Minimal, deliberately dumb TOML block reader — the same stance as the Rust
# readers above, and for the same reason: hand-rolling it keeps this gate's
# environment contract at "requires python3", where `tomllib` would quietly
# require 3.11+ and turn an older runner into an exit-2 that nobody predicted.
# It reads ONE shape (`[[test]]` tables and their scalar keys) and treats
# anything it cannot read as a clause failure.
_TOML_TABLE = re.compile(r"(?m)^\s*(\[\[?[^\]]+\]\]?)\s*$")


def cargo_test_target_defect(manifest_rel, stem):
    """None, or why the test target `stem` does not run on a default `cargo test`.

    fix-5 SWEEP, and NOT a hypothetical: `fathomdb-engine/Cargo.toml` ALREADY
    carries nineteen `[[test]]` blocks with `required-features`, because that is
    how this crate legitimately keeps operator-only and reranker-only suites out
    of the default build. So "somebody adds a `required-features` to a test
    target" is an established pattern here, exactly as "somebody adds a module"
    was in fix-3 — and adding one for slice15d/slice20/slice25 would stop the
    pinned proofs running under `cargo test -p fathomdb-engine` while every
    attribute this gate reads stayed exactly as it is.

    That is codex §9 round 5 finding #2 one level up: the proof is still a
    `#[test]`, still un-`#[ignore]`d, and still never executed.
    """
    text = read_source(manifest_rel)
    stripped = re.sub(r"(?m)#.*$", "", text)
    if re.search(r"(?m)^\s*autotests\s*=\s*false", stripped):
        return (
            f"{manifest_rel} sets `autotests = false`, so integration-test files are no longer "
            "auto-discovered and only explicitly declared `[[test]]` targets are built. This "
            "gate cannot tell from source text whether the pinned proofs are still among them"
        )
    blocks, current, header = [], [], None
    for line in stripped.splitlines():
        match = _TOML_TABLE.match(line)
        if match:
            blocks.append((header, current))
            header, current = match.group(1).strip(), []
        else:
            current.append(line)
    blocks.append((header, current))
    for head, body in blocks:
        if head != "[[test]]":
            continue
        joined = "\n".join(body)
        name = re.search(r'(?m)^\s*name\s*=\s*"([^"]*)"', joined)
        if name is None:
            return (
                f"{manifest_rel} declares a `[[test]]` target with no readable `name = \"..\"`, so "
                "this gate cannot tell which test file it configures — and a target block is "
                "exactly where a pinned proof gets quietly gated out of the default build. "
                "Reported RED rather than guessed at"
            )
        if name.group(1) != stem:
            continue
        gate = re.search(r"(?m)^\s*required-features\s*=", joined)
        if gate is not None:
            return (
                f"{manifest_rel} declares `[[test]] name = \"{stem}\"` with `required-features`, so "
                "the WHOLE target — and every pinned proof in it — is skipped on a default "
                "`cargo test`. The proof's `#[test]` attribute is untouched and means nothing: "
                "this is the same evaporation one level up (this crate already gates nineteen "
                "operator/reranker suites exactly this way)"
            )
        if re.search(r"(?m)^\s*test\s*=\s*false", joined):
            return (
                f"{manifest_rel} declares `[[test]] name = \"{stem}\"` with `test = false`, so the "
                "target is excluded from `cargo test` entirely"
            )
    return None


def const_str_values(rel, name):
    """(line, value) for every `const`/`static <name>: &str = "..."` DECLARATION.

    Located in the blanked CODE view (a declaration inside a string literal is
    not a declaration, and `rust_view` has already dropped comments), with the
    value read back out of the literal-carrying view at the same offset — the two
    views have identical length by construction.

    `value` is None when the declaration exists but is NOT initialised from a
    plain string literal (a `concat!`, another const, an `include_str!`). The
    caller reports that as a clause failure: it is residual class #3 (identifier
    indirection) and the gate errs RED on it rather than guessing.
    """
    code, raw = rust_code(rel), rust_view(rel)
    pattern = (
        r"\b(?:pub\s*(?:\([^)]*\)\s*)?)?(?:const|static)\s+"
        + re.escape(name)
        + r"\s*:\s*&\s*(?:'[A-Za-z_]\w*\s+)?str\s*=\s*"
    )
    found = []
    for match in re.finditer(pattern, code):
        i = match.end()
        if i >= len(raw) or raw[i] != '"':
            found.append((line_of(code, match), None))
            continue
        j = i + 1
        while j < len(raw):
            if raw[j] == "\\":
                j += 2
                continue
            if raw[j] == '"':
                break
            j += 1
        found.append((line_of(code, match), raw[i + 1:j]))
    return found


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
        # fix-5. `fn_defined` is for PRODUCTION functions only. Pointing it at a
        # test is the round-5 #2 defect by construction — it accepts a function
        # whose `#[test]` was deleted — so the weaker kind is refused outright
        # rather than left as the path of least effort for the next editor.
        if "/tests/" in path or path.startswith("tests/"):
            die_env(
                f"internal error: the `fn_defined` probe is pointed at the test file {path}. A "
                "test's DEFINITION existing is not proof that the test still RUNS: deleting "
                "`#[test]` or adding `#[ignore]` leaves the definition untouched (codex §9 round "
                "5 finding #2). Use `test_defined`, which asserts the function is an ACTIVE test."
            )
        if not rust_items(path, "fn", name):
            return (
                f"`fn {name}` has no DEFINITION in {path}. The contract's proof of this "
                "obligation is that named function; a comment or doc reference that "
                "mentions the name is not the proof (the source is read comment-stripped, and a "
                "parseable body is required)"
            )
        return None
    if kind == "test_defined":
        _, path, name = probe
        definitions = fn_definitions(path, name)
        if not definitions:
            return (
                f"`fn {name}` has no DEFINITION in {path}. The contract's proof of this "
                "obligation is that named test; a comment, a doc reference or a mention inside a "
                "string literal is not the proof (the source is read comment-stripped, literals "
                "are blanked, and a parseable body is required)"
            )
        # THE WHOLE FILE, and THE WHOLE TARGET, must run too — a proof switched
        # off one or two levels up is switched off just the same, and neither is
        # visible in the function's own attributes (fix-5 SWEEP).
        gated_file = [a for a in file_inner_attrs(path)
                      if a and a.split("::")[-1] in ("cfg", "cfg_attr")]
        if gated_file:
            return (
                f"the test file {path} is CONDITIONALLY COMPILED as a whole "
                f"(inner attribute(s) {gated_file}), so `fn {name}` may never be built at all. "
                "A file-level `#![cfg(..)]` switches every proof in the file off while each one "
                "still reads as an ordinary `#[test]`"
            )
        manifest = crate_manifest_for(path)
        if manifest is not None:
            stem = os.path.basename(path)[:-3] if path.endswith(".rs") else os.path.basename(path)
            defect = cargo_test_target_defect(manifest, stem)
            if defect is not None:
                return defect
        # EVERY definition of that name must be an active test — a name denoting
        # several functions is ambiguous, and the strict side is the safe one.
        for attrs, depth in definitions:
            if depth != 0:
                return (
                    f"`fn {name}` in {path} is NESTED {depth} brace level(s) deep rather than at "
                    "the top level of the integration-test file. Whatever encloses it may carry a "
                    "`#[cfg(..)]` this reader never looks at, so its attributes alone no longer "
                    "establish that the proof runs. Reported RED rather than assumed"
                )
            present = [a for a in attrs if a]
            tail = [a.split("::")[-1] for a in present]
            if "test" not in tail:
                return (
                    f"`fn {name}` in {path} exists but carries NO `#[test]`-family attribute "
                    f"(it carries {present or 'no attributes at all'}). The clause's proof is a "
                    "TEST; a function that `cargo test` no longer runs proves nothing, and "
                    "deleting the attribute is a one-token edit that leaves the definition — and "
                    "every 'the proof still exists' check — looking untouched (TC-37 evaporation)"
                )
            if "ignore" in tail:
                return (
                    f"`fn {name}` in {path} is a test but is DISABLED by `#[ignore]`. `cargo "
                    "test` skips it, so the behavioural obligation this clause pins has no "
                    "running proof. Re-enable the test, or take the clause back through the "
                    "contract — do not leave a pinned proof switched off"
                )
            gated = [a for a in present if a.split("::")[-1] in ("cfg", "cfg_attr")]
            if gated:
                return (
                    f"`fn {name}` in {path} is a test but is CONDITIONALLY COMPILED "
                    f"({gated}). Whether it runs then depends on the feature set, and this gate "
                    "cannot know that from source text, so it refuses to call it a live proof. "
                    "This is deliberately the RED side: a pinned proof must run unconditionally"
                )
        return None
    if kind == "const_str_agree":
        _, subjects, pinned = probe
        seen = {}
        for path, const_name in subjects:
            decls = const_str_values(path, const_name)
            if not decls:
                return (
                    f"`const {const_name}` has no DECLARATION in {path}. This clause is about "
                    "the value THAT constant carries, and every declaration site named here must "
                    "exist: a renamed, deleted or commented-out constant is the obligation gone, "
                    "not a probe to relax (the source is read comment-stripped and literals are "
                    "blanked, so a mention inside a string is not a declaration)"
                )
            opaque = [line for line, value in decls if value is None]
            if opaque:
                return (
                    f"`const {const_name}` in {path} (line(s) {opaque}) is not initialised from a "
                    "plain string literal, so its shipped value cannot be read from source text. "
                    "That is identifier indirection (residual class #3) and it is reported RED "
                    "rather than guessed at"
                )
            values = sorted({value for _, value in decls})
            if len(values) > 1:
                return (
                    f"`const {const_name}` in {path} is declared with CONFLICTING values "
                    f"{values} — the shipped value is ambiguous, so the agreement this clause "
                    "requires cannot be established"
                )
            seen[f"{path}::{const_name}"] = values[0]
        distinct = sorted(set(seen.values()))
        if len(distinct) > 1:
            return (
                "the constants this clause requires to AGREE do not: "
                + ", ".join(f"{where} = '{value}'" for where, value in seen.items())
                + ". The engine's shipped default and the pinned embedder's own identity must be "
                "the same name — the engine fails closed on an identity mismatch, so a "
                "divergence here is a broken shipped default, not a cosmetic drift"
            )
        if distinct[0] != pinned:
            return (
                "the constants "
                + ", ".join(sorted(seen))
                + f" agree on '{distinct[0]}', but the pinned contract value is '{pinned}'. "
                "The clause registry was derived from a contract that names this default: "
                "changing the shipped default is a contract-relevant change, not a re-pin"
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
