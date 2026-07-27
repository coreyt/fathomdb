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
#     it is checked as one); and
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
#   4. a source file (or source tree) a clause's assertion must read is missing
#      or unreadable — the assertion could not be EVALUATED, which is neither a
#      pass nor a clause failure.
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
# Probe kinds:
#   ("present", path, regex)          regex must match at least once in path
#   ("absent",  path, regex)          regex must not match anywhere in path
#   ("min",     path, regex, n)       regex must match at least n times in path
#   ("absent_tree", tree, regex, exts) regex must not match in any text file
#                                      under tree (exts=None ⇒ every text file)
# ---------------------------------------------------------------------------
ASSERTIONS = {
    # ---- Cohesion seam --------------------------------------------------
    "C1-SEAM-ENGINE-BUILD-DROP": [
        ("present", ENG, r"pub fn configure_projections\("),
        ("present", ENG, r"drop: &\[String\],"),
        ("present", ENG, r"-> Result<ProjectionDelta, EngineError>"),
    ],
    # ---- Q1 --------------------------------------------------------------
    "C1-Q1-ROLE-SET": [
        ("present", ENG, r"pub enum ProjectionRole \{"),
        ("present", ENG, r"pub roles: BTreeSet<ProjectionRole>,"),
    ],
    # ---- Q3 --------------------------------------------------------------
    "C1-Q3-SOLE-AUTHORITY": [
        ("present", SCH, r"CREATE TABLE _fathomdb_projection_registry\("),
        ("present", ENG, r"fn apply_projection_config\("),
        ("present", ENG, r"pub struct ProjectionDelta \{"),
    ],
    "C1-Q3-DESTRUCTIVE-DELTA": [
        ("present", ENG, r"ProjectionDestructive \{"),
        ("present", T15, r"fn destructive_change_requires_explicit_drop\(\)"),
    ],
    "C1-Q3-OMISSION-NOT-DROP": [
        ("present", T15, r"fn role_add_builds_and_explicit_drop_drops_exactly_one\(\)"),
        ("present", T15, r"fn dropping_an_absent_name_is_a_clean_noop\(\)"),
    ],
    # ---- Q5 --------------------------------------------------------------
    "C1-Q5-DERIVED-CACHE-IDEMPOTENT": [
        ("present", ENG, r"pub unchanged: bool,"),
        ("present", T15, r"fn idempotent_reregistration_is_a_noop\(\)"),
    ],
    # ---- Q2 --------------------------------------------------------------
    "C1-Q2-ENGINE-EAV-PROPERTY-FTS": [
        ("present", SCH, r"CREATE TABLE canonical_attributes\("),
        ("present", SCH, r"CREATE VIRTUAL TABLE property_search_index USING fts5\("),
        ("present", SCH, r"CREATE INDEX canonical_attributes_name_value_idx"),
    ],
    "C1-Q2-ENGINE-PROJECTS-VIA-CONFIGURE": [
        ("present", T15, r"fn property_filter_returns_correct_rows\(\)"),
        ("present", T15, r"fn property_fts_search_returns_correct_rows\(\)"),
    ],
    "C1-Q2-NO-DATA-MIGRATION": [
        ("present", SCH, r"NO DATA MIGRATION \(HITL 2026-07-21\): shape only, no backfill\."),
        ("absent", SCH, r"INSERT INTO canonical_attributes"),
        ("absent", SCH, r"INSERT INTO property_search_index"),
    ],
    # ---- Q4 --------------------------------------------------------------
    "C1-Q4-CHEAP-SAME-TRANSACTION": [
        ("present", ENG, r"apply_projection_config\(&tx,"),
        ("present", ENG, r"pub built: Vec<String>,"),
    ],
    "C1-Q4-DENSE-READINESS-TWO-MEMBERS": [
        ("present", ENG, r"pub enum DenseReadiness \{"),
        ("present", ENG, r'DenseReadiness::Ready => "ready",'),
        ("present", ENG, r'DenseReadiness::Embedding => "embedding",'),
        ("absent", ENG, r"DenseReadiness::Pending"),
    ],
    "C1-Q4-NO-PROVISIONAL-CONCEPT": [
        ("absent_tree", SRC_TREE, r"(?i)provisional", (".rs",)),
    ],
    # ---- Q6(a) -----------------------------------------------------------
    "C1-Q6A-THREE-ROLES": [
        ("present", ENG, r'"filterable" => Some\(ProjectionRole::Filterable\),'),
        ("present", ENG, r'"rankable" => Some\(ProjectionRole::Rankable\),'),
        ("present", ENG, r'"searchable" => Some\(ProjectionRole::Searchable\),'),
        ("absent", ENG, r"ProjectionRole::Vector\b"),
        ("absent", ENG, r"ProjectionRole::Fts\b"),
    ],
    "C1-Q6A-RANKABLE-GRACEFUL-DEFER": [
        ("present", ENG, r"pub deferred: Vec<String>,"),
        ("present", T15, r"fn rankable_is_graceful_deferred_never_blocking\(\)"),
        ("present", T15, r"fn idempotent_reregistration_holds_for_deferred_rankable\(\)"),
    ],
    # ---- Q6(b), AS AMENDED at efa8d584 -----------------------------------
    "C1-Q6B-NO-ENTITYTYPESPEC-NO-IDPREFIX": [
        ("absent_tree", SRC_TREE, r"EntityTypeSpec", None),
        ("absent_tree", SRC_TREE, r"\bid_prefix\b", None),
        ("absent_tree", SRC_TREE, r"\bidPrefix\b", None),
    ],
    "C1-Q6B-IDSPACE-TOTAL-THREE": [
        ("present", ENG, r"pub enum IdSpaceKind \{"),
        ("present", ENG, r'Self::Logical => "l:",'),
        ("present", ENG, r'Self::Content => "h:",'),
        ("present", ENG, r'Self::Passage => "p:",'),
    ],
    "C1-Q6B-ID-NON-NULL": [
        ("present", ENG, r"pub id: IdSpace,"),
        ("absent", ENG, r"pub id: Option<IdSpace>"),
    ],
    "C1-Q6B-H-TERMINAL-NOT-LIFECYCLE-ADDRESSABLE": [
        ("present", ENG, r"NotLifecycleAddressable \{"),
        ("present", T25, r"fn an_anonymous_write_stays_anonymous_through_the_whole_durable_path\(\)"),
    ],
    "C1-Q6B-SURROGATE-GOVERNED-ONLY": [
        ("present", T25, r"fn registering_projections_never_alters_a_pre_existing_row_id_space\(\)"),
        ("present", T25, r"fn the_internal_structural_row_writer_mints_no_logical_id\(\)"),
    ],
    # ---- Apply atomicity -------------------------------------------------
    "C1-AA-ATOMIC-FLIP": [
        ("present", ENG, r"fn commit_projection_outcomes\("),
        ("present", T20,
         r"fn atomic_flip_never_exposes_ready_without_the_vector_under_concurrent_write\(\)"),
    ],
    "C1-AA-NO-BLOCK-ON-EMBEDDING": [
        ("present", ENG, r"notify_new_work\(\)"),
        ("present", T20,
         r"fn readiness_reads_embedding_while_embeds_are_outstanding_then_flips_to_ready\(\)"),
    ],
    "C1-AA-CRASH-HEAL-BOOT-REDERIVE": [
        ("present", ENG, r"fn load_projection_registry\("),
        ("present", T15, r"fn boot_rederive_converges_after_simulated_crash\(\)"),
    ],
    # ---- Tokenizer / embedder defaults -----------------------------------
    "C1-TE-DEFAULT-EMBEDDER": [
        ("present", ENG, r"pub embedder: Option<String>,"),
        ("present", EMB,
         r'pub const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-bge-small-en-v1\.5";'),
    ],
    "C1-TE-DEFAULT-TOKENIZER": [
        ("present", ENG, r"pub tokenizer: Option<String>,"),
        ("min", SCH, r"tokenize = 'porter unicode61 remove_diacritics 2'", 2),
    ],
    "C1-TE-CUSTOM-TOKENIZER-DEFERRED": [
        ("present", SCH, r"fts_tokenizer TEXT,"),
        ("present", SCH, r"recorded in the registry but not honoured here"),
        ("absent", ENG, r"CREATE VIRTUAL TABLE property_search_index"),
    ],
    # ---- Landing ---------------------------------------------------------
    "C1-LAND-0820-SLOT": [
        ("present", PLAN, r"\| R-20-PR \|"),
        ("present", PLAN, r"\| R-20-EAV \|"),
        ("present", PLAN, r"\| R-20-DR \|"),
        ("present", PLAN, r"\| R-20-SUR \|"),
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


def walk_tree(rel, exts):
    """Yield (path, text) for every text file under a root-relative tree."""
    base = os.path.join(ROOT, rel)
    if not os.path.isdir(base):
        die_env(
            f"the source tree {rel} does not exist under --root {ROOT} — a clause assertion could "
            "not be EVALUATED (TC-37 evaporation path #4). The gate computed no verdict for that "
            "clause, so it refuses to report one."
        )
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
            yield os.path.relpath(full, ROOT), blob.decode("utf-8", errors="replace")


def line_of(text, match):
    return text.count("\n", 0, match.start()) + 1


def run_probe(probe):
    """Return a human-readable defect string, or None when the probe holds."""
    kind = probe[0]
    if kind == "present":
        _, path, pattern = probe
        text = read_source(path)
        if re.search(pattern, text) is None:
            return f"expected /{pattern}/ in {path}, found 0 match(es)"
        return None
    if kind == "absent":
        _, path, pattern = probe
        text = read_source(path)
        found = list(re.finditer(pattern, text))
        if found:
            return (
                f"expected NO match for /{pattern}/ in {path}, found {len(found)} "
                f"(first at {path}:{line_of(text, found[0])})"
            )
        return None
    if kind == "min":
        _, path, pattern, least = probe
        text = read_source(path)
        found = len(re.findall(pattern, text))
        if found < least:
            return f"expected at least {least} match(es) of /{pattern}/ in {path}, found {found}"
        return None
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
