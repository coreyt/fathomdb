#!/usr/bin/env bash
# scripts/tests/test_design_status_frontmatter.sh — T2c recurrence guard
# (DOC-HYGIENE-2). Mirrors scripts/tests/test_plans_status_frontmatter.sh.
#
# Proves scripts/lint-design-status.sh:
#   1. FAILS a dev/design/**.md file with no YAML frontmatter at all.
#   2. FAILS a dev/design/**.md file with frontmatter but no `status:` key.
#   3. FAILS a dev/design/**.md file whose `status:` value is outside the
#      allowed set and outside the frozen legacy set.
#   4. FAILS `status: SUPERSEDED` with no `superseded_by:` (and with an empty
#      one) — a supersession must name a successor.
#   5. FAILS when it discovers ZERO files (TC-37 vacuous-pass guard) — both
#      when dev/design/ is empty and when it does not exist at all.
#   6. PASSES valid UNREVIEWED / ACTIVE / SUPERSEDED+superseded_by, including
#      in a SUBDIRECTORY (the scan is recursive, unlike the dev/plans one).
#   7. Stays SCOPED to dev/design/** — a bare .md planted in dev/plans/,
#      dev/notes/, docs/ or the repo root must not fail this gate.
#   8. Grandfathers the frozen pre-gate legacy vocabulary (e.g. `locked`) but
#      only up to the ceiling, and carries NO filename-based exception.
#   9. FAILS an UNTERMINATED frontmatter block (opens with `---`, never closes)
#      — otherwise awk reads the whole document as the block and any `status:`
#      line in the BODY satisfies a doc with no valid frontmatter at all.
#      A properly-closed block followed by a later `---` horizontal rule must
#      still pass (fix-1, codex §9 [P2]).
#  10. Enforces the legacy ratchet in BOTH directions: over the ceiling fails
#      (a new legacy doc), and UNDER the ceiling also fails, naming the value to
#      ratchet to. A ceiling that never falls is just a constant, and would let
#      a later change re-add legacy statuses for free (fix-1, codex §9 [P2]).
#
# Builds a throwaway fixture repo under mktemp -d; never touches this
# checkout's real dev/design/.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

FIX="$(mktemp -d)"
trap 'rm -rf "$FIX"' EXIT

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

setup_fixture() {
  rm -rf "$FIX/repo"
  mkdir -p "$FIX/repo/scripts" "$FIX/repo/dev/design/subdir" \
    "$FIX/repo/dev/plans" "$FIX/repo/dev/notes" "$FIX/repo/docs"
  cp "$REPO_ROOT/scripts/lint-design-status.sh" "$FIX/repo/scripts/lint-design-status.sh"
  chmod +x "$FIX/repo/scripts/lint-design-status.sh"
  (cd "$FIX/repo" && git init -q && git config user.email t@example.com && git config user.name t)
  # A fixture repo has none of the 46 grandfathered docs, and the ratchet now
  # fails BELOW the ceiling as well as above it, so every arm must declare the
  # ceiling it means to test against. Default 0 = "this arm is not about legacy".
  set_budget 0
}

# The ceiling is a hard-coded constant on purpose — an env-var seam would be a
# way to raise it silently, which is the thing the ratchet exists to prevent.
# Fixtures therefore rewrite the constant in their own throwaway COPY.
set_budget() {
  sed -i -E "s/^LEGACY_BUDGET=.*/LEGACY_BUDGET=$1/" "$FIX/repo/scripts/lint-design-status.sh"
}

run_lint() {
  set +e
  OUT="$(cd "$FIX/repo" && bash scripts/lint-design-status.sh 2>&1)"
  RC=$?
  set -e
}

# A known-good doc, so a fixture that is otherwise empty still discovers files
# (keeps every arm testing the intended failure, not the zero-file guard).
plant_good() {
  cat >"$FIX/repo/dev/design/good.md" <<'EOF'
---
status: UNREVIEWED
---

# A doc nobody has classified yet
EOF
}

# --- Arm 1: no frontmatter at all -> FAIL -------------------------------
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/no-frontmatter.md" <<'EOF'
# A design doc with no frontmatter

Body text.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "no-frontmatter.md" <<<"$OUT" && grep -qi "frontmatter" <<<"$OUT"; then
  pass "missing frontmatter -> non-zero exit + names the offending file"
else
  fail "missing frontmatter case: rc=$RC out=$OUT"
fi

# --- Arm 2: frontmatter present, no status: key -> FAIL -----------------
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/no-status-key.md" <<'EOF'
---
title: Some design doc
date: 2026-07-01
---

# Some design doc
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "no-status-key.md" <<<"$OUT" && grep -qi "status" <<<"$OUT"; then
  pass "frontmatter without status: key -> non-zero exit"
else
  fail "missing status key case: rc=$RC out=$OUT"
fi

# --- Arm 2b: status: key present but empty -> FAIL ----------------------
setup_fixture
plant_good
printf -- '---\nstatus:\n---\n\n# empty status\n' >"$FIX/repo/dev/design/empty-status.md"
run_lint
if [ "$RC" -ne 0 ] && grep -q "empty-status.md" <<<"$OUT"; then
  pass "empty status: value -> non-zero exit (presence must mean a real value)"
else
  fail "empty status value case: rc=$RC out=$OUT"
fi

# --- Arm 3: invalid status value -> FAIL ---------------------------------
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/bad-status.md" <<'EOF'
---
title: Some design doc
status: DONE
---

# Some design doc
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "bad-status.md" <<<"$OUT" && grep -qi "DONE" <<<"$OUT"; then
  pass "invalid status value (DONE) -> non-zero exit, names the bad value"
else
  fail "invalid status value case: rc=$RC out=$OUT"
fi

# --- Arm 4: SUPERSEDED with no superseded_by: -> FAIL --------------------
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/superseded-no-target.md" <<'EOF'
---
status: SUPERSEDED
---

# Superseded, but by what?
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "superseded-no-target.md" <<<"$OUT" && grep -qi "superseded_by" <<<"$OUT"; then
  pass "SUPERSEDED without superseded_by: -> non-zero exit"
else
  fail "superseded-no-target case: rc=$RC out=$OUT"
fi

# --- Arm 4b: SUPERSEDED with an EMPTY superseded_by: -> FAIL -------------
setup_fixture
plant_good
printf -- '---\nstatus: SUPERSEDED\nsuperseded_by:\n---\n\n# empty target\n' \
  >"$FIX/repo/dev/design/superseded-empty-target.md"
run_lint
if [ "$RC" -ne 0 ] && grep -q "superseded-empty-target.md" <<<"$OUT"; then
  pass "SUPERSEDED with an empty superseded_by: -> non-zero exit (not a loophole)"
else
  fail "superseded-empty-target case: rc=$RC out=$OUT"
fi

# --- Arm 5: ZERO files discovered -> FAIL (TC-37 vacuous-pass guard) -----
setup_fixture
# dev/design/ and dev/design/subdir/ exist but contain no .md at all.
run_lint
if [ "$RC" -ne 0 ] && grep -qi "0 files" <<<"$OUT"; then
  pass "zero .md discovered -> hard fail, not a silent pass (TC-37)"
else
  fail "zero-files guard (empty dir): rc=$RC out=$OUT"
fi

# --- Arm 5b: dev/design/ absent entirely -> FAIL -------------------------
setup_fixture
rm -rf "$FIX/repo/dev/design"
run_lint
if [ "$RC" -ne 0 ]; then
  pass "dev/design/ missing entirely -> hard fail (scope evaporation is not a pass)"
else
  fail "zero-files guard (missing dir) reported a pass: rc=$RC out=$OUT"
fi

# --- Arm 6: valid governed values, incl. a subdirectory -> PASS ----------
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/active.md" <<'EOF'
---
title: Live design
status: ACTIVE
---

# Live design
EOF
cat >"$FIX/repo/dev/design/subdir/superseded-ok.md" <<'EOF'
---
status: SUPERSEDED
superseded_by: dev/design/active.md
---

# Superseded, and it says by what
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "UNREVIEWED + ACTIVE + SUPERSEDED-with-target (incl. subdir) -> exit 0"
else
  fail "valid values case unexpectedly failed: rc=$RC out=$OUT"
fi

# --- Arm 6b: the scan is RECURSIVE — a bad doc in a subdir still fails ---
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/subdir/nested-bad.md" <<'EOF'
# nested, no frontmatter — subdirectories are IN scope here
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "nested-bad.md" <<<"$OUT"; then
  pass "recursive scan — dev/design/subdir/ is in scope (unlike the dev/plans gate)"
else
  fail "recursion broken: a bad nested doc did not fail (rc=$RC out=$OUT)"
fi

# --- Arm 7: scope fence — nothing outside dev/design/** is read ----------
setup_fixture
plant_good
cat >"$FIX/repo/dev/plans/bare-plan.md" <<'EOF'
# no frontmatter, but this is dev/plans — lint-plans-status.sh's job, not ours
EOF
cat >"$FIX/repo/dev/notes/bare-note.md" <<'EOF'
# no frontmatter, dev/notes is not a governed status tier
EOF
cat >"$FIX/repo/docs/bare-doc.md" <<'EOF'
# no frontmatter, docs/ is gated by mkdocs --strict
EOF
cat >"$FIX/repo/README.md" <<'EOF'
# no frontmatter, repo root
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "scope fence — dev/plans, dev/notes, docs/ and the repo root are untouched"
else
  fail "scope fence broken: something outside dev/design/** leaked in (rc=$RC out=$OUT)"
fi

# --- Arm 8: frozen legacy vocabulary is grandfathered -> PASS ------------
setup_fixture
set_budget 2   # exactly the two legacy docs planted below
plant_good
cat >"$FIX/repo/dev/design/legacy.md" <<'EOF'
---
status: locked
---

# A pre-gate architecture spec, grandfathered by value (TC-50 retires this)
EOF
cat >"$FIX/repo/dev/design/legacy-trailing.md" <<'EOF'
---
status: SIGNED (HITL 2026-06-21 — trailing prose after the leading token)
---

# Legacy value with trailing prose
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "frozen legacy values (locked, SIGNED …) are grandfathered -> exit 0"
else
  fail "legacy grandfathering broken: rc=$RC out=$OUT"
fi

# --- Arm 8b: legacy grandfathering is VALUE-keyed, not FILENAME-keyed ----
# lint-plans-status.sh's header records why a filename carve-out is "exactly the
# shape of gate that rots" (T3 fix-1 removed one). Prove none crept in here: a
# fixture file named after a real grandfathered doc, with NO status, still fails.
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/engine.md" <<'EOF'
# no frontmatter — there is no filename exception for the locked arch specs
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "engine.md" <<<"$OUT"; then
  pass "no filename-based exception — engine.md is scanned like any other design doc"
else
  fail "a filename exception exists for engine.md: rc=$RC out=$OUT"
fi

# --- Arm 9: UNTERMINATED frontmatter block -> FAIL (fix-1, codex §9 [P2]) -
# The bypass: with no closing `---`, awk's "print until the next `---`" reads
# the ENTIRE document as the frontmatter block, so a `status:` line buried in
# the prose satisfies a doc that has no valid frontmatter at all. This arm is
# the RED witness — against the pre-fix script it exits 0.
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/unterminated.md" <<'EOF'
---
title: A design doc whose frontmatter block is never closed

# Body starts here, still inside the "block" as far as awk is concerned

Prose that happens to mention a key later on:

status: ACTIVE

More prose.
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -q "unterminated.md" <<<"$OUT" \
  && grep -qi "unterminated" <<<"$OUT" && ! grep -qi "missing a .status" <<<"$OUT"; then
  pass "unterminated frontmatter -> non-zero exit (a body status: is not frontmatter)"
else
  fail "unterminated frontmatter bypass: rc=$RC out=$OUT"
fi

# --- Arm 9b: closed block, then a later `---` horizontal rule -> PASS -----
# No-regression fence for arm 9: `---` is also valid Markdown for a thematic
# break, and one appearing AFTER a properly-closed block must not be read as a
# second/unbalanced delimiter.
setup_fixture
plant_good
cat >"$FIX/repo/dev/design/closed-then-hr.md" <<'EOF'
---
status: ACTIVE
---

# A normal doc

Some prose.

---

## A section after a horizontal rule

More prose.

---
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass 'closed block + later `---` horizontal rules -> exit 0 (no false positive)'
else
  fail "horizontal rule after a closed block was misread: rc=$RC out=$OUT"
fi

# --- Arm 9c: `---` on line 1 and nothing else -> FAIL ---------------------
setup_fixture
plant_good
printf -- '---\n' >"$FIX/repo/dev/design/only-opener.md"
run_lint
if [ "$RC" -ne 0 ] && grep -q "only-opener.md" <<<"$OUT"; then
  pass 'a lone opening `---` with no block and no closer -> non-zero exit'
else
  fail "lone-opener case: rc=$RC out=$OUT"
fi

# --- Arm 10: legacy count BELOW the ceiling -> FAIL (fix-1, codex §9 [P2]) -
# The ratchet is meant to be one-way. Pre-fix this only printed a NOTICE and
# exited 0, so the stale ceiling survived and a later PR could re-add legacy
# statuses back up to it for free. This arm is the second RED witness.
setup_fixture
set_budget 5
plant_good
cat >"$FIX/repo/dev/design/legacy-one.md" <<'EOF'
---
status: locked
---

# One legacy doc, against a ceiling of five
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -qi "LEGACY_BUDGET" <<<"$OUT" && grep -q " 1" <<<"$OUT"; then
  pass "legacy count below the ceiling -> non-zero exit, names the value to ratchet to"
else
  fail "below-budget ratchet did not block: rc=$RC out=$OUT"
fi

# --- Arm 10b: legacy count ABOVE the ceiling -> FAIL (no regression) ------
setup_fixture
set_budget 1
plant_good
cat >"$FIX/repo/dev/design/legacy-a.md" <<'EOF'
---
status: locked
---

# legacy a
EOF
cat >"$FIX/repo/dev/design/legacy-b.md" <<'EOF'
---
status: accepted
---

# legacy b
EOF
run_lint
if [ "$RC" -ne 0 ] && grep -qi "ceiling" <<<"$OUT"; then
  pass "legacy count over the ceiling -> non-zero exit (grandfathering is closed)"
else
  fail "over-budget ratchet did not fail: rc=$RC out=$OUT"
fi

# --- Arm 10c: legacy count EXACTLY at the ceiling -> PASS (no regression) -
# The real tree's steady state: 46 legacy docs against a budget of 46.
setup_fixture
set_budget 2
plant_good
cat >"$FIX/repo/dev/design/legacy-a.md" <<'EOF'
---
status: locked
---

# legacy a
EOF
cat >"$FIX/repo/dev/design/legacy-b.md" <<'EOF'
---
status: accepted
---

# legacy b
EOF
run_lint
if [ "$RC" -eq 0 ]; then
  pass "legacy count exactly at the ceiling -> exit 0 (the steady state stays green)"
else
  fail "at-budget case is not green: rc=$RC out=$OUT"
fi

if [ "$FAILED" -ne 0 ]; then
  printf '%d arm(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf 'PASS  all arms — scripts/lint-design-status.sh catches missing/empty/invalid status and\n'
printf 'PASS  targetless SUPERSEDED, hard-fails on zero discovered files, scans dev/design/** recursively,\n'
printf 'PASS  stays scoped to it, and grandfathers legacy values by value rather than by filename;\n'
printf 'PASS  rejects an unterminated frontmatter block (no body-status bypass) while tolerating a\n'
printf 'PASS  later `---` horizontal rule, and enforces the legacy ratchet both over AND under the ceiling\n'
