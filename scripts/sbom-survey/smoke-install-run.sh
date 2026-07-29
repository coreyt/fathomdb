#!/usr/bin/env bash
# TC-115 — install-then-run smoke for `sbom-survey` (0.8.20 Slice 33).
#
# WHY THIS EXISTS
# ---------------
# Steward `seq-172` ruled CI wiring for this tool **OUT** — not deferred, out.
# This script is therefore the ONLY guard for the install-path defect class, and
# it is run by hand.
#
# On Slice 32 both an implementer and the orchestrator made the same error:
# **installing is not verifying an install — invoking what was installed is.**
# A `pip install` that exits 0 proves a wheel built; it proves nothing about the
# console script, the entry point, or the package's importability from site-
# packages. This script closes exactly that gap: it installs into a throwaway
# venv OUTSIDE the repository, invokes the INSTALLED console script, and then
# proves the source tree produces byte-identical artifacts.
#
# WHAT IT ASSERTS
#   A. the installed console script FILE exists and is executable;
#   B. RUN A — `$VENV/bin/sbom-survey` (the real entry point) exits 0;
#   C. PROVENANCE — after uninstalling, `import sbom_survey` resolves under the
#      repo source tree, so RUN B genuinely exercises the tree and the identity
#      check below cannot be vacuously true against the still-installed copy
#      (TC-105: Slice 31's dominant defect class was a criterion graded against
#      a helper while the real boundary went ungraded);
#   D. RUN B — `python -m sbom_survey` from the source tree exits 0;
#   E. the artifact SETS are identical (an extra/missing file is caught too);
#   F. all three artifacts are byte-identical between the two runs;
#   G. VACUITY GUARD — two empty files are byte-identical, so the run is only
#      believed when `summary.components > 0` and `rows` is non-empty.
#
# DELIBERATELY NOT CI-WIRED (`seq-172`). Do not add it to `scripts/agent-test.sh`,
# `.github/workflows/ci.yml`, `scripts/agent-verify.sh` or `scripts/check.sh` —
# `AC-SBOM-19` asserts the absence of any `sbom-survey` reference in the wiring
# files and must stay green.
#
# NETWORK: the `pip install` step needs PyPI. The survey runs themselves are
# `--offline` and consult no registry.
#
# USAGE:  bash scripts/sbom-survey/smoke-install-run.sh
# EXIT:   0 = PASS, non-zero = a real defect (the diagnostic names which one).

set -euo pipefail

# --- 1. repo root, resolved from this script's own location (never hardcoded) --
SCRIPT_DIR="$(dirname "$0")"
REPO="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
PROJECT="$REPO/scripts/sbom-survey"

echo "smoke: repo    = $REPO"
echo "smoke: project = $PROJECT"

# --- 2. scrub stale build products BEFORE installing ---------------------------
# A stale `build/` tree makes setuptools package OLD code into the wheel. That
# cost Slice 32 an entire verification cycle chasing a phantom. One destructive
# `rm -rf` per statement; never `find -delete`.
scrub_build_tree() {
    if [ -d "$PROJECT/build" ]; then
        rm -rf "$PROJECT/build"
    fi
    local egg
    shopt -s nullglob
    for egg in "$PROJECT"/*.egg-info; do
        rm -rf "$egg"
    done
    shopt -u nullglob
}

scrub_build_tree
echo "smoke: scrubbed build/ and *.egg-info/ before install"

# --- 3. work dir, asserted OUTSIDE the repo ------------------------------------
WORK="$(mktemp -d)"
WORK_REAL="$(cd "$WORK" && pwd -P)"
REPO_REAL="$(cd "$REPO" && pwd -P)"
case "$WORK_REAL/" in
    "$REPO_REAL"/*)
        echo "smoke: FAIL — work dir $WORK_REAL is INSIDE the repo $REPO_REAL." >&2
        echo "smoke:        a venv inside the repo tree is the trap this guards." >&2
        rm -rf "$WORK"
        exit 1
        ;;
esac
echo "smoke: work    = $WORK (verified outside the repo)"

cleanup() {
    local rc=$?
    if [ -d "$WORK" ]; then
        rm -rf "$WORK"
    fi
    # Leave the tree as we found it.
    scrub_build_tree
    exit "$rc"
}
trap cleanup EXIT

VENV="$WORK/venv"

# --- 4. venv + install ---------------------------------------------------------
set +e
python3 -m venv "$VENV"
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "smoke: FAIL — python3 -m venv exited rc=$rc" >&2
    exit 1
fi

echo "smoke: installing $PROJECT into $VENV (needs PyPI) ..."
set +e
"$VENV/bin/pip" install --disable-pip-version-check "$PROJECT"
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "smoke: FAIL — pip install exited rc=$rc." >&2
    echo "smoke:        The most likely cause is PyPI being unreachable: this is" >&2
    echo "smoke:        the ONE step that needs the network (the survey runs" >&2
    echo "smoke:        themselves are --offline). Re-run with network access" >&2
    echo "smoke:        before treating this as a defect in the tool." >&2
    exit 1
fi

# --- 5. the console script FILE must exist and be executable -------------------
CONSOLE="$VENV/bin/sbom-survey"
if [ ! -f "$CONSOLE" ]; then
    echo "smoke: FAIL — console script $CONSOLE was not created by the install." >&2
    echo "smoke:        [project.scripts] in pyproject.toml is not taking effect." >&2
    exit 1
fi
if [ ! -x "$CONSOLE" ]; then
    echo "smoke: FAIL — console script $CONSOLE exists but is not executable." >&2
    exit 1
fi
echo "smoke: console script present and executable: $CONSOLE"

OUT_INSTALLED="$WORK/out-installed"
OUT_SOURCE="$WORK/out-source"

# --- 6. RUN A — the INSTALLED path, the real entry point ------------------------
echo "smoke: RUN A — installed console script"
set +e
"$CONSOLE" --repo "$REPO" --offline --out "$OUT_INSTALLED"
rc_a=$?
set -e
if [ "$rc_a" -ne 0 ]; then
    echo "smoke: FAIL — RUN A (installed console script) exited rc=$rc_a, expected 0." >&2
    exit 1
fi
echo "smoke: RUN A rc=$rc_a"

# --- 7a. uninstall, so the code must now come from the tree --------------------
# Dependencies stay installed; only the `sbom-survey` distribution goes.
set +e
"$VENV/bin/pip" uninstall -y --disable-pip-version-check sbom-survey
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "smoke: FAIL — pip uninstall sbom-survey exited rc=$rc." >&2
    exit 1
fi

# --- 8. PROVENANCE ASSERTION — RUN B must really be the source tree ------------
# Without this, RUN B could silently still be the installed copy and the
# byte-identity check below would be vacuously true.
set +e
RESOLVED="$(PYTHONPATH="$PROJECT" "$VENV/bin/python" -c 'import sbom_survey; print(sbom_survey.__file__)')"
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "smoke: FAIL — could not import sbom_survey from the source tree (rc=$rc)." >&2
    exit 1
fi
case "$RESOLVED" in
    "$PROJECT"/*)
        echo "smoke: provenance OK — sbom_survey resolves to $RESOLVED"
        ;;
    *)
        echo "smoke: FAIL — provenance. sbom_survey resolved to:" >&2
        echo "smoke:        $RESOLVED" >&2
        echo "smoke:        expected a path under $PROJECT. RUN B would have been" >&2
        echo "smoke:        the installed copy again, making the byte-identity" >&2
        echo "smoke:        assertion vacuously true." >&2
        exit 1
        ;;
esac

# --- 7b. RUN B — the SOURCE-TREE path ------------------------------------------
echo "smoke: RUN B — source tree via python -m sbom_survey"
set +e
PYTHONPATH="$PROJECT" "$VENV/bin/python" -m sbom_survey --repo "$REPO" --offline --out "$OUT_SOURCE"
rc_b=$?
set -e
if [ "$rc_b" -ne 0 ]; then
    echo "smoke: FAIL — RUN B (source tree) exited rc=$rc_b, expected 0." >&2
    exit 1
fi
echo "smoke: RUN B rc=$rc_b"

# --- 9. the artifact SETS must be identical ------------------------------------
# Compare sorted listings, so an EXTRA or MISSING file is caught, not just
# differing content of the three files we go on to compare.
set +e
SET_A="$(cd "$OUT_INSTALLED" && ls -A | LC_ALL=C sort)"
SET_B="$(cd "$OUT_SOURCE" && ls -A | LC_ALL=C sort)"
set -e
if [ "$SET_A" != "$SET_B" ]; then
    echo "smoke: FAIL — the two runs wrote DIFFERENT artifact sets." >&2
    echo "smoke:        installed ($OUT_INSTALLED):" >&2
    printf '%s\n' "$SET_A" | sed 's/^/smoke:          /' >&2
    echo "smoke:        source ($OUT_SOURCE):" >&2
    printf '%s\n' "$SET_B" | sed 's/^/smoke:          /' >&2
    exit 1
fi
echo "smoke: artifact sets identical:"
printf '%s\n' "$SET_A" | sed 's/^/smoke:   /'

# --- 10. the three artifacts must exist in BOTH and be byte-identical ----------
ARTIFACTS="sbom.cdx.json staleness.json staleness.md"
for name in $ARTIFACTS; do
    if [ ! -f "$OUT_INSTALLED/$name" ]; then
        echo "smoke: FAIL — $name missing from the INSTALLED run's output dir." >&2
        exit 1
    fi
    if [ ! -f "$OUT_SOURCE/$name" ]; then
        echo "smoke: FAIL — $name missing from the SOURCE run's output dir." >&2
        exit 1
    fi
    if ! cmp -s "$OUT_INSTALLED/$name" "$OUT_SOURCE/$name"; then
        echo "smoke: FAIL — $name DIFFERS between the installed run and the source run." >&2
        echo "smoke:        installed: $OUT_INSTALLED/$name" >&2
        echo "smoke:        source:    $OUT_SOURCE/$name" >&2
        echo "smoke:        first 20 diff lines:" >&2
        diff "$OUT_INSTALLED/$name" "$OUT_SOURCE/$name" 2>&1 | head -20 | sed 's/^/smoke:        /' >&2
        exit 1
    fi
    echo "smoke: byte-identical: $name"
done

# --- 11. VACUITY GUARD — two empty files are byte-identical --------------------
# Read the INSTALLED run's staleness.json with the venv interpreter and stdlib
# `json` (no jq dependency). A survey that found nothing must never PASS.
set +e
COMPONENTS="$(
    "$VENV/bin/python" - "$OUT_INSTALLED/staleness.json" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    doc = json.load(handle)

summary = doc.get("summary") or {}
components = summary.get("components")
rows = doc.get("rows")

if not isinstance(components, int) or components <= 0:
    print(
        f"VACUOUS: summary.components is {components!r}, expected a positive int",
        file=sys.stderr,
    )
    raise SystemExit(1)
if not isinstance(rows, list) or not rows:
    print(
        f"VACUOUS: rows is {type(rows).__name__} of length "
        f"{len(rows) if isinstance(rows, list) else 'n/a'}, expected a non-empty list",
        file=sys.stderr,
    )
    raise SystemExit(1)

print(components)
PY
)"
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "smoke: FAIL — VACUITY GUARD. The two runs agree, but they agree on" >&2
    echo "smoke:        nothing: $OUT_INSTALLED/staleness.json reports no" >&2
    echo "smoke:        components and/or no rows. A byte-identity PASS over" >&2
    echo "smoke:        empty artifacts certifies nothing." >&2
    exit 1
fi

# --- 12. PASS -----------------------------------------------------------------
echo "smoke: PASS — installed run rc=$rc_a, source run rc=$rc_b, artifacts byte-identical over ${COMPONENTS} components (sbom.cdx.json, staleness.json, staleness.md); provenance and vacuity guards both held."
exit 0
