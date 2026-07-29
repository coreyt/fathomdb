OpenAI Codex v0.136.0
--------
workdir: /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
model: gpt-5.5
provider: openai
approval: never
sandbox: danger-full-access
reasoning effort: medium
reasoning summaries: none
session id: 019faf84-15c8-7772-a91f-48a8b67401f8
--------
user
changes against '29c2eae0'
exec
/bin/bash -lc "cat MEMORY.md 2>/dev/null || true
ls feedback_*.md 2>/dev/null | xargs -r -n1 -I{} sh -c 'echo --- {}; sed -n \"1,160p\" {}'" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
xargs: warning: options --max-args and --replace/-I/-i are mutually exclusive, ignoring previous --max-args value

exec
/bin/bash -lc 'git diff --stat 29c2eae00ba12c2929e4cd138eefaf7e74db720c && git diff --name-only 29c2eae00ba12c2929e4cd138eefaf7e74db720c' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
 dev/DOC-INDEX.md                                   |    2 +
 .../0.8.20-slice-33-library-sweep-3-FINDINGS.md    |  587 ++++
 .../0.8.20/slice-33-round1-20260729T193509Z.md     | 3347 ++++++++++++++++++++
 ...ice-33-round2-fix1-rereview-20260729T193712Z.md | 3067 ++++++++++++++++++
 ...ice-33-round3-fix2-rereview-20260729T200429Z.md | 1307 ++++++++
 scripts/sbom-survey/README.md                      |   22 +
 scripts/sbom-survey/smoke-install-run.sh           |  385 +++
 7 files changed, 8717 insertions(+)
dev/DOC-INDEX.md
dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md
scripts/sbom-survey/README.md
scripts/sbom-survey/smoke-install-run.sh

exec
/bin/bash -lc 'git diff 29c2eae00ba12c2929e4cd138eefaf7e74db720c -- scripts/sbom-survey/smoke-install-run.sh scripts/sbom-survey/README.md dev/DOC-INDEX.md' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
diff --git a/dev/DOC-INDEX.md b/dev/DOC-INDEX.md
index 556e8825..9d2825d9 100644
--- a/dev/DOC-INDEX.md
+++ b/dev/DOC-INDEX.md
@@ -138,6 +138,8 @@ refresh in the closing commit when you touch a doc).
 | `dev/design/0.8.20-tc90-tc91-characterization.md` | Characterization (no fix) — `Engine::transition`'s deferred write race (reproduces 10/10 under stress), and the cadence-sensitive duplicate embeds whose discarded worker commit is structurally invisible to terminal-state counting | 0.8.20 Slice 23 (R-20-SV leg 2); TC-90/TC-91, fix at 0.8.21 | 2026-07-29 |
 | `dev/design/0.8.20-slice-31-sbom-survey-tool.md` | Spec of record for `scripts/sbom-survey` — CycloneDX SBOM over tracked manifests, tiering, used-vs-published diff; 23 criteria | 0.8.20 Slice 31 (Library Sweep #3 leg 1/3; no requirement id, TC-76) | 2026-07-29 |
 | `scripts/sbom-survey/README.md` | Operating note for the dependency-survey mini-project — how to run the suite, and why it is deliberately not CI-gating | 0.8.20 Slice 31 (Library Sweep #3 leg 1/3) | 2026-07-29 |
+| `scripts/sbom-survey/smoke-install-run.sh` | TC-115 install-then-run smoke — installs the tool into a throwaway venv, invokes the INSTALLED console script, and asserts its artifacts are byte-identical to a source-tree run. Deliberately NOT CI-wired (`seq-172`) | 0.8.20 Slice 33 (Library Sweep #3 leg 3/3) | 2026-07-29 |
+| `dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md` | **Findings of record** for Library Sweep #3 — the ONLINE `sbom-survey` run at `29c2eae0`: 774 components, 28 direct outdated, per-dependency surgical verdicts, and the hand-off to 0.8.22. ASCERTAIN-ONLY; applied nothing | 0.8.20 Slice 33 (Library Sweep #3 leg 3/3; no requirement id, TC-76) | 2026-07-29 |
 
 ## `dev/adr/` — architecture decision records
 
diff --git a/scripts/sbom-survey/README.md b/scripts/sbom-survey/README.md
index 5eaef018..5fefe028 100644
--- a/scripts/sbom-survey/README.md
+++ b/scripts/sbom-survey/README.md
@@ -129,6 +129,28 @@ Expected: **`24 passed, 0 failed, 0 skipped, 0 errors`** (exit code `0`).
   `pytest.ini`, `tox.ini` or `setup.cfg` above this directory** — either would start applying real
   settings to this suite.
 
+## Install-then-run smoke (TC-115)
+
+`scripts/sbom-survey/smoke-install-run.sh` guards the **install path**, which the suite above does
+not reach: **installing is not verifying an install — invoking what was installed is.** It builds a
+throwaway venv **outside** the repo, `pip install`s this project, runs the **installed console
+script**, then re-runs the same survey from the source tree and asserts the two agree.
+
+```bash
+bash scripts/sbom-survey/smoke-install-run.sh     # exit 0 = PASS
+```
+
+It asserts: the console-script file exists and is executable · both runs exit `0` · **two symmetric
+provenance** checks — the installed run resolves inside the venv's `site-packages` (invoked under
+`env -u PYTHONPATH -u PYTHONHOME`, so an ambient `PYTHONPATH` cannot smuggle the source tree into
+the "installed" leg) and the source run resolves under `scripts/sbom-survey` (not the still-installed
+copy) · identical artifact **sets** · byte-identical `sbom.cdx.json` / `staleness.json` /
+`staleness.md` · and a **vacuity guard** (`summary.components > 0`, non-empty `rows`) — two empty
+files are byte-identical. Only the `pip install` needs network; both surveys run `--offline`.
+
+**It is deliberately NOT CI-wired** (steward `seq-172` ruled wiring out, not deferred) — run it by
+hand, and do not add it to `agent-test.sh` or `ci.yml`.
+
 ## Deliberately NOT wired into CI
 
 This tool is **recurring by design and NOT CI-gating** — it is **informational**
diff --git a/scripts/sbom-survey/smoke-install-run.sh b/scripts/sbom-survey/smoke-install-run.sh
new file mode 100755
index 00000000..28f91ce0
--- /dev/null
+++ b/scripts/sbom-survey/smoke-install-run.sh
@@ -0,0 +1,385 @@
+#!/usr/bin/env bash
+# TC-115 — install-then-run smoke for `sbom-survey` (0.8.20 Slice 33).
+#
+# WHY THIS EXISTS
+# ---------------
+# Steward `seq-172` ruled CI wiring for this tool **OUT** — not deferred, out.
+# This script is therefore the ONLY guard for the install-path defect class, and
+# it is run by hand.
+#
+# On Slice 32 both an implementer and the orchestrator made the same error:
+# **installing is not verifying an install — invoking what was installed is.**
+# A `pip install` that exits 0 proves a wheel built; it proves nothing about the
+# console script, the entry point, or the package's importability from site-
+# packages. This script closes exactly that gap: it installs into a throwaway
+# venv OUTSIDE the repository, invokes the INSTALLED console script, and then
+# proves the source tree produces byte-identical artifacts.
+#
+# WHAT IT ASSERTS
+#   A. the installed console script FILE exists and is executable;
+#   B. PROVENANCE (RUN A) — under a scrubbed import environment, the installed
+#      `sbom_survey` resolves inside the venv's site-packages and NOT under the
+#      repo. An ambient `PYTHONPATH` would otherwise make the "installed" run
+#      import the source tree, passing while the wheel is broken;
+#   C. RUN A — `$VENV/bin/sbom-survey` (the real entry point) exits 0;
+#   D. PROVENANCE (RUN B) — after uninstalling, `import sbom_survey` resolves
+#      under the repo source tree, so RUN B genuinely exercises the tree and the
+#      identity check below cannot be vacuously true against the still-installed
+#      copy (TC-105: Slice 31's dominant defect class was a criterion graded
+#      against a helper while the real boundary went ungraded);
+#   E. RUN B — `python -m sbom_survey` from the source tree exits 0;
+#   F. the artifact SETS are identical (an extra/missing file is caught too);
+#   G. all three artifacts are byte-identical between the two runs;
+#   H. VACUITY GUARD — two empty files are byte-identical, so the run is only
+#      believed when `summary.components > 0` and `rows` is non-empty.
+#
+# DELIBERATELY NOT CI-WIRED (`seq-172`). Do not add it to `scripts/agent-test.sh`,
+# `.github/workflows/ci.yml`, `scripts/agent-verify.sh` or `scripts/check.sh` —
+# `AC-SBOM-19` asserts the absence of any `sbom-survey` reference in the wiring
+# files and must stay green.
+#
+# NETWORK: the `pip install` step needs PyPI. The survey runs themselves are
+# `--offline` and consult no registry.
+#
+# CWD-INDEPENDENT. Both runs and every provenance probe execute (in subshells)
+# with cwd set to the throwaway work dir, because cwd lands on `sys.path` for
+# `python -c` and `python -m`. Every path handed to them is absolute, so this
+# changes nothing about what is surveyed — only that it cannot matter where you
+# invoked the smoke from. See §6a.
+#
+# USAGE:  bash scripts/sbom-survey/smoke-install-run.sh
+# EXIT:   0 = PASS, non-zero = a real defect (the diagnostic names which one).
+
+set -euo pipefail
+
+# --- 1. repo root, resolved from this script's own location (never hardcoded) --
+SCRIPT_DIR="$(dirname "$0")"
+REPO="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
+PROJECT="$REPO/scripts/sbom-survey"
+
+echo "smoke: repo    = $REPO"
+echo "smoke: project = $PROJECT"
+
+# --- 2. scrub stale build products BEFORE installing ---------------------------
+# A stale `build/` tree makes setuptools package OLD code into the wheel. That
+# cost Slice 32 an entire verification cycle chasing a phantom. One destructive
+# `rm -rf` per statement; never `find -delete`.
+scrub_build_tree() {
+    if [ -d "$PROJECT/build" ]; then
+        rm -rf "$PROJECT/build"
+    fi
+    local egg
+    shopt -s nullglob
+    for egg in "$PROJECT"/*.egg-info; do
+        rm -rf "$egg"
+    done
+    shopt -u nullglob
+}
+
+scrub_build_tree
+echo "smoke: scrubbed build/ and *.egg-info/ before install"
+
+# --- 3. work dir, asserted OUTSIDE the repo ------------------------------------
+WORK="$(mktemp -d)"
+WORK_REAL="$(cd "$WORK" && pwd -P)"
+REPO_REAL="$(cd "$REPO" && pwd -P)"
+case "$WORK_REAL/" in
+    "$REPO_REAL"/*)
+        echo "smoke: FAIL — work dir $WORK_REAL is INSIDE the repo $REPO_REAL." >&2
+        echo "smoke:        a venv inside the repo tree is the trap this guards." >&2
+        rm -rf "$WORK"
+        exit 1
+        ;;
+esac
+echo "smoke: work    = $WORK (verified outside the repo)"
+
+cleanup() {
+    local rc=$?
+    if [ -d "$WORK" ]; then
+        rm -rf "$WORK"
+    fi
+    # Leave the tree as we found it.
+    scrub_build_tree
+    exit "$rc"
+}
+trap cleanup EXIT
+
+VENV="$WORK/venv"
+
+# --- 4. venv + install ---------------------------------------------------------
+set +e
+python3 -m venv "$VENV"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — python3 -m venv exited rc=$rc" >&2
+    exit 1
+fi
+
+echo "smoke: installing $PROJECT into $VENV (needs PyPI) ..."
+set +e
+"$VENV/bin/pip" install --disable-pip-version-check "$PROJECT"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — pip install exited rc=$rc." >&2
+    echo "smoke:        The most likely cause is PyPI being unreachable: this is" >&2
+    echo "smoke:        the ONE step that needs the network (the survey runs" >&2
+    echo "smoke:        themselves are --offline). Re-run with network access" >&2
+    echo "smoke:        before treating this as a defect in the tool." >&2
+    exit 1
+fi
+
+# --- 5. the console script FILE must exist and be executable -------------------
+CONSOLE="$VENV/bin/sbom-survey"
+if [ ! -f "$CONSOLE" ]; then
+    echo "smoke: FAIL — console script $CONSOLE was not created by the install." >&2
+    echo "smoke:        [project.scripts] in pyproject.toml is not taking effect." >&2
+    exit 1
+fi
+if [ ! -x "$CONSOLE" ]; then
+    echo "smoke: FAIL — console script $CONSOLE exists but is not executable." >&2
+    exit 1
+fi
+echo "smoke: console script present and executable: $CONSOLE"
+
+OUT_INSTALLED="$WORK/out-installed"
+OUT_SOURCE="$WORK/out-source"
+
+# --- 6a. PROVENANCE ASSERTION FOR RUN A — symmetric with RUN B's (codex §9 rd 2)
+#
+# RUN A inherits the caller's environment, and `PYTHONPATH` beats site-packages
+# on `sys.path`. So an ambient `PYTHONPATH` pointing at THIS checkout (or any
+# checkout carrying `sbom_survey`) makes the installed console script import the
+# SOURCE TREE — and the smoke then passes while the installed wheel is broken,
+# incomplete, or missing files entirely. That is a vacuous pass on the exact leg
+# this script exists to prove.
+#
+# Two things are needed, and only the second is a guard:
+#   * RUN A is invoked under `env -u PYTHONPATH -u PYTHONHOME` — scrubbed
+#     PER-INVOCATION, never globally, because RUN B *needs* `PYTHONPATH`;
+#   * and that arrangement is ASSERTED here. Unsetting only ARRANGES for the
+#     right thing; the assertion PROVES it. RUN B's provenance was graded from
+#     the start and RUN A's was not — that asymmetry was the finding.
+#
+# THE PROBE MUST REPLICATE RUN A'S IMPORT ENVIRONMENT, NOT APPROXIMATE IT
+# (codex §9 round 3). `python -c` puts the CURRENT DIRECTORY on `sys.path[0]`
+# (it is `''`), whereas a script executed BY PATH — which is what
+# `$VENV/bin/sbom-survey` is — gets the SCRIPT's directory (`venv/bin`) there
+# and never cwd. An earlier probe therefore graded a stricter, different import
+# environment than the one it certified: run from `scripts/sbom-survey`, it
+# reported a source-tree import that RUN A would never have performed, and the
+# smoke failed before RUN A ever executed. A guard that cries wolf gets ignored,
+# which costs exactly the protection `seq-172` left this script as the only
+# source of.
+#
+# The fix is a NEUTRAL cwd: every probe below, and both runs, execute with cwd
+# set to `$WORK`, which is already asserted to be outside the repo and contains
+# no `sbom_survey`. `--repo`, `--out`, `$CONSOLE` and `$PROJECT` are all absolute,
+# so cwd cannot change what is surveyed or where it is written — only which
+# package would be importable, which is precisely what must not vary. (A neutral
+# cwd is preferred over `python -P` / `PYTHONSAFEPATH=1` because it mirrors the
+# console script's own situation directly and carries no interpreter-version
+# caveat.) `cd` happens in SUBSHELLS so the script's own cwd never moves.
+set +e
+SITE_PACKAGES="$(cd "$WORK" && env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ] || [ -z "$SITE_PACKAGES" ]; then
+    echo "smoke: FAIL — could not resolve the venv's site-packages (rc=$rc)." >&2
+    exit 1
+fi
+
+set +e
+RESOLVED_A="$(cd "$WORK" && env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sbom_survey; print(sbom_survey.__file__)')"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — the INSTALLED sbom_survey is not importable from the venv (rc=$rc)." >&2
+    echo "smoke:        pip install reported success, so the wheel does not contain" >&2
+    echo "smoke:        an importable package. This is the install-path defect." >&2
+    exit 1
+fi
+case "$RESOLVED_A" in
+    "$REPO"/*)
+        echo "smoke: FAIL — provenance (RUN A). The installed entry point resolves to:" >&2
+        echo "smoke:        $RESOLVED_A" >&2
+        echo "smoke:        which is INSIDE the repo ($REPO), not the venv's" >&2
+        echo "smoke:        site-packages ($SITE_PACKAGES). RUN A would exercise the" >&2
+        echo "smoke:        SOURCE TREE, so a PASS would say nothing about the wheel." >&2
+        exit 1
+        ;;
+esac
+case "$RESOLVED_A" in
+    "$SITE_PACKAGES"/*)
+        echo "smoke: provenance OK (RUN A) — installed sbom_survey resolves to $RESOLVED_A"
+        ;;
+    *)
+        echo "smoke: FAIL — provenance (RUN A). The installed entry point resolves to:" >&2
+        echo "smoke:        $RESOLVED_A" >&2
+        echo "smoke:        expected a path under the venv's site-packages:" >&2
+        echo "smoke:        $SITE_PACKAGES" >&2
+        exit 1
+        ;;
+esac
+
+# --- 6b. RUN A — the INSTALLED path, the real entry point -----------------------
+# Same scrubbed environment AND same neutral cwd the assertion above was made
+# under, so what was proved is exactly what runs. RUN A is in fact already
+# cwd-immune (a script run by path never puts cwd on `sys.path`), but it is run
+# from `$WORK` anyway so the probe and the run are not merely equivalent by
+# argument — they are identical by construction.
+echo "smoke: RUN A — installed console script"
+set +e
+( cd "$WORK" && env -u PYTHONPATH -u PYTHONHOME "$CONSOLE" --repo "$REPO" --offline --out "$OUT_INSTALLED" )
+rc_a=$?
+set -e
+if [ "$rc_a" -ne 0 ]; then
+    echo "smoke: FAIL — RUN A (installed console script) exited rc=$rc_a, expected 0." >&2
+    exit 1
+fi
+echo "smoke: RUN A rc=$rc_a"
+
+# --- 7a. uninstall, so the code must now come from the tree --------------------
+# Dependencies stay installed; only the `sbom-survey` distribution goes.
+set +e
+"$VENV/bin/pip" uninstall -y --disable-pip-version-check sbom-survey
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — pip uninstall sbom-survey exited rc=$rc." >&2
+    exit 1
+fi
+
+# --- 8. PROVENANCE ASSERTION (RUN B) — it must really be the source tree -------
+# Without this, RUN B could silently still be the installed copy and the
+# byte-identity check below would be vacuously true. The mirror image of §6a:
+# there the repo must NOT be on the import path, here it must be.
+#
+# Neutral cwd here too, and here it is not merely hygiene: `python -m` DOES put
+# cwd on `sys.path` (unlike a script run by path), so RUN B is genuinely
+# cwd-sensitive. Running probe and run from `$WORK` makes `PYTHONPATH="$PROJECT"`
+# the one and only reason the package is importable — which is the thing being
+# asserted. Run from `scripts/sbom-survey`, cwd would supply the same tree and
+# the assertion would pass for a reason it had not tested.
+set +e
+RESOLVED="$(cd "$WORK" && PYTHONPATH="$PROJECT" "$VENV/bin/python" -c 'import sbom_survey; print(sbom_survey.__file__)')"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — could not import sbom_survey from the source tree (rc=$rc)." >&2
+    exit 1
+fi
+case "$RESOLVED" in
+    "$PROJECT"/*)
+        echo "smoke: provenance OK (RUN B) — sbom_survey resolves to $RESOLVED"
+        ;;
+    *)
+        echo "smoke: FAIL — provenance (RUN B). sbom_survey resolved to:" >&2
+        echo "smoke:        $RESOLVED" >&2
+        echo "smoke:        expected a path under $PROJECT. RUN B would have been" >&2
+        echo "smoke:        the installed copy again, making the byte-identity" >&2
+        echo "smoke:        assertion vacuously true." >&2
+        exit 1
+        ;;
+esac
+
+# --- 7b. RUN B — the SOURCE-TREE path ------------------------------------------
+echo "smoke: RUN B — source tree via python -m sbom_survey"
+set +e
+( cd "$WORK" && PYTHONPATH="$PROJECT" "$VENV/bin/python" -m sbom_survey --repo "$REPO" --offline --out "$OUT_SOURCE" )
+rc_b=$?
+set -e
+if [ "$rc_b" -ne 0 ]; then
+    echo "smoke: FAIL — RUN B (source tree) exited rc=$rc_b, expected 0." >&2
+    exit 1
+fi
+echo "smoke: RUN B rc=$rc_b"
+
+# --- 9. the artifact SETS must be identical ------------------------------------
+# Compare sorted listings, so an EXTRA or MISSING file is caught, not just
+# differing content of the three files we go on to compare.
+set +e
+SET_A="$(cd "$OUT_INSTALLED" && ls -A | LC_ALL=C sort)"
+SET_B="$(cd "$OUT_SOURCE" && ls -A | LC_ALL=C sort)"
+set -e
+if [ "$SET_A" != "$SET_B" ]; then
+    echo "smoke: FAIL — the two runs wrote DIFFERENT artifact sets." >&2
+    echo "smoke:        installed ($OUT_INSTALLED):" >&2
+    printf '%s\n' "$SET_A" | sed 's/^/smoke:          /' >&2
+    echo "smoke:        source ($OUT_SOURCE):" >&2
+    printf '%s\n' "$SET_B" | sed 's/^/smoke:          /' >&2
+    exit 1
+fi
+echo "smoke: artifact sets identical:"
+printf '%s\n' "$SET_A" | sed 's/^/smoke:   /'
+
+# --- 10. the three artifacts must exist in BOTH and be byte-identical ----------
+ARTIFACTS="sbom.cdx.json staleness.json staleness.md"
+for name in $ARTIFACTS; do
+    if [ ! -f "$OUT_INSTALLED/$name" ]; then
+        echo "smoke: FAIL — $name missing from the INSTALLED run's output dir." >&2
+        exit 1
+    fi
+    if [ ! -f "$OUT_SOURCE/$name" ]; then
+        echo "smoke: FAIL — $name missing from the SOURCE run's output dir." >&2
+        exit 1
+    fi
+    if ! cmp -s "$OUT_INSTALLED/$name" "$OUT_SOURCE/$name"; then
+        echo "smoke: FAIL — $name DIFFERS between the installed run and the source run." >&2
+        echo "smoke:        installed: $OUT_INSTALLED/$name" >&2
+        echo "smoke:        source:    $OUT_SOURCE/$name" >&2
+        echo "smoke:        first 20 diff lines:" >&2
+        diff "$OUT_INSTALLED/$name" "$OUT_SOURCE/$name" 2>&1 | head -20 | sed 's/^/smoke:        /' >&2
+        exit 1
+    fi
+    echo "smoke: byte-identical: $name"
+done
+
+# --- 11. VACUITY GUARD — two empty files are byte-identical --------------------
+# Read the INSTALLED run's staleness.json with the venv interpreter and stdlib
+# `json` (no jq dependency). A survey that found nothing must never PASS.
+set +e
+COMPONENTS="$(
+    cd "$WORK" && "$VENV/bin/python" - "$OUT_INSTALLED/staleness.json" <<'PY'
+import json
+import sys
+
+path = sys.argv[1]
+with open(path, encoding="utf-8") as handle:
+    doc = json.load(handle)
+
+summary = doc.get("summary") or {}
+components = summary.get("components")
+rows = doc.get("rows")
+
+if not isinstance(components, int) or components <= 0:
+    print(
+        f"VACUOUS: summary.components is {components!r}, expected a positive int",
+        file=sys.stderr,
+    )
+    raise SystemExit(1)
+if not isinstance(rows, list) or not rows:
+    print(
+        f"VACUOUS: rows is {type(rows).__name__} of length "
+        f"{len(rows) if isinstance(rows, list) else 'n/a'}, expected a non-empty list",
+        file=sys.stderr,
+    )
+    raise SystemExit(1)
+
+print(components)
+PY
+)"
+rc=$?
+set -e
+if [ "$rc" -ne 0 ]; then
+    echo "smoke: FAIL — VACUITY GUARD. The two runs agree, but they agree on" >&2
+    echo "smoke:        nothing: $OUT_INSTALLED/staleness.json reports no" >&2
+    echo "smoke:        components and/or no rows. A byte-identity PASS over" >&2
+    echo "smoke:        empty artifacts certifies nothing." >&2
+    exit 1
+fi
+
+# --- 12. PASS -----------------------------------------------------------------
+echo "smoke: PASS — installed run rc=$rc_a, source run rc=$rc_b, artifacts byte-identical over ${COMPONENTS} components (sbom.cdx.json, staleness.json, staleness.md); both provenance guards (installed=site-packages, source=tree) and the vacuity guard held."
+exit 0

exec
/bin/bash -lc "find scripts/sbom-survey -maxdepth 3 -type f -not -path '*/__pycache__/*' -print | sort | xargs -I{} sh -c 'echo --- {}; sed -n \"1,220p\" {}'" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
--- scripts/sbom-survey/.pytest_cache/.gitignore
# Created by pytest automatically.
*
--- scripts/sbom-survey/.pytest_cache/CACHEDIR.TAG
Signature: 8a477f597d28d172789f06886806bc55
# This file is a cache directory tag created by pytest.
# For information about cache directory tags, see:
#	https://bford.info/cachedir/spec.html
--- scripts/sbom-survey/.pytest_cache/README.md
# pytest cache directory #

This directory contains data from the pytest's cache plugin,
which provides the `--lf` and `--ff` options, as well as the `cache` fixture.

**Do not** commit this to version control.

See [the docs](https://docs.pytest.org/en/stable/how-to/cache.html) for more information.
--- scripts/sbom-survey/README.md
# `sbom-survey` — dependency survey tool (Library Sweep #3)

An **isolated Python mini-project** that produces a **CycloneDX 1.6 JSON** SBOM over every
dependency manifest **tracked on `main`**, enumerates the **library↔library** dependency graph, and
diffs **used (locked) versus published (registry latest)** versions.

It mechanizes the manual triage loop in `dev/plans/prompts/LIBRARY-BUMP-STEWARD.md` §1–2 — is the
manifest tracked, is the dependency direct or transitive, is the locked version already at target —
so a Library Sweep is ~90% mechanical instead of a search → review → check → reason cycle per
candidate.

**Spec of record:** `dev/design/0.8.20-slice-31-sbom-survey-tool.md` — requirements, the 24
acceptance criteria, the design, and the answers to every resolved design question. Read that first;
this file is only the operating note.

## Layout

| Path | What it is |
|---|---|
| `pyproject.toml` | the isolated project manifest — dependencies per design §5.7 |
| `tiers.toml` | the **tracked tier/exclusion data** (§5.3); rules are DATA, never code |
| `sbom_survey/discovery.py` | `git ls-files`-derived manifest discovery (§5.1) |
| `sbom_survey/tiers.py` | longest-prefix-wins matching, `UntieredManifestError` (§5.3) |
| `sbom_survey/parse/` | `Cargo.lock`/`Cargo.toml`, `package-lock.json`/`package.json`, `uv.lock`/`pyproject.toml`/`requirements*.txt` (§5.5) |
| `sbom_survey/registry.py` | the injectable published-version seam (§5.4) |
| `sbom_survey/survey.py` | `run_survey` · `classify_status` · the staleness rows (§5.4, §5.8) |
| `sbom_survey/cyclonedx.py` | CycloneDX 1.6 assembly + real schema `validate()` (§5.5, §5.7) |
| `sbom_survey/report.py` | `sbom.cdx.json` · `staleness.json` · `staleness.md` (§5.6, §5.8) |
| `sbom_survey/cli.py` | the `argparse` entry point and its exit codes (§5.9) |

## Status

| Slice | Deliverable | State |
|---|---|---|
| **31** | requirements · acceptance criteria · design · **RED tests** | landed |
| **32** | the code that turns the RED tests GREEN | **landed — 24 passed, 0 failed, 0 skipped, 0 errors** |
| **33** | **runs** the tool and writes the survey findings | not started |

This directory now contains the implementation as well: the `sbom_survey` package, its isolated
`pyproject.toml`, the tracked `tiers.toml` rule data and the CLI. Slice 32 turned all 24 acceptance
criteria GREEN **without weakening any assertion Slice 31 wrote**.

**The count moved 23 → 24 at fix-1**, under HITL ruling `seq-168` (`TC-112` (a)) — the single
occasion the Slice-31 suite has been unfrozen. `AC-SBOM-24`
(`tests/test_cyclonedx.py::test_no_component_carries_a_constraint_its_version_violates`) rules that
**no component may carry a `fathomdb:constraint` that its own version does not satisfy**. It exists
because codex §9 round 1 found, by reading, a defect the other 23 criteria could not see: a
declaration was attached to every locked version sharing its name, so `sha2 0.10.9` carried
`constraint = "0.11"` and `thiserror 2.0.18` was tagged `direct` under `"1"`. `depth` and
`edit_sites` are the two fields Slice 33 decides on. The id is numbered last rather than slotted in,
per the `AC-SBOM-23` precedent in design §4, so every existing id keeps its meaning.

## Running the tool

```bash
sbom-survey --repo PATH [--offline | --online] [--out DIR] [--tiers FILE] [--now ISO8601]
sbom-survey --describe
```

**The tier rules are read from the SURVEYED repository — `<repo>/scripts/sbom-survey/tiers.toml` —
never from the installed package.** They are data *about a repository* (every rule is a path prefix
into the surveyed tree), so a copy baked into a wheel would describe whichever repository the wheel
was built from. It also keeps `AC-SBOM-08` / `AC-SBOM-11` grading the very file the survey consumed.

Consequences worth knowing before Slice 33 runs this:

- `pip install ./scripts/sbom-survey` followed by `sbom-survey --repo <that repo> --offline --out DIR`
  works and exits `0`. (Before fix-2 it exited `1` with a bare `FileNotFoundError`, because the
  default was resolved relative to the *package* and no package data is declared.)
- Surveying a repository that does **not** track `scripts/sbom-survey/tiers.toml` requires an
  explicit `--tiers FILE`. That is deliberate: §5.3 rules there is **no catch-all tier rule**, so
  guessing a rule set for an unknown repository would be the silent mis-tag REQ-4 exists to prevent.
  The tool exits `3` and names both the file it wanted and the `--tiers` override.

**Timestamps are validated, never substituted.** `--now` and `SOURCE_DATE_EPOCH` both pass through
one function (`util.parse_timestamp`), and so do `staleness.json`'s `generated` and the CycloneDX
`metadata.timestamp` — so the two artifacts of a single run cannot disagree about when it happened.
A malformed value is **rejected** rather than quietly replaced by the default epoch: this value is
the provenance of the whole run, and §5.6 makes the findings doc's provenance header load-bearing.

| Exit | Meaning |
|---|---|
| `0` | survey written |
| `2` | a tracked manifest has no tier assignment (an offending path on stderr) |
| `3` | a tracked manifest could not be parsed, or the tier rules could not be read |
| `1` | unexpected internal error |
| `64` | bad command line (`EX_USAGE`) — e.g. a malformed `--now` |

§5.9 rules the first four. `64` is deliberately **outside** that set: a bad argument is none of those
things, and argparse's own default for a usage error is `2`, which would collide head-on with
"untiered manifest" and make `AC-SBOM-21`'s signal ambiguous for anyone reading exit codes.

## Running the suite

The mini-project is deliberately **not installed**: `tests/conftest.py` puts `scripts/sbom-survey/`
on `sys.path` itself (and on `PYTHONPATH` for the CLI subprocess). Only the third-party dependencies
declared in `pyproject.toml` need to be importable by the interpreter running pytest — build a venv
for them **outside the repository tree**, and never install anything into the repo's shared `.venv`.

```bash
python3 -m venv /tmp/sbom-survey-venv
/tmp/sbom-survey-venv/bin/pip install \
  'cyclonedx-python-lib[json-validation]>=8.0,<9.0' 'packageurl-python>=0.15,<1.0' \
  'packaging>=24.0,<26.0' 'semver>=3.0,<4.0' 'pytest>=8.0,<10.0'
/tmp/sbom-survey-venv/bin/python -m pytest scripts/sbom-survey/tests -q
```

Expected: **`24 passed, 0 failed, 0 skipped, 0 errors`** (exit code `0`).

- **No test may skip.** A skip is a vacuous green — an ungraded criterion reporting success.
- **No module-level `import sbom_survey`.** The import happens inside each test body via the
  `require()` helper in `tests/conftest.py`, so a broken package produces 24 attributable FAILEDs
  rather than one collection error that hides 23 of them.
- The suite needs **no network**. The published-version lookup is behind an injectable seam; the
  tests inject `OfflineSource` / `StaticSource`, and one test asserts zero socket I/O.
- **`AC-SBOM-10` grades CycloneDX validity with an INDEPENDENT validator** — the upstream
  `cyclonedx-python-lib[json-validation]` one, plus a known-invalid negative control — never with
  `sbom_survey.cyclonedx.validate()`, which would be self-certification. From Slice 32 that
  distribution must be installed: if it is missing the criterion **FAILS** naming what to install.
  It does **not** skip, because an ungraded criterion is a green that means nothing (design §5.7).
- **TC-97.** The only pytest *settings* in this repository live in `src/python/pyproject.toml`, whose
  `pythonpath = ["."]` shadows an installed wheel. That file is **not** an ancestor of
  `scripts/sbom-survey/tests`, so this suite can never inherit it. Since Slice 32 the header does
  print `configfile: pyproject.toml`, pointing at **this directory's own** `pyproject.toml`: from
  pytest 8.1 a `pyproject.toml` found while walking up from the test arguments becomes the rootdir
  anchor even when it carries no `[tool.pytest.ini_options]` table, in which case the applied
  settings are the empty dict (verified: `config.inicfg == {}`, `config.getini("pythonpath") == []`).
  **Do not add a `[tool.pytest.ini_options]` table here, and do not add a `pyproject.toml`,
  `pytest.ini`, `tox.ini` or `setup.cfg` above this directory** — either would start applying real
  settings to this suite.

## Install-then-run smoke (TC-115)

`scripts/sbom-survey/smoke-install-run.sh` guards the **install path**, which the suite above does
not reach: **installing is not verifying an install — invoking what was installed is.** It builds a
throwaway venv **outside** the repo, `pip install`s this project, runs the **installed console
script**, then re-runs the same survey from the source tree and asserts the two agree.

```bash
bash scripts/sbom-survey/smoke-install-run.sh     # exit 0 = PASS
```

It asserts: the console-script file exists and is executable · both runs exit `0` · **two symmetric
provenance** checks — the installed run resolves inside the venv's `site-packages` (invoked under
`env -u PYTHONPATH -u PYTHONHOME`, so an ambient `PYTHONPATH` cannot smuggle the source tree into
the "installed" leg) and the source run resolves under `scripts/sbom-survey` (not the still-installed
copy) · identical artifact **sets** · byte-identical `sbom.cdx.json` / `staleness.json` /
`staleness.md` · and a **vacuity guard** (`summary.components > 0`, non-empty `rows`) — two empty
files are byte-identical. Only the `pip install` needs network; both surveys run `--offline`.

**It is deliberately NOT CI-wired** (steward `seq-172` ruled wiring out, not deferred) — run it by
hand, and do not add it to `agent-test.sh` or `ci.yml`.

## Deliberately NOT wired into CI

This tool is **recurring by design and NOT CI-gating** — it is **informational**
(`plan-0.8.20.md` §3a, HITL 2026-07-29, steward `seq-153`).

- It is **not** registered in `scripts/agent-test.sh`.
- It is **not** referenced by `.github/workflows/ci.yml`.
- It is **not** part of `scripts/agent-verify.sh`, `scripts/check.sh`, or any lint/typecheck scope
  (`ruff` and `pyright` are scoped to `src/python`).

Do not wire it in. **The suite being GREEN does not authorize wiring**: the standing HITL ruling
(`seq-166`) is that the suite may be wired only when it is green **AND** the HITL has approved —
both required, neither sufficient. `tests/test_cli.py::test_tool_declares_non_ci_gating_and_is_absent_from_ci_wiring`
is the standing guard: it greps both wiring files and fails if either grows a reference, so it stays
green precisely **because the wiring is absent**.

## Isolation

The mini-project's own `pyproject.toml` (a Slice 32 artifact) is standalone: not a Cargo workspace
member, not referenced by `src/python/pyproject.toml`, not a dependency of the root `package.json`.
It can never enlarge the published dependency graph or the advisory backlog. Its own dependencies
are surveyed by the tool and tagged `dev-tooling` — the tool appears in its own SBOM, by design.

Generated reports go to `scripts/sbom-survey/out/`, which Slice 32 added to `.gitignore`. Slice 33's
**findings** have a separate tracked home:
`dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md` — the house convention for a dated run
report, weighed against `dev/design/` and `dev/deps/` in design §5.6.

## Scope guard

The tool **never** applies a dependency bump and **never** edits a manifest or a lockfile. Its only
write path is its own gitignored output directory. The survey is an **input to 0.8.22**, which owns
the actual upgrades.
--- scripts/sbom-survey/out/sbom.cdx.json
{
  "components": [
    {
      "bom-ref": "pkg:npm/%40antfu/install-pkg@1.1.0",
      "name": "@antfu/install-pkg",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:edge-kind",
          "value": "resolved"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40antfu/install-pkg@1.1.0",
      "type": "library",
      "version": "1.1.0"
    },
    {
      "bom-ref": "pkg:npm/%40braintree/sanitize-url@7.1.2",
      "name": "@braintree/sanitize-url",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40braintree/sanitize-url@7.1.2",
      "type": "library",
      "version": "7.1.2"
    },
    {
      "bom-ref": "pkg:npm/%40chevrotain/types@11.1.2",
      "name": "@chevrotain/types",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40chevrotain/types@11.1.2",
      "type": "library",
      "version": "11.1.2"
    },
    {
      "bom-ref": "pkg:npm/%40floating-ui/core@1.7.5",
      "name": "@floating-ui/core",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:edge-kind",
          "value": "resolved"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40floating-ui/core@1.7.5",
      "type": "library",
      "version": "1.7.5"
    },
    {
      "bom-ref": "pkg:npm/%40floating-ui/dom@1.7.6",
      "name": "@floating-ui/dom",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:edge-kind",
          "value": "resolved"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40floating-ui/dom@1.7.6",
      "type": "library",
      "version": "1.7.6"
    },
    {
      "bom-ref": "pkg:npm/%40floating-ui/react@0.26.28",
      "name": "@floating-ui/react",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:edge-kind",
          "value": "resolved"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40floating-ui/react@0.26.28",
      "type": "library",
      "version": "0.26.28"
    },
    {
      "bom-ref": "pkg:npm/%40floating-ui/react@0.27.19",
      "name": "@floating-ui/react",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:edge-kind",
          "value": "resolved"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40floating-ui/react@0.27.19",
      "type": "library",
      "version": "0.27.19"
    },
    {
      "bom-ref": "pkg:npm/%40floating-ui/react-dom@2.1.8",
      "name": "@floating-ui/react-dom",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:edge-kind",
          "value": "resolved"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40floating-ui/react-dom@2.1.8",
      "type": "library",
      "version": "2.1.8"
    },
    {
      "bom-ref": "pkg:npm/%40floating-ui/utils@0.2.11",
      "name": "@floating-ui/utils",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40floating-ui/utils@0.2.11",
      "type": "library",
      "version": "0.2.11"
    },
    {
      "bom-ref": "pkg:npm/%40fortawesome/fontawesome-free@7.3.0",
      "name": "@fortawesome/fontawesome-free",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40fortawesome/fontawesome-free@7.3.0",
      "type": "library",
      "version": "7.3.0"
    },
    {
      "bom-ref": "pkg:npm/%40headlessui/react@2.2.10",
      "name": "@headlessui/react",
      "properties": [
        {
          "name": "fathomdb:depth",
          "value": "transitive"
        },
        {
          "name": "fathomdb:edge-kind",
          "value": "resolved"
        },
        {
          "name": "fathomdb:tier",
          "value": "dev-tooling"
        }
      ],
      "purl": "pkg:npm/%40headlessui/react@2.2.10",
      "type": "library",
      "version": "2.2.10"
    },
    {
      "bom-ref": "pkg:npm/%40headlessui/tailwindcss@0.2.2",
      "name": "@headlessui/tailwindcss",
--- scripts/sbom-survey/out/staleness.json
{
  "generated": "1980-01-01T00:00:00+00:00",
  "rows": [
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "2.0.1",
      "locked_version": "1.1.0",
      "lookup_error": null,
      "name": "@antfu/install-pkg",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "7.1.2",
      "locked_version": "7.1.2",
      "lookup_error": null,
      "name": "@braintree/sanitize-url",
      "status": "current",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "13.0.0",
      "locked_version": "11.1.2",
      "lookup_error": null,
      "name": "@chevrotain/types",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "1.8.0",
      "locked_version": "1.7.5",
      "lookup_error": null,
      "name": "@floating-ui/core",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "1.8.0",
      "locked_version": "1.7.6",
      "lookup_error": null,
      "name": "@floating-ui/dom",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "0.27.20",
      "locked_version": "0.26.28",
      "lookup_error": null,
      "name": "@floating-ui/react",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "0.27.20",
      "locked_version": "0.27.19",
      "lookup_error": null,
      "name": "@floating-ui/react",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "2.1.9",
      "locked_version": "2.1.8",
      "lookup_error": null,
      "name": "@floating-ui/react-dom",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "0.2.12",
      "locked_version": "0.2.11",
      "lookup_error": null,
      "name": "@floating-ui/utils",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "7.3.1",
      "locked_version": "7.3.0",
      "lookup_error": null,
      "name": "@fortawesome/fontawesome-free",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "2.2.10",
      "locked_version": "2.2.10",
      "lookup_error": null,
      "name": "@headlessui/react",
      "status": "current",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "0.2.2",
      "locked_version": "0.2.2",
      "lookup_error": null,
      "name": "@headlessui/tailwindcss",
      "status": "current",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "2.0.0",
      "locked_version": "2.0.0",
      "lookup_error": null,
      "name": "@iconify/types",
      "status": "current",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "3.1.4",
      "locked_version": "3.1.3",
      "lookup_error": null,
      "name": "@iconify/utils",
      "status": "outdated",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "3.12.2",
      "locked_version": "3.12.2",
      "lookup_error": null,
      "name": "@internationalized/date",
      "status": "current",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "3.6.7",
      "locked_version": "3.6.7",
      "lookup_error": null,
      "name": "@internationalized/number",
      "status": "current",
      "tier": "dev-tooling"
    },
    {
      "declared_in": [],
      "depth": "transitive",
      "ecosystem": "npm",
      "edit_site_count": 0,
      "edit_sites": [],
      "latest_version": "3.2.9",
      "locked_version": "3.2.9",
      "lookup_error": null,
--- scripts/sbom-survey/out/staleness.md
# Dependency staleness — `sbom-survey`

**Generated:** 1980-01-01T00:00:00+00:00 · **Source:** http · **Components:** 774

**current:** 429 · **outdated:** 331 · **ahead:** 1 · **unknown:** 13 of 774

| ecosystem | name | tier | depth | locked | latest | status | edit sites |
|---|---|---|---|---|---|---|---|
| npm | `@antfu/install-pkg` | dev-tooling | transitive | 1.1.0 | 2.0.1 | outdated | — |
| npm | `@braintree/sanitize-url` | dev-tooling | transitive | 7.1.2 | 7.1.2 | current | — |
| npm | `@chevrotain/types` | dev-tooling | transitive | 11.1.2 | 13.0.0 | outdated | — |
| npm | `@floating-ui/core` | dev-tooling | transitive | 1.7.5 | 1.8.0 | outdated | — |
| npm | `@floating-ui/dom` | dev-tooling | transitive | 1.7.6 | 1.8.0 | outdated | — |
| npm | `@floating-ui/react` | dev-tooling | transitive | 0.26.28 | 0.27.20 | outdated | — |
| npm | `@floating-ui/react` | dev-tooling | transitive | 0.27.19 | 0.27.20 | outdated | — |
| npm | `@floating-ui/react-dom` | dev-tooling | transitive | 2.1.8 | 2.1.9 | outdated | — |
| npm | `@floating-ui/utils` | dev-tooling | transitive | 0.2.11 | 0.2.12 | outdated | — |
| npm | `@fortawesome/fontawesome-free` | dev-tooling | transitive | 7.3.0 | 7.3.1 | outdated | — |
| npm | `@headlessui/react` | dev-tooling | transitive | 2.2.10 | 2.2.10 | current | — |
| npm | `@headlessui/tailwindcss` | dev-tooling | transitive | 0.2.2 | 0.2.2 | current | — |
| npm | `@iconify/types` | dev-tooling | transitive | 2.0.0 | 2.0.0 | current | — |
| npm | `@iconify/utils` | dev-tooling | transitive | 3.1.3 | 3.1.4 | outdated | — |
| npm | `@internationalized/date` | dev-tooling | transitive | 3.12.2 | 3.12.2 | current | — |
| npm | `@internationalized/number` | dev-tooling | transitive | 3.6.7 | 3.6.7 | current | — |
| npm | `@internationalized/string` | dev-tooling | transitive | 3.2.9 | 3.2.9 | current | — |
| npm | `@mermaid-js/layout-elk` | dev-tooling | transitive | 0.2.2 | 0.2.2 | current | — |
| npm | `@mermaid-js/layout-tidy-tree` | dev-tooling | transitive | 0.2.2 | 0.2.2 | current | — |
| npm | `@mermaid-js/mermaid-cli` | dev-tooling | direct | 11.16.0 | 11.16.0 | current | `dev/tools/mermaid/package.json` |
| npm | `@mermaid-js/mermaid-zenuml` | dev-tooling | transitive | 0.2.3 | 0.2.3 | current | — |
| npm | `@mermaid-js/parser` | dev-tooling | transitive | 1.2.0 | 1.2.0 | current | — |
| npm | `@napi-rs/canvas` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-android-arm64` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-darwin-arm64` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-darwin-x64` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-linux-arm-gnueabihf` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-linux-arm64-gnu` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-linux-arm64-musl` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-linux-riscv64-gnu` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-linux-x64-gnu` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-linux-x64-musl` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-win32-arm64-msvc` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@napi-rs/canvas-win32-x64-msvc` | dev-tooling | transitive | 0.1.100 | 1.0.3 | outdated | — |
| npm | `@nodelib/fs.scandir` | dev-tooling | transitive | 2.1.5 | 4.0.1 | outdated | — |
| npm | `@nodelib/fs.stat` | dev-tooling | transitive | 2.0.5 | 4.0.0 | outdated | — |
| npm | `@nodelib/fs.walk` | dev-tooling | transitive | 1.2.8 | 3.0.1 | outdated | — |
| npm | `@puppeteer/browsers` | dev-tooling | transitive | 3.0.6 | 3.0.6 | current | — |
| npm | `@react-aria/focus` | dev-tooling | transitive | 3.22.1 | 3.22.1 | current | — |
| npm | `@react-aria/interactions` | dev-tooling | transitive | 3.28.1 | 3.28.1 | current | — |
| npm | `@react-types/shared` | dev-tooling | transitive | 3.36.0 | 3.36.0 | current | — |
| npm | `@sindresorhus/merge-streams` | dev-tooling | transitive | 4.0.0 | 4.0.0 | current | — |
| npm | `@swc/helpers` | dev-tooling | transitive | 0.5.23 | 0.5.23 | current | — |
| npm | `@tanstack/react-virtual` | dev-tooling | transitive | 3.14.5 | 3.14.9 | outdated | — |
| npm | `@tanstack/virtual-core` | dev-tooling | transitive | 3.17.3 | 3.17.7 | outdated | — |
| npm | `@types/d3` | dev-tooling | transitive | 7.4.3 | 7.4.3 | current | — |
| npm | `@types/d3-array` | dev-tooling | transitive | 3.2.2 | 3.2.2 | current | — |
| npm | `@types/d3-axis` | dev-tooling | transitive | 3.0.6 | 3.0.6 | current | — |
| npm | `@types/d3-brush` | dev-tooling | transitive | 3.0.6 | 3.0.6 | current | — |
| npm | `@types/d3-chord` | dev-tooling | transitive | 3.0.6 | 3.0.6 | current | — |
| npm | `@types/d3-color` | dev-tooling | transitive | 3.1.3 | 3.1.3 | current | — |
| npm | `@types/d3-contour` | dev-tooling | transitive | 3.0.6 | 3.0.6 | current | — |
| npm | `@types/d3-delaunay` | dev-tooling | transitive | 6.0.4 | 6.0.4 | current | — |
| npm | `@types/d3-dispatch` | dev-tooling | transitive | 3.0.7 | 3.0.7 | current | — |
| npm | `@types/d3-drag` | dev-tooling | transitive | 3.0.7 | 3.0.7 | current | — |
| npm | `@types/d3-dsv` | dev-tooling | transitive | 3.0.7 | 3.0.7 | current | — |
| npm | `@types/d3-ease` | dev-tooling | transitive | 3.0.2 | 3.0.2 | current | — |
| npm | `@types/d3-fetch` | dev-tooling | transitive | 3.0.7 | 3.0.7 | current | — |
| npm | `@types/d3-force` | dev-tooling | transitive | 3.0.10 | 3.0.10 | current | — |
| npm | `@types/d3-format` | dev-tooling | transitive | 3.0.4 | 3.0.4 | current | — |
| npm | `@types/d3-geo` | dev-tooling | transitive | 3.1.0 | 3.1.0 | current | — |
| npm | `@types/d3-hierarchy` | dev-tooling | transitive | 3.1.7 | 3.1.7 | current | — |
| npm | `@types/d3-interpolate` | dev-tooling | transitive | 3.0.4 | 3.0.4 | current | — |
| npm | `@types/d3-path` | dev-tooling | transitive | 3.1.1 | 3.1.1 | current | — |
| npm | `@types/d3-polygon` | dev-tooling | transitive | 3.0.2 | 3.0.2 | current | — |
| npm | `@types/d3-quadtree` | dev-tooling | transitive | 3.0.6 | 3.0.6 | current | — |
| npm | `@types/d3-random` | dev-tooling | transitive | 3.0.4 | 3.0.4 | current | — |
| npm | `@types/d3-scale` | dev-tooling | transitive | 4.0.9 | 4.0.9 | current | — |
| npm | `@types/d3-scale-chromatic` | dev-tooling | transitive | 3.1.0 | 3.1.0 | current | — |
| npm | `@types/d3-selection` | dev-tooling | transitive | 3.0.11 | 3.0.11 | current | — |
| npm | `@types/d3-shape` | dev-tooling | transitive | 3.1.8 | 3.1.8 | current | — |
| npm | `@types/d3-time` | dev-tooling | transitive | 3.0.4 | 3.0.4 | current | — |
| npm | `@types/d3-time-format` | dev-tooling | transitive | 4.0.3 | 4.0.3 | current | — |
| npm | `@types/d3-timer` | dev-tooling | transitive | 3.0.2 | 3.0.2 | current | — |
| npm | `@types/d3-transition` | dev-tooling | transitive | 3.0.9 | 3.0.9 | current | — |
| npm | `@types/d3-zoom` | dev-tooling | transitive | 3.0.8 | 3.0.8 | current | — |
| npm | `@types/debug` | dev-tooling | transitive | 4.1.13 | 4.1.13 | current | — |
| npm | `@types/geojson` | dev-tooling | transitive | 7946.0.16 | 7946.0.16 | current | — |
| npm | `@types/katex` | dev-tooling | transitive | 0.16.8 | 0.16.8 | current | — |
| npm | `@types/ms` | dev-tooling | transitive | 2.1.0 | 2.1.0 | current | — |
| npm | `@types/trusted-types` | dev-tooling | transitive | 2.0.7 | 2.0.7 | current | — |
| npm | `@types/unist` | dev-tooling | transitive | 2.0.11 | 3.0.3 | outdated | — |
| npm | `@upsetjs/venn.js` | dev-tooling | transitive | 2.0.0 | 2.0.0 | current | — |
| npm | `@zenuml/core` | dev-tooling | transitive | 3.50.1 | 4.2.0 | outdated | — |
| npm | `ansi-regex` | dev-tooling | transitive | 6.2.2 | 6.2.2 | current | — |
| npm | `ansi-styles` | dev-tooling | transitive | 6.2.3 | 7.0.0 | outdated | — |
| npm | `antlr4` | dev-tooling | transitive | 4.11.0 | 4.13.2 | outdated | — |
| npm | `argparse` | dev-tooling | transitive | 2.0.1 | 3.0.0 | outdated | — |
| npm | `aria-hidden` | dev-tooling | transitive | 1.2.6 | 1.2.6 | current | — |
| npm | `braces` | dev-tooling | transitive | 3.0.3 | 3.0.3 | current | — |
| npm | `chalk` | dev-tooling | transitive | 5.6.2 | 6.0.0 | outdated | — |
| npm | `character-entities` | dev-tooling | transitive | 2.0.2 | 2.0.2 | current | — |
| npm | `character-entities-legacy` | dev-tooling | transitive | 3.0.0 | 3.0.0 | current | — |
| npm | `character-reference-invalid` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `chromium-bidi` | dev-tooling | transitive | 16.0.1 | 17.0.2 | outdated | — |
| npm | `class-variance-authority` | dev-tooling | transitive | 0.7.1 | 0.7.1 | current | — |
| npm | `cliui` | dev-tooling | transitive | 9.0.1 | 9.0.1 | current | — |
| npm | `clsx` | dev-tooling | transitive | 2.1.1 | 2.1.1 | current | — |
| npm | `color-name` | dev-tooling | transitive | 2.1.0 | 2.1.1 | outdated | — |
| npm | `color-string` | dev-tooling | transitive | 2.1.4 | 2.1.4 | current | — |
| npm | `commander` | dev-tooling | transitive | 13.1.0 | 15.0.0 | outdated | — |
| npm | `commander` | dev-tooling | transitive | 7.2.0 | 15.0.0 | outdated | — |
| npm | `commander` | dev-tooling | transitive | 8.3.0 | 15.0.0 | outdated | — |
| npm | `cose-base` | dev-tooling | transitive | 1.0.3 | 2.2.0 | outdated | — |
| npm | `cose-base` | dev-tooling | transitive | 2.2.0 | 2.2.0 | current | — |
| npm | `cytoscape` | dev-tooling | transitive | 3.34.0 | 3.34.0 | current | — |
| npm | `cytoscape-cose-bilkent` | dev-tooling | transitive | 4.1.0 | 4.1.0 | current | — |
| npm | `cytoscape-fcose` | dev-tooling | transitive | 2.2.0 | 2.2.0 | current | — |
| npm | `d3` | dev-tooling | transitive | 7.9.0 | 7.9.0 | current | — |
| npm | `d3-array` | dev-tooling | transitive | 2.12.1 | 3.2.4 | outdated | — |
| npm | `d3-array` | dev-tooling | transitive | 3.2.4 | 3.2.4 | current | — |
| npm | `d3-axis` | dev-tooling | transitive | 3.0.0 | 3.0.0 | current | — |
| npm | `d3-brush` | dev-tooling | transitive | 3.0.0 | 3.0.0 | current | — |
| npm | `d3-chord` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-color` | dev-tooling | transitive | 3.1.0 | 3.1.0 | current | — |
| npm | `d3-contour` | dev-tooling | transitive | 4.0.2 | 4.0.2 | current | — |
| npm | `d3-delaunay` | dev-tooling | transitive | 6.0.4 | 6.0.4 | current | — |
| npm | `d3-dispatch` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-drag` | dev-tooling | transitive | 3.0.0 | 3.0.0 | current | — |
| npm | `d3-dsv` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-ease` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-fetch` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-force` | dev-tooling | transitive | 3.0.0 | 3.0.0 | current | — |
| npm | `d3-format` | dev-tooling | transitive | 3.1.2 | 3.1.2 | current | — |
| npm | `d3-geo` | dev-tooling | transitive | 3.1.1 | 3.1.1 | current | — |
| npm | `d3-hierarchy` | dev-tooling | transitive | 3.1.2 | 3.1.2 | current | — |
| npm | `d3-interpolate` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-path` | dev-tooling | transitive | 1.0.9 | 3.1.0 | outdated | — |
| npm | `d3-path` | dev-tooling | transitive | 3.1.0 | 3.1.0 | current | — |
| npm | `d3-polygon` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-quadtree` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-random` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-sankey` | dev-tooling | transitive | 0.12.3 | 0.12.3 | current | — |
| npm | `d3-scale` | dev-tooling | transitive | 4.0.2 | 4.0.2 | current | — |
| npm | `d3-scale-chromatic` | dev-tooling | transitive | 3.1.0 | 3.1.0 | current | — |
| npm | `d3-selection` | dev-tooling | transitive | 3.0.0 | 3.0.0 | current | — |
| npm | `d3-shape` | dev-tooling | transitive | 1.3.7 | 3.2.0 | outdated | — |
| npm | `d3-shape` | dev-tooling | transitive | 3.2.0 | 3.2.0 | current | — |
| npm | `d3-time` | dev-tooling | transitive | 3.1.0 | 3.1.0 | current | — |
| npm | `d3-time-format` | dev-tooling | transitive | 4.1.0 | 4.1.0 | current | — |
| npm | `d3-timer` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-transition` | dev-tooling | transitive | 3.0.1 | 3.0.1 | current | — |
| npm | `d3-zoom` | dev-tooling | transitive | 3.0.0 | 3.0.0 | current | — |
| npm | `dagre-d3-es` | dev-tooling | transitive | 7.0.14 | 7.0.14 | current | — |
| npm | `dayjs` | dev-tooling | transitive | 1.11.21 | 1.11.21 | current | — |
| npm | `debug` | dev-tooling | transitive | 4.4.3 | 4.4.3 | current | — |
| npm | `decode-named-character-reference` | dev-tooling | transitive | 1.3.0 | 1.3.0 | current | — |
| npm | `delaunator` | dev-tooling | transitive | 5.1.0 | 5.1.0 | current | — |
| npm | `dequal` | dev-tooling | transitive | 2.0.3 | 2.0.3 | current | — |
| npm | `devlop` | dev-tooling | transitive | 1.1.0 | 1.1.0 | current | — |
| npm | `devtools-protocol` | dev-tooling | transitive | 0.0.1638949 | 0.0.1669207 | outdated | — |
| npm | `dompurify` | dev-tooling | transitive | 3.4.11 | 3.4.12 | outdated | — |
| npm | `elkjs` | dev-tooling | transitive | 0.9.3 | 0.12.0 | outdated | — |
| npm | `emoji-regex` | dev-tooling | transitive | 10.6.0 | 10.6.0 | current | — |
| npm | `entities` | dev-tooling | transitive | 4.5.0 | 8.0.0 | outdated | — |
| npm | `es-toolkit` | dev-tooling | transitive | 1.49.0 | 1.50.0 | outdated | — |
| npm | `escalade` | dev-tooling | transitive | 3.2.0 | 3.2.0 | current | — |
| npm | `fast-glob` | dev-tooling | transitive | 3.3.3 | 3.3.3 | current | — |
| npm | `fastq` | dev-tooling | transitive | 1.20.1 | 1.20.1 | current | — |
| npm | `fill-range` | dev-tooling | transitive | 7.1.1 | 7.1.1 | current | — |
| npm | `get-caller-file` | dev-tooling | transitive | 2.0.5 | 2.0.5 | current | — |
| npm | `get-east-asian-width` | dev-tooling | transitive | 1.6.0 | 1.6.0 | current | — |
| npm | `glob-parent` | dev-tooling | transitive | 5.1.2 | 6.0.2 | outdated | — |
| npm | `globby` | dev-tooling | transitive | 16.2.0 | 16.2.2 | outdated | — |
| npm | `hachure-fill` | dev-tooling | transitive | 0.5.2 | 0.5.2 | current | — |
| npm | `highlight.js` | dev-tooling | transitive | 11.11.1 | 11.11.1 | current | — |
| npm | `html-to-image` | dev-tooling | transitive | 1.11.13 | 1.11.13 | current | — |
| npm | `iconv-lite` | dev-tooling | transitive | 0.6.3 | 0.7.3 | outdated | — |
| npm | `ignore` | dev-tooling | transitive | 7.0.5 | 7.0.6 | outdated | — |
| npm | `import-meta-resolve` | dev-tooling | transitive | 4.2.0 | 4.2.0 | current | — |
| npm | `internmap` | dev-tooling | transitive | 1.0.1 | 2.0.3 | outdated | — |
| npm | `internmap` | dev-tooling | transitive | 2.0.3 | 2.0.3 | current | — |
| npm | `is-alphabetical` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `is-alphanumerical` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `is-decimal` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `is-extglob` | dev-tooling | transitive | 2.1.1 | 2.1.1 | current | — |
| npm | `is-glob` | dev-tooling | transitive | 4.0.3 | 4.0.3 | current | — |
| npm | `is-hexadecimal` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `is-number` | dev-tooling | transitive | 7.0.0 | 7.0.0 | current | — |
| npm | `is-path-inside` | dev-tooling | transitive | 4.0.0 | 4.0.0 | current | — |
| npm | `jotai` | dev-tooling | transitive | 2.20.1 | 2.20.2 | outdated | — |
| npm | `js-yaml` | dev-tooling | transitive | 4.2.0 | 5.2.2 | outdated | — |
| npm | `jsonc-parser` | dev-tooling | transitive | 3.3.1 | 3.3.1 | current | — |
| npm | `jsonpointer` | dev-tooling | transitive | 5.0.1 | 5.0.1 | current | — |
| npm | `katex` | dev-tooling | transitive | 0.16.47 | 0.18.1 | outdated | — |
| npm | `khroma` | dev-tooling | transitive | 2.1.0 | 2.1.0 | current | — |
| npm | `layout-base` | dev-tooling | transitive | 1.0.2 | 2.0.1 | outdated | — |
| npm | `layout-base` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `lilconfig` | dev-tooling | transitive | 3.1.3 | 3.1.3 | current | — |
| npm | `linkify-it` | dev-tooling | transitive | 5.0.1 | 6.1.0 | outdated | — |
| npm | `lodash-es` | dev-tooling | transitive | 4.18.1 | 4.18.1 | current | — |
| npm | `markdown-it` | dev-tooling | transitive | 14.2.0 | 14.3.0 | outdated | — |
| npm | `markdownlint` | dev-tooling | transitive | 0.41.0 | 0.41.1 | outdated | — |
| npm | `markdownlint-cli2` | dev-tooling | direct | 0.23.0 | 0.23.2 | outdated | `package.json` |
| npm | `markdownlint-cli2-formatter-default` | dev-tooling | transitive | 0.0.6 | 0.0.6 | current | — |
| npm | `marked` | dev-tooling | transitive | 16.4.2 | 18.0.7 | outdated | — |
| npm | `marked` | dev-tooling | transitive | 4.3.0 | 18.0.7 | outdated | — |
| npm | `mdurl` | dev-tooling | transitive | 2.0.0 | 2.1.0 | outdated | — |
| npm | `merge2` | dev-tooling | transitive | 1.4.1 | 1.4.1 | current | — |
| npm | `mermaid` | dev-tooling | transitive | 11.16.0 | 11.16.0 | current | — |
| npm | `micromark` | dev-tooling | transitive | 4.0.2 | 4.0.2 | current | — |
| npm | `micromark-core-commonmark` | dev-tooling | transitive | 2.0.3 | 2.0.3 | current | — |
| npm | `micromark-extension-directive` | dev-tooling | transitive | 4.0.0 | 4.0.0 | current | — |
| npm | `micromark-extension-gfm-autolink-literal` | dev-tooling | transitive | 2.1.0 | 2.1.0 | current | — |
| npm | `micromark-extension-gfm-footnote` | dev-tooling | transitive | 2.1.0 | 2.1.0 | current | — |
| npm | `micromark-extension-gfm-table` | dev-tooling | transitive | 2.1.1 | 2.1.1 | current | — |
| npm | `micromark-extension-math` | dev-tooling | transitive | 3.1.0 | 3.1.0 | current | — |
| npm | `micromark-factory-destination` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `micromark-factory-label` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `micromark-factory-space` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `micromark-factory-title` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
| npm | `micromark-factory-whitespace` | dev-tooling | transitive | 2.0.1 | 2.0.1 | current | — |
--- scripts/sbom-survey/pyproject.toml
# `sbom-survey` — the dependency-survey mini-project (0.8.20 Slice 32).
#
# ISOLATION (design §2). This project is deliberately standalone:
#   * NOT a member of the Cargo workspace,
#   * NOT referenced by `src/python/pyproject.toml`,
#   * NOT a dependency of the root `package.json`.
# It can therefore never enlarge the published dependency graph or the advisory
# backlog. Its own dependencies are surveyed BY the tool and tier `dev-tooling`
# — the tool appears in its own SBOM, which is the correct answer.
#
# TC-97 — THERE IS DELIBERATELY NO `[tool.pytest.ini_options]` TABLE HERE.
# pytest only treats a `pyproject.toml` as its *configfile* when that table is
# present. Adding one would make this file the configfile for
# `scripts/sbom-survey/tests` and start importing settings (the repo's only
# other pytest config, `src/python/pyproject.toml`, sets `pythonpath = ["."]`,
# which shadows an installed wheel). The suite must run with NO configfile: the
# pytest header prints `rootdir:` and no `configfile:` line. Do not add one.

[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[project]
name = "sbom-survey"
version = "0.1.0"
description = "CycloneDX 1.6 dependency survey over FathomDB's tracked manifests (Library Sweep #3)"
readme = "README.md"
requires-python = ">=3.11"
license = { text = "Apache-2.0" }

# Design §5.7 names every one of these and why stdlib is insufficient. The set
# is kept deliberately small: a dependency-hygiene tool with a bloated
# dependency set is self-refuting.
#
# Deliberately NOT taken (§5.7, binding):
#   * no HTTP client   — stdlib `urllib.request` covers three GET-JSON calls;
#   * no TOML library  — `tomllib` is stdlib from 3.11 (hence requires-python);
#   * no `jsonschema`  — the `json-validation` extra already binds a validator
#                        to the normative 1.6 schema; a second one would drift;
#   * no `GitPython`   — one `git ls-files -z` via `subprocess` is the whole
#                        git surface;
#   * no setup.py/AST tooling — §5.2.
dependencies = [
    "cyclonedx-python-lib[json-validation]>=8.0,<9.0",
    "packageurl-python>=0.15,<1.0",
    "packaging>=24.0,<26.0",
    "semver>=3.0,<4.0",
]

[project.optional-dependencies]
# Dev-only. `pytest` is NOT a runtime dependency of the tool.
dev = ["pytest>=8.0,<10.0"]

[project.scripts]
sbom-survey = "sbom_survey.cli:console_main"

[tool.setuptools]
packages = ["sbom_survey", "sbom_survey.parse"]
--- scripts/sbom-survey/sbom_survey/__init__.py
"""`sbom-survey` — a CycloneDX 1.6 dependency survey over FathomDB's tracked manifests.

Spec of record: `dev/design/0.8.20-slice-31-sbom-survey-tool.md` (0.8.20 Slice 31).
This package is the Slice 32 implementation of that spec.

The tool is **informational and NOT CI-gating** (`plan-0.8.20.md` §3a, HITL
2026-07-29, steward `seq-153`). It never applies a dependency bump and never
edits a manifest or a lockfile; its only write path is its own gitignored
output directory.
"""

from __future__ import annotations

__all__ = ["TIER_VOCABULARY", "__version__"]

__version__ = "0.1.0"

#: The HITL-ruled tier vocabulary, in the ruled order — `shipped` first because
#: it is the tier that outranks the others in Slice 33's triage (design §5.9).
#: `fixture` is an EXCLUSION REASON, never a fourth tier (§5.2): putting
#: deliberately fake packages in the component list would hand a vulnerability
#: feed real-looking phantoms.
TIER_VOCABULARY: tuple[str, str, str] = ("shipped", "dev-tooling", "eval-only")
--- scripts/sbom-survey/sbom_survey/__main__.py
"""`python -m sbom_survey` — the entry point the acceptance suite drives."""

from __future__ import annotations

import sys

from .cli import main

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
--- scripts/sbom-survey/sbom_survey/cli.py
"""The `sbom-survey` command line (design §5.9).

```text
sbom-survey --repo PATH [--offline | --online] [--out DIR] [--tiers FILE] [--now ISO8601]
sbom-survey --describe
```

| Exit | Meaning |
|------|---------|
| `0`  | survey written |
| `2`  | a tracked manifest has no tier assignment (an offending path on stderr) |
| `3`  | a tracked manifest could not be parsed, or the tier rules could not be read |
| `1`  | unexpected internal error |
| `64` | bad command line (`EX_USAGE`) — NOT one of §5.9's ruled codes, on purpose |

§5.9 rules the first four. `64` is added rather than reusing one of them because
a malformed argument is none of those things, and argparse's own default for a
usage error is `2` — which would collide head-on with "untiered manifest" and
make that signal ambiguous for anyone reading exit codes.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import TIER_VOCABULARY, __version__
from .parse import ManifestParseError
from .paths import DEFAULT_REPORT_DIR
from .registry import HttpRegistrySource, OfflineSource
from .report import write_reports
from .survey import run_survey
from .tiers import TierRuleFileError, UntieredManifestError, load_tier_map
from .util import TimestampFormatError, normalize_timestamp

__all__ = ["build_parser", "console_main", "describe", "main"]

EXIT_OK = 0
EXIT_INTERNAL = 1
EXIT_UNTIERED = 2
EXIT_UNPARSEABLE = 3

#: A BAD COMMAND LINE, and deliberately NOT one of §5.9's four ruled codes.
#:
#: §5.9 fixes 0 = written, 2 = untiered manifest, 3 = unparseable manifest,
#: 1 = unexpected internal error. A malformed argument is none of those, and
#: argparse's own default for a usage error is 2 — which would collide head-on
#: with "a tracked manifest has no tier assignment" and make `AC-SBOM-21`'s
#: signal ambiguous for any consumer reading exit codes.
#:
#: 64 is `EX_USAGE` from BSD `sysexits.h`, the long-standing convention for
#: exactly this case: outside the ruled set, so it cannot overload a ruled
#: meaning, and self-documenting rather than arbitrary. `_Parser` below routes
#: EVERY argparse usage error here, so the CLI is internally consistent — an
#: unknown flag and a malformed `--now` report the same way.
EXIT_USAGE = 64


class _Parser(argparse.ArgumentParser):
    """`argparse.ArgumentParser` whose usage errors exit `EXIT_USAGE`, not 2."""

    def error(self, message: str):  # noqa: D102 - argparse contract
        self.print_usage(sys.stderr)
        print(f"{self.prog}: error: {message}", file=sys.stderr)
        raise SystemExit(EXIT_USAGE)


def describe() -> dict:
    """The machine-readable "this is informational" declaration (AC-SBOM-19).

    `tiers` publishes the ruled vocabulary IN THE RULED ORDER so downstream
    tooling can discover it without importing this package.
    """
    return {
        "name": "sbom-survey",
        "version": __version__,
        "ci_gating": False,
        "recurring": True,
        "tiers": list(TIER_VOCABULARY),
    }


def build_parser() -> argparse.ArgumentParser:
    parser = _Parser(
        prog="sbom-survey",
        description=(
            "CycloneDX 1.6 dependency survey over the manifests tracked on main."
            " Informational and NOT CI-gating."
        ),
    )
    parser.add_argument("--repo", default=".", help="repository root to survey")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--offline",
        action="store_true",
        help="do not consult any registry; every row degrades to `unknown`",
    )
    mode.add_argument(
        "--online",
        action="store_true",
        help="consult crates.io / the npm registry / PyPI for published versions",
    )
    parser.add_argument("--out", default=None, help=f"output directory (default: {DEFAULT_REPORT_DIR})")
    parser.add_argument("--tiers", default=None, help="tier rule file (default: the tracked tiers.toml)")
    # DELIBERATELY `None`, NEVER a wall-clock stamp. `None` defers to
    # `survey.resolve_timestamp()`, which is the SAME function `run_survey` uses
    # for its in-process default, so the CLI default and the in-process default
    # are equal by construction and `SOURCE_DATE_EPOCH` keeps working. An
    # argparse default of `datetime.now()` here would sail straight past an
    # in-process determinism test, which is why §5.8 grades this path
    # separately.
    parser.add_argument("--now", default=None, help="ISO-8601 timestamp (default: a FIXED epoch)")
    parser.add_argument(
        "--describe",
        action="store_true",
        help="print the tool's self-description as JSON and exit",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    # Validate the advertised `--now ISO8601` contract HERE, before any work,
    # so a typo is a usage error rather than a survey that runs to completion
    # and writes artifacts disagreeing about when it happened.
    if args.now is not None:
        try:
            normalize_timestamp(args.now, source="--now")
        except TimestampFormatError as exc:
            parser.error(str(exc))

    if args.describe:
        print(json.dumps(describe(), indent=2, sort_keys=True))
        return EXIT_OK

    repo_root = Path(args.repo).resolve()
    out_dir = Path(args.out) if args.out else repo_root / DEFAULT_REPORT_DIR

    published = OfflineSource() if not args.online else HttpRegistrySource()

    try:
        tier_map = load_tier_map(args.tiers) if args.tiers else None
    except TierRuleFileError as exc:
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE
    except OSError as exc:
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE

    try:
        survey = run_survey(
            repo_root,
            published=published,
            tier_map=tier_map,
            now=args.now,
        )
    except UntieredManifestError as exc:
        # REQ-4 at the CLI boundary: name AN offending path so the fix is
        # obvious, and never exit 0 with an untagged component.
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNTIERED
    except TierRuleFileError as exc:
        # The DEFAULT tier-rule file is resolved from `--repo`, so this is
        # reachable without `--tiers` and must not fall through to the bare
        # `Exception` handler below: an unreadable rule file is a legible input
        # problem, not an internal error (codex §9 round 2 [P1]).
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE
    except TimestampFormatError as exc:
        # Reachable via SOURCE_DATE_EPOCH, which `--now` validation above does
        # not cover. Same door, same code.
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_USAGE
    except ManifestParseError as exc:
        print(f"sbom-survey: {exc}", file=sys.stderr)
        return EXIT_UNPARSEABLE
    except Exception as exc:  # noqa: BLE001 - anything else is an internal error
        print(f"sbom-survey: unexpected internal error: {exc!r}", file=sys.stderr)
        return EXIT_INTERNAL

    written = write_reports(survey, out_dir)
    summary = survey.summary()
    print(
        f"sbom-survey: {summary['components']} components"
        f" ({summary['direct']} direct, {summary['transitive']} transitive),"
        f" {summary['unknown']} unknown, {summary['excluded_manifests']} manifests excluded"
    )
    for path in written:
        print(f"  wrote {path}")
    return EXIT_OK


def console_main() -> int:  # pragma: no cover - console-script shim
    return main(sys.argv[1:])
--- scripts/sbom-survey/sbom_survey/constraints.py
"""Does a declared constraint admit a particular locked version?

**Why this exists** (fix-1, codex §9 round 1 `[P2]`). A manifest declares a
dependency by NAME and RANGE; a lockfile resolves it to one or more concrete
versions. Attaching a declaration to *every* locked version of that name is
wrong whenever the lock carries more than one, and it was wrong on this
repository in a way that corrupts the two fields Slice 33 makes its
surgical/not-surgical call on:

```text
sha2       0.10.9  direct  constraint='0.11'   <- false: 0.11 cannot resolve to 0.10.9
thiserror  2.0.18  direct  constraint='1'      <- false: ^1 cannot resolve to 2.0.18
tokenizers 0.22.2  direct  constraint='0.20'   <- false
```

So the survey asks this module which locked entries a constraint can actually
resolve to, and attaches the declaration only to those.

**The honesty rule this module is built around.** `matches()` returns
`True`/`False` only when it genuinely evaluated the constraint, and `None` when
it could not — an unparseable range, an opaque specifier (`workspace:`, a git
URL, a filesystem path), an unparseable version. `None` is NOT "no", and the
caller must never turn it into "attach to everything": widening on uncertainty
is exactly the defect this module was written to remove. See
`survey._Assembler.add_declarations` for what the caller does instead.

**No new dependency was taken for this.** PEP 440 is evaluated by `packaging`,
already declared for exactly that purpose (design §5.7); semver ranges are
evaluated here against `semver.Version`, also already declared. The supported
range grammar is deliberately bounded and everything outside it degrades to
`None` rather than to a guess.
"""

from __future__ import annotations

import re

import semver
from packaging.specifiers import InvalidSpecifier, SpecifierSet
from packaging.version import InvalidVersion, Version

__all__ = ["matches"]

#: Placeholders the manifest parsers emit when a dependency carries no version
#: range at all (`foo.workspace = true` with no root pin, `{ path = … }`,
#: `{ git = … }`). These are not ranges and must never be evaluated as one.
_OPAQUE_CONSTRAINTS = frozenset({"workspace", "path", "git"})

#: A constraint containing any of these is a URL, an alias, a filesystem path or
#: a dist-tag (`npm:pkg@^1`, `workspace:*`, `file:../x`, `git+https://…`,
#: `latest`), none of which this grammar covers.
_OPAQUE_CHARS = (":", "/", "\\")

_ANY = frozenset({"*", "x", "X", ""})

_HYPHEN_RANGE = re.compile(r"^\s*(v?[0-9][^\s]*)\s+-\s+(v?[0-9][^\s]*)\s*$")

_COMPARATOR = re.compile(
    r"\s*(\^|~>|~|>=|<=|>|<|==|=)?\s*(v?[0-9xX*][0-9A-Za-z.\-+*xX]*)\s*"
)

_PARTIAL = re.compile(
    r"^v?(\d+|[xX*])"
    r"(?:\.(\d+|[xX*]))?"
    r"(?:\.(\d+|[xX*]))?"
    r"(?:-([0-9A-Za-z.\-]+))?"
    r"(?:\+[0-9A-Za-z.\-]+)?$"
)


def matches(ecosystem: str, constraint: str | None, version: str | None) -> bool | None:
    """`True` / `False` if the constraint was evaluated, `None` if it could not be.

    `None` means "unknown", never "no" and never "yes".
    """
    if constraint is None or version is None:
        return None
    text = constraint.strip()
    if text in _ANY:
        return True
    if ecosystem == "pypi":
        return _matches_pep440(text, version)
    if ecosystem in ("cargo", "npm"):
        return _matches_semver(ecosystem, text, version)
    return None


def _matches_pep440(constraint: str, version: str) -> bool | None:
    try:
        specifier = SpecifierSet(constraint)
    except InvalidSpecifier:
        return None
    try:
        parsed = Version(version)
    except InvalidVersion:
        return None
    # `prereleases=True` because the question is "could the lock have resolved
    # this constraint to this version", and a lock that pinned a prerelease is
    # evidence that it could.
    return specifier.contains(parsed, prereleases=True)


def _matches_semver(ecosystem: str, constraint: str, version: str) -> bool | None:
    if constraint in _OPAQUE_CONSTRAINTS:
        return None
    if any(char in constraint for char in _OPAQUE_CHARS):
        return None
    try:
        actual = semver.Version.parse(version)
    except (TypeError, ValueError):
        return None

    saw_true = False
    for alternative in constraint.split("||"):
        verdict = _alternative(ecosystem, alternative, actual)
        if verdict is None:
            return None  # one unparseable alternative makes the whole thing unknown
        saw_true = saw_true or verdict
    return saw_true


def _alternative(ecosystem: str, text: str, actual: semver.Version) -> bool | None:
    """A whitespace/comma-separated comparator set — every comparator must hold."""
    cleaned = text.replace(",", " ").strip()
    if not cleaned:
        return True

    hyphen = _HYPHEN_RANGE.match(cleaned)
    if hyphen:
        pairs: list[tuple[str | None, str]] = [
            (">=", hyphen.group(1)),
            ("<=", hyphen.group(2)),
        ]
    else:
        pairs = []
        position = 0
        while position < len(cleaned):
            match = _COMPARATOR.match(cleaned, position)
            if match is None or match.end() == position:
                return None  # something outside the grammar: unknown, not "no"
            pairs.append((match.group(1), match.group(2)))
            position = match.end()
        if not pairs:
            return None

    for operator, raw in pairs:
        verdict = _comparator(ecosystem, operator, raw, actual)
        if verdict is None:
            return None
        if not verdict:
            return False
    return True


def _segment(raw: str | None) -> int | None:
    if raw is None or raw in ("x", "X", "*"):
        return None
    return int(raw)


def _version(major: int, minor: int | None, patch: int | None, pre: str | None = None):
    return semver.Version(major, minor or 0, patch or 0, prerelease=pre)


def _comparator(
    ecosystem: str,
    operator: str | None,
    raw: str,
    actual: semver.Version,
) -> bool | None:
    parsed = _PARTIAL.match(raw)
    if parsed is None:
        return None
    major = _segment(parsed.group(1))
    if major is None:
        return True  # a bare `x` / `*` major admits everything
    minor = _segment(parsed.group(2))
    patch = _segment(parsed.group(3))
    pre = parsed.group(4)

    if operator is None:
        # Cargo reads a bare requirement as CARET (`serde = "1"` is `^1`); npm
        # reads a bare full version as EXACT and a bare partial as a prefix
        # range. Coercing one to the other would silently mis-match, which is
        # the failure class this module exists to remove.
        operator = "^" if ecosystem == "cargo" else "="

    low = _version(major, minor, patch, pre)

    if operator == "^":
        if major > 0 or minor is None:
            high = _version(major + 1, 0, 0)
        elif minor > 0 or patch is None:
            high = _version(0, minor + 1, 0)
        else:
            high = _version(0, 0, patch + 1)
        return low <= actual < high

    if operator in ("~", "~>"):
        high = _version(major + 1, 0, 0) if minor is None else _version(major, minor + 1, 0)
        return low <= actual < high

    if operator == ">=":
        return actual >= low
    if operator == ">":
        return actual > low
    if operator == "<=":
        return actual <= low
    if operator == "<":
        return actual < low

    # `=` / `==`: exact when fully specified, a prefix range when partial.
    if minor is None:
        return low <= actual < _version(major + 1, 0, 0)
    if patch is None:
        return low <= actual < _version(major, minor + 1, 0)
    return actual == low
--- scripts/sbom-survey/sbom_survey/cyclonedx.py
"""CycloneDX 1.6 document assembly and schema validation (design §5.5, §5.7, REQ-6).

The document is BUILT and SERIALIZED by the upstream `cyclonedx-python-lib`
model, not hand-rolled: CycloneDX is a versioned spec with normative schemas and
`bom-ref` semantics, and hand-rolled JSON is exactly the "SBOM no consumer will
validate" failure REQ-6 exists to prevent.

`validate()` runs the **real** normative 1.6 schema from the
`cyclonedx-python-lib[json-validation]` extra. It is deliberately NOT the oracle
the acceptance suite grades with — that would be self-certification — and the
suite cross-checks it against the upstream validator in BOTH directions, so a
stub returning `None` unconditionally is caught.

NOTE ON THE MODULE NAME. This module is `sbom_survey.cyclonedx`; `cyclonedx` on
its own is the third-party distribution. Python 3 imports are absolute, so the
`from cyclonedx… import …` lines below reach the upstream package, never this
file.
"""

from __future__ import annotations

import json
import uuid
from typing import TYPE_CHECKING, Any

from cyclonedx.model import Property
from cyclonedx.model.bom import Bom
from cyclonedx.model.component import Component, ComponentType
from cyclonedx.output.json import JsonV1Dot6
from cyclonedx.schema import SchemaVersion
from packageurl import PackageURL

from .util import parse_timestamp

if TYPE_CHECKING:  # pragma: no cover - typing only
    from .survey import Survey

__all__ = ["ROOT_BOM_REF", "build_document", "validate"]

#: The `metadata.component` ref. Deliberately not a purl, so it can never
#: collide with a component `bom-ref` (which always is one).
ROOT_BOM_REF = "fathomdb-repository"

#: A fixed namespace so the UUIDv5 `serialNumber` is a pure function of the
#: component set — not `uuid4`, which would make every re-run diff (REQ-13).
_SERIAL_NAMESPACE = uuid.UUID("6f0d5c9a-2f27-5b3e-9f2e-0a3b3d4c5e6f")


#: NOTE THE ABSENCE OF A LOCAL PARSER HERE.
#:
#: This module used to carry its own `_timestamp()` that swallowed a
#: `ValueError` and returned a hardcoded 1980 epoch. That made a malformed
#: `--now` produce TWO artifacts from ONE run that disagreed about when the run
#: happened: `staleness.json` recorded the raw string, this document recorded
#: the silent fallback (codex §9 round 3 `[P3]`). The stamp is now taken from
#: `util.parse_timestamp` — the same function `survey.resolve_timestamp` uses —
#: so there is a SINGLE resolution and a future edit cannot re-open the gap on
#: one side only. It has no fallback: an unparseable stamp raises.


def build_document(survey: Survey) -> tuple[dict[str, Any], str]:
    """`(document, serialized)` for `survey` — deterministic byte-for-byte."""
    root = Component(
        name="fathomdb",
        type=ComponentType.APPLICATION,
        bom_ref=ROOT_BOM_REF,
    )

    bom = Bom()
    bom.metadata.component = root
    bom.metadata.timestamp = parse_timestamp(
        survey.timestamp, source="survey.timestamp"
    )
    bom.serial_number = uuid.uuid5(
        _SERIAL_NAMESPACE,
        "\n".join(sorted(component.purl for component in survey.components)),
    )

    # The exclusions are mirrored into the BOM itself so that "these tracked
    # manifests were knowingly left out" travels with the document rather than
    # living only in the tool's own head (§5.2, auditable exclusion).
    for excluded in survey.excluded:
        bom.metadata.properties.add(
            Property(name="fathomdb:excluded-manifest", value=excluded.path)
        )
        bom.metadata.properties.add(
            Property(
                name="fathomdb:excluded-manifest-reason",
                value=f"{excluded.path}={excluded.reason}",
            )
        )

    by_purl: dict[str, Component] = {}
    direct: list[Component] = []
    for surveyed in survey.components:
        properties = [
            Property(name="fathomdb:tier", value=surveyed.tier),
            Property(name="fathomdb:depth", value=surveyed.depth),
        ]
        if surveyed.version is None:
            properties.append(Property(name="fathomdb:resolution", value="unresolved"))
            if surveyed.resolution_note:
                # WHY it is unresolved, when the reason is not simply "no
                # lockfile mentions this name" — e.g. a declared constraint that
                # admits none of the locked versions of that name. Recorded so
                # the survey never has to guess and never silently widens.
                properties.append(
                    Property(
                        name="fathomdb:resolution-note",
                        value=surveyed.resolution_note,
                    )
                )
        for origin in surveyed.origins:
            properties.append(
                Property(name="fathomdb:declared-in", value=origin.path)
            )
        # ONE PROPERTY PER DISTINCT CONSTRAINT, never a joined string. Joining
        # them made the value ambiguous exactly where it has to be machine-
        # readable: `", "` is also the AND separator inside a cargo range, so a
        # component declared at `0.20` by one manifest and `0.22` by another
        # serialized as `"0.20, 0.22"` — indistinguishable from a single
        # unsatisfiable conjunction. `AC-SBOM-24` reads this property back and
        # checks the component's own version against it, so it must round-trip
        # unambiguously. `fathomdb:declared-in` is already multi-valued the same
        # way, and CycloneDX allows repeated property names.
        for constraint in sorted({origin.constraint for origin in surveyed.origins}):
            properties.append(Property(name="fathomdb:constraint", value=constraint))
        if surveyed.lock_derived_edges:
            # §5.5's honest limitation: lock `dependencies` lists are already
            # feature-resolved and carry no normal/dev/build distinction, so the
            # edges they produce are tagged `resolved`. Only the manifest-derived
            # declarations carry a real kind, and those travel in
            # `staleness.json`'s `declared_in[].kind`.
            properties.append(Property(name="fathomdb:edge-kind", value="resolved"))

        component = Component(
            name=surveyed.name,
            version=surveyed.version,
            type=ComponentType.LIBRARY,
            purl=PackageURL.from_string(surveyed.purl),
            bom_ref=surveyed.purl,
            properties=properties,
        )
        by_purl[surveyed.purl] = component
        bom.components.add(component)
        if surveyed.depth == "direct":
            direct.append(component)

    # Every component gets a `dependencies` entry — a leaf takes an empty one.
    # Direction "no dangling refs" is trivially true of an empty array, so it is
    # this half that carries the weight (CycloneDX's own guidance, §5.5).
    bom.register_dependency(root, direct)
    for surveyed in survey.components:
        component = by_purl[surveyed.purl]
        targets = [by_purl[ref] for ref in surveyed.depends_on if ref in by_purl]
        bom.register_dependency(component, targets)

    serialized = JsonV1Dot6(bom).output_as_string(indent=2)
    if not serialized.endswith("\n"):
        serialized += "\n"
    return json.loads(serialized), serialized


def validate(doc: dict[str, Any] | str) -> str | None:
    """`None` when `doc` is CycloneDX-1.6-valid, else a diagnostic string.

    This really runs the normative schema shipped by the
    `cyclonedx-python-lib[json-validation]` extra. It is cross-checked by the
    acceptance suite against the upstream validator in both directions
    precisely because a `validate()` that certifies itself certifies nothing.
    """
    from cyclonedx.validation.json import JsonStrictValidator

    payload = doc if isinstance(doc, str) else json.dumps(doc)
    problem = JsonStrictValidator(SchemaVersion.V1_6).validate_str(payload)
    return None if problem is None else str(problem)
--- scripts/sbom-survey/sbom_survey/discovery.py
"""Manifest discovery — `git ls-files`-derived, never a filesystem walk (REQ-1, REQ-2).

Design §5.1. A filesystem walk sees `target/`, `node_modules/`, `.venv/`, `site/`
and every gitignored scratch tree — tens of thousands of vendored manifests the
project does not own — plus the gitignored `/python/` tree, whose pinned `0.1.0`
shim would be silently pulled into the SBOM on a developer machine that has it.
`git ls-files` makes all of that structurally unreachable.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable

__all__ = [
    "MANIFEST_TABLE",
    "ManifestRef",
    "discover_manifests",
    "git_ls_files",
]

#: The recognized-basename table (design §5.1). ⚠ MIRRORED 1:1 BY
#: `MANIFEST_BASENAMES` in `tests/conftest.py` AND BY THE DESIGN TABLE; the
#: three must be kept in lockstep. A name present in the design but missing here
#: is a manifest this tool silently skips.
#:
#: Two groups. The first matches tracked paths today. The second currently
#: matches nothing and exists as a FORWARD GUARD: adding one of those files to
#: the repository must make it DISCOVERED (and then fail REQ-4 tiering loudly)
#: rather than silently ignored.
MANIFEST_TABLE: dict[str, tuple[str, str]] = {
    # --- matches tracked paths today ---
    "Cargo.toml": ("cargo", "manifest"),
    "Cargo.lock": ("cargo", "lockfile"),
    "package.json": ("npm", "manifest"),
    "package-lock.json": ("npm", "lockfile"),
    "pyproject.toml": ("pypi", "manifest"),
    "uv.lock": ("pypi", "lockfile"),
    "setup.py": ("pypi", "manifest"),
    # --- recognized, forward-looking: nothing tracked matches these today ---
    "yarn.lock": ("npm", "lockfile"),
    "pnpm-lock.yaml": ("npm", "lockfile"),
    "poetry.lock": ("pypi", "lockfile"),
    "Pipfile": ("pypi", "manifest"),
    "setup.cfg": ("pypi", "manifest"),
}

#: `requirements*.txt` is a glob rather than an exact basename, so it is matched
#: separately. It is a pypi MANIFEST (no lock exists for it — §5.5).
_REQUIREMENTS_ECOSYSTEM_KIND = ("pypi", "manifest")


@dataclass(frozen=True, order=True)
class ManifestRef:
    """A tracked dependency manifest or lockfile.

    `path` is repo-relative and POSIX-separated, exactly as `git ls-files`
    reports it.
    """

    path: str
    ecosystem: str
    kind: str


def _classify_basename(path: str) -> tuple[str, str] | None:
    name = path.rsplit("/", 1)[-1]
    if name in MANIFEST_TABLE:
        return MANIFEST_TABLE[name]
    if name.startswith("requirements") and name.endswith(".txt"):
        return _REQUIREMENTS_ECOSYSTEM_KIND
    return None


def git_ls_files(repo_root: Path) -> list[str]:
    """Every path tracked at `repo_root`, straight from git.

    `-z` because repository paths may contain anything but NUL. This is the
    tool's entire git surface — no `GitPython`, no second subprocess (§5.7).
    """
    completed = subprocess.run(  # noqa: S603 - fixed argv, no shell
        ["git", "-C", str(repo_root), "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
    )
    return [p for p in completed.stdout.split("\0") if p]


def discover_manifests(
    repo_root: Path | str,
    *,
    ls_files: Callable[[Path], Iterable[str]] | None = None,
) -> list[ManifestRef]:
    """The tracked manifests/lockfiles at `repo_root`, sorted by path.

    The candidate set comes from `ls_files` — `git_ls_files` by default,
    injectable for tests — and from nowhere else. There is no `os.walk`, no
    `glob` and no `Path.rglob` in this package's discovery path, so a manifest
    that exists on disk but is not tracked can never reach the survey.
    """
    root = Path(repo_root)
    runner = ls_files if ls_files is not None else git_ls_files

    refs: list[ManifestRef] = []
    for path in runner(root):
        classified = _classify_basename(path)
        if classified is None:
            continue
        ecosystem, kind = classified
        refs.append(ManifestRef(path=path, ecosystem=ecosystem, kind=kind))
    return sorted(refs, key=lambda ref: ref.path)
--- scripts/sbom-survey/sbom_survey/parse/__init__.py
"""Lockfile + manifest parsers (design §5.5).

Edges come from LOCKFILES; constraints and direct-ness come from MANIFESTS.

Why parse the tracked lockfile rather than shell out to `cargo metadata` /
`npm ls`, in order of weight:

1. **Hermeticity.** A subprocess needs the toolchain, a populated registry cache
   and frequently the network, which would make the acceptance suite
   non-hermetic and violate REQ-9 at its root.
2. **Fidelity to the question.** "What version are we *using*?" is answered by
   the tracked lockfile by definition. A subprocess can resolve differently from
   the committed lock and would then report something the repository does not
   contain.
3. **Cost.** A cold `cargo metadata` on this workspace is minutes; the whole
   point of the re-scope is a fast mechanical run.
"""

from __future__ import annotations

from dataclasses import dataclass, field

__all__ = ["Declaration", "LockPackage", "ManifestParseError"]


class ManifestParseError(Exception):
    """A tracked manifest or lockfile could not be parsed. CLI exit 3 (§5.9)."""

    def __init__(self, path: str, cause: BaseException) -> None:
        self.path = path
        self.cause = cause
        super().__init__(f"could not parse {path}: {cause!r}")


@dataclass(frozen=True)
class Declaration:
    """A dependency DECLARED by a tracked manifest.

    This is what makes a package `direct` (§5.5) and what supplies both the
    manifest constraint and the `edit_sites` a bump would have to touch.
    """

    ecosystem: str
    name: str
    constraint: str
    kind: str
    manifest_path: str


@dataclass
class LockPackage:
    """A package RESOLVED by a tracked lockfile, with its resolved edges."""

    ecosystem: str
    name: str
    version: str
    #: Opaque, per-lockfile identity — the key edges are resolved against.
    key: str
    #: Keys of the packages this one depends on, within the same lockfile.
    depends_on: list[str] = field(default_factory=list)
--- scripts/sbom-survey/sbom_survey/parse/cargo.py
"""`Cargo.lock` and `Cargo.toml` parsing (design §5.5)."""

from __future__ import annotations

import tomllib
from typing import Any, Mapping

from . import Declaration, LockPackage, ManifestParseError

__all__ = ["parse_lock", "parse_manifest", "workspace_dependencies"]

_DEPENDENCY_TABLES = (
    ("dependencies", "normal"),
    ("dev-dependencies", "dev"),
    ("build-dependencies", "build"),
)


def _load(path: str, text: str) -> dict[str, Any]:
    try:
        return tomllib.loads(text)
    except Exception as exc:  # noqa: BLE001 - any parse failure is exit 3
        raise ManifestParseError(path, exc) from exc


def parse_lock(path: str, text: str) -> list[LockPackage]:
    """Every `[[package]]` in a `Cargo.lock`, with its resolved edges.

    A lock `dependencies` entry is either `"name"` or `"name version"` — cargo
    only writes the version when the name is ambiguous — so edges are resolved
    against a name→versions index built from the same file.
    """
    data = _load(path, text)
    entries = data.get("package", [])

    by_name: dict[str, list[str]] = {}
    for entry in entries:
        name = entry.get("name")
        version = entry.get("version")
        if isinstance(name, str) and isinstance(version, str):
            by_name.setdefault(name, []).append(version)

    packages: list[LockPackage] = []
    for entry in entries:
        name = entry.get("name")
        version = entry.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            continue
        depends_on: list[str] = []
        for raw in entry.get("dependencies", []) or []:
            if not isinstance(raw, str):
                continue
            parts = raw.split()
            dep_name = parts[0]
            if len(parts) >= 2:
                dep_version: str | None = parts[1]
            else:
                candidates = by_name.get(dep_name, [])
                # Ambiguous and unqualified cannot happen in a well-formed lock
                # (cargo qualifies exactly then), so an unresolvable edge is
                # dropped rather than guessed — a wrong edge is worse than a
                # missing one.
                dep_version = candidates[0] if len(candidates) == 1 else None
            if dep_version is None:
                continue
            depends_on.append(f"{dep_name} {dep_version}")
        packages.append(
            LockPackage(
                ecosystem="cargo",
                name=name,
                version=version,
                key=f"{name} {version}",
                depends_on=depends_on,
            )
        )
    return packages


def workspace_dependencies(path: str, text: str) -> dict[str, str]:
    """`[workspace.dependencies]` version pins from a workspace root manifest.

    Member crates write `foo.workspace = true`, so the member is the declaring
    manifest (that is where a bump's edit site is) while the constraint string
    lives here.
    """
    data = _load(path, text)
    table = data.get("workspace", {}).get("dependencies", {})
    pins: dict[str, str] = {}
    if isinstance(table, Mapping):
        for name, spec in table.items():
            pins[name] = _constraint_of(spec, {}) or "*"
    return pins


def _constraint_of(spec: Any, workspace_pins: Mapping[str, str]) -> str | None:
    if isinstance(spec, str):
        return spec
    if not isinstance(spec, Mapping):
        return None
    if spec.get("workspace") is True:
        return None  # resolved by the caller, which knows the crate name
    version = spec.get("version")
    if isinstance(version, str):
        return version
    if "path" in spec:
        return "path"
    if "git" in spec:
        return "git"
    return "*"


def parse_manifest(
    path: str,
    text: str,
    *,
    workspace_pins: Mapping[str, str] | None = None,
) -> list[Declaration]:
    """`[dependencies]` / `[dev-dependencies]` / `[build-dependencies]` declarations.

    `[workspace.dependencies]` in a virtual workspace root is deliberately NOT a
    declaration site: the crate that writes `foo.workspace = true` is the one a
    bump has to touch, and the root's pin is folded in as that declaration's
    constraint instead.
    """
    data = _load(path, text)
    pins = dict(workspace_pins or {})

    declarations: list[Declaration] = []
    for table, kind in _DEPENDENCY_TABLES:
        entries = data.get(table, {})
        if not isinstance(entries, Mapping):
            continue
        for key, spec in entries.items():
            # `foo = { package = "real-crate", version = "1" }` renames.
            name = key
            if isinstance(spec, Mapping) and isinstance(spec.get("package"), str):
                name = spec["package"]
            constraint = _constraint_of(spec, pins)
            if constraint is None:
                constraint = pins.get(name, "workspace")
            declarations.append(
                Declaration(
                    ecosystem="cargo",
                    name=name,
                    constraint=constraint,
                    kind=kind,
                    manifest_path=path,
                )
            )
    return declarations
--- scripts/sbom-survey/sbom_survey/parse/npm.py
"""`package-lock.json` (v3) and `package.json` parsing (design §5.5)."""

from __future__ import annotations

import json
from typing import Any, Mapping

from . import Declaration, LockPackage, ManifestParseError

__all__ = ["parse_lock", "parse_manifest"]

_DEPENDENCY_TABLES = (
    ("dependencies", "normal"),
    ("devDependencies", "dev"),
    ("optionalDependencies", "optional"),
)

_NODE_MODULES = "node_modules/"


def _load(path: str, text: str) -> dict[str, Any]:
    try:
        return json.loads(text)
    except Exception as exc:  # noqa: BLE001 - any parse failure is exit 3
        raise ManifestParseError(path, exc) from exc


def _name_of(key: str, entry: Mapping[str, Any]) -> str:
    name = entry.get("name")
    if isinstance(name, str) and name:
        return name
    # `node_modules/@scope/pkg` and the nested
    # `node_modules/a/node_modules/@scope/pkg` both end in the real name.
    index = key.rfind(_NODE_MODULES)
    return key[index + len(_NODE_MODULES) :] if index >= 0 else key


def _resolve(packages: Mapping[str, Any], from_key: str, dep_name: str) -> str | None:
    """npm's own resolution: nearest `node_modules` walking up from `from_key`.

    A v3 lock can carry `node_modules/a/node_modules/b` for a conflicting
    version, so `b` must resolve to the nested entry when reached through `a`
    and to the hoisted one otherwise — otherwise the two distinct components
    would share edges they do not have.
    """
    base = from_key
    while True:
        candidate = f"{base}/{_NODE_MODULES}{dep_name}" if base else f"{_NODE_MODULES}{dep_name}"
        if candidate in packages:
            return candidate
        if not base:
            return None
        index = base.rfind(f"/{_NODE_MODULES}")
        base = base[:index] if index >= 0 else ""


def parse_lock(path: str, text: str) -> list[LockPackage]:
    """Every real entry in a `package-lock.json` v3 `packages` map."""
    data = _load(path, text)
    packages = data.get("packages")
    if not isinstance(packages, Mapping):
        return []

    resolved: list[LockPackage] = []
    for key, entry in packages.items():
        if key == "" or not isinstance(entry, Mapping):
            continue  # the root project itself is not one of its own dependencies
        if entry.get("link") is True:
            continue  # a workspace symlink, not a resolved third-party package
        version = entry.get("version")
        if not isinstance(version, str) or not version:
            continue
        name = _name_of(key, entry)

        depends_on: list[str] = []
        for table, _kind in _DEPENDENCY_TABLES:
            deps = entry.get(table)
            if not isinstance(deps, Mapping):
                continue
            for dep_name in deps:
                target = _resolve(packages, key, dep_name)
                if target is not None:
                    depends_on.append(target)

        resolved.append(
            LockPackage(
                ecosystem="npm",
                name=name,
                version=version,
                key=key,
                depends_on=depends_on,
            )
        )
    return resolved


def parse_manifest(path: str, text: str) -> list[Declaration]:
    """`dependencies` / `devDependencies` / `optionalDependencies` declarations."""
    data = _load(path, text)
    declarations: list[Declaration] = []
    for table, kind in _DEPENDENCY_TABLES:
        entries = data.get(table)
        if not isinstance(entries, Mapping):
            continue
        for name, constraint in entries.items():
            declarations.append(
                Declaration(
                    ecosystem="npm",
                    name=name,
                    constraint=constraint if isinstance(constraint, str) else "*",
                    kind=kind,
                    manifest_path=path,
                )
            )
    return declarations
--- scripts/sbom-survey/sbom_survey/parse/python.py
"""`uv.lock`, `pyproject.toml` and `requirements*.txt` parsing (design §5.5).

**No `setup.py` is ever executed or AST-parsed.** The only tracked `setup.py`
files are the four excluded pip-skew fixtures, so the single most fragile parser
in the Python packaging space is never written (§5.2). If a real `setup.py` is
ever tracked it is DISCOVERED, and it contributes no declarations rather than
being mis-parsed.
"""

from __future__ import annotations

import tomllib
from typing import Any, Mapping

from packaging.requirements import InvalidRequirement, Requirement
from packaging.utils import canonicalize_name

from . import Declaration, LockPackage, ManifestParseError

__all__ = ["parse_pyproject", "parse_requirements", "parse_uv_lock"]

#: A lock entry whose source is one of these IS the project being locked, not a
#: dependency of it — `src/python/uv.lock` locks `fathomdb` itself.
_SELF_SOURCES = ("editable", "virtual", "directory")


def _load_toml(path: str, text: str) -> dict[str, Any]:
    try:
        return tomllib.loads(text)
    except Exception as exc:  # noqa: BLE001 - any parse failure is exit 3
        raise ManifestParseError(path, exc) from exc


def _dep_names(block: Any) -> list[str]:
    names: list[str] = []
    if isinstance(block, list):
        for item in block:
            if isinstance(item, Mapping) and isinstance(item.get("name"), str):
                names.append(canonicalize_name(item["name"]))
    elif isinstance(block, Mapping):
        for group in block.values():
            names.extend(_dep_names(group))
    return names


def parse_uv_lock(path: str, text: str) -> list[LockPackage]:
    """Every `[[package]]` in a `uv.lock`, with its resolved edges."""
    data = _load_toml(path, text)
    entries = data.get("package", [])

    present = {
        canonicalize_name(entry["name"])
        for entry in entries
        if isinstance(entry, Mapping) and isinstance(entry.get("name"), str)
    }

    packages: list[LockPackage] = []
    for entry in entries:
        if not isinstance(entry, Mapping):
            continue
        name = entry.get("name")
        version = entry.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            continue
        source = entry.get("source")
        if isinstance(source, Mapping) and any(k in source for k in _SELF_SOURCES):
            continue
        key = canonicalize_name(name)
        depends_on = [
            dep
            for dep in (
                *_dep_names(entry.get("dependencies")),
                *_dep_names(entry.get("optional-dependencies")),
                *_dep_names(entry.get("dev-dependencies")),
            )
            if dep in present and dep != key
        ]
        packages.append(
            LockPackage(
                ecosystem="pypi",
                name=name,
                version=version,
                key=key,
                depends_on=depends_on,
            )
        )
    return packages


def _declaration(spec: str, kind: str, path: str) -> Declaration | None:
    try:
        requirement = Requirement(spec)
    except InvalidRequirement:
        return None
    constraint = str(requirement.specifier) or "*"
    return Declaration(
        ecosystem="pypi",
        name=requirement.name,
        constraint=constraint,
        kind=kind,
        manifest_path=path,
    )


def parse_pyproject(path: str, text: str) -> list[Declaration]:
    """`project.dependencies` + `project.optional-dependencies` (PEP 621)."""
    data = _load_toml(path, text)
    project = data.get("project")
    if not isinstance(project, Mapping):
        return []

    declarations: list[Declaration] = []
    for spec in project.get("dependencies", []) or []:
        if isinstance(spec, str):
            declaration = _declaration(spec, "normal", path)
            if declaration is not None:
                declarations.append(declaration)

    optional = project.get("optional-dependencies")
    if isinstance(optional, Mapping):
        for group, specs in optional.items():
            kind = "dev" if group in ("dev", "test", "lint", "typecheck") else "optional"
            for spec in specs or []:
                if isinstance(spec, str):
                    declaration = _declaration(spec, kind, path)
                    if declaration is not None:
                        declarations.append(declaration)
    return declarations


def parse_requirements(path: str, text: str) -> list[tuple[Declaration, str | None]]:
    """`requirements*.txt` declarations, each with its pinned version if any.

    No lock exists for a `requirements.txt` (§5.5), so an `==` pin IS the locked
    version and anything looser resolves to nothing — which becomes
    `locked_version=None` and therefore `status="unknown"`, never `current`.
    """
    parsed: list[tuple[Declaration, str | None]] = []
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line or line.startswith("-"):
            continue
        declaration = _declaration(line, "normal", path)
        if declaration is None:
            continue
        pinned: str | None = None
        try:
            requirement = Requirement(line)
        except InvalidRequirement:  # pragma: no cover - _declaration already returned
            requirement = None
        if requirement is not None:
            exact = [s for s in requirement.specifier if s.operator == "=="]
            if len(exact) == 1 and "*" not in exact[0].version:
                pinned = exact[0].version
        parsed.append((declaration, pinned))
    return parsed
--- scripts/sbom-survey/sbom_survey/paths.py
"""Well-known repo-relative paths and the fixed default timestamp (design §5.6, §5.8)."""

from __future__ import annotations

from pathlib import Path

__all__ = [
    "DEFAULT_EPOCH_TIMESTAMP",
    "DEFAULT_REPORT_DIR",
    "SLICE_33_FINDINGS_DOC",
    "TIERS_RELPATH",
    "tiers_file_for",
]

#: Where the tracked tier/exclusion data lives, RELATIVE TO THE SURVEYED
#: REPOSITORY (§5.3). Rules are DATA, never code.
TIERS_RELPATH = "scripts/sbom-survey/tiers.toml"


def tiers_file_for(repo_root: Path | str) -> Path:
    """The tier rules for `repo_root`.

    RESOLVED AGAINST THE SURVEYED REPOSITORY, NOT AGAINST THE INSTALLED PACKAGE.
    This used to be `Path(__file__).parent.parent / "tiers.toml"`, which is the
    source tree when the tool is run from a checkout and
    `site-packages/sbom_survey/` after a non-editable `pip install` — where the
    file does not exist, because `pyproject.toml` declares no package data. The
    CLI then died with a bare `FileNotFoundError` on the very first command of
    the TC-111 install-then-run flow Slice 33 has to use (codex §9 round 2
    `[P1]`).

    Deriving from `repo_root` is the correct fix rather than merely a working
    one, for three reasons:

    1. **The rules are data ABOUT a repository, not about this tool.** Every
       rule is a path prefix into the surveyed tree — `src/rust/crates/`,
       `dev/tools/`, `Cargo.`. A copy baked into a wheel describes whichever
       repository the wheel was built from, which is meaningless (and silently
       wrong) when surveying a different one.
    2. **Otherwise the oracle would stop grading the file the tool uses.**
       `AC-SBOM-08` and `AC-SBOM-11` load the TRACKED `tiers.toml` and compare it
       against the document `run_survey()` produced with no `tier_map`. If the
       default came from the installed package those two could diverge, and the
       criteria would be grading a file the survey never read — exactly the
       boundary class (TC-105) this slice has been fighting throughout. Resolved
       from `repo_root`, they are byte-for-byte the same file.
    3. **A packaged copy can go stale.** Editing the tracked rules without
       reinstalling would silently re-tier every component, which is the event
       REQ-4 exists to make loud.

    The cost, stated rather than hidden: surveying a repository that does not
    track this file now REQUIRES an explicit `--tiers`. That is correct, not
    unfortunate — §5.3 rules that there is deliberately NO catch-all rule, so
    inventing a default rule set for an unknown repository would be precisely
    the silent mis-tag REQ-4 forbids.
    """
    return Path(repo_root) / TIERS_RELPATH

#: Generated reports land here, and this directory is GITIGNORED (REQ-11).
#: Repo-relative on purpose: it is compared against `git check-ignore`.
DEFAULT_REPORT_DIR = "scripts/sbom-survey/out"

#: Slice 33's findings — the TRACKED durable home, deliberately NOT ignored.
#: `dev/plans/runs/` is the house convention for a dated run report, weighed
#: against `dev/design/` and `dev/deps/` in design §5.6.
SLICE_33_FINDINGS_DOC = "dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md"

#: The FIXED default `metadata.timestamp` (§5.8, REQ-13).
#:
#: It is deliberately NOT wall-clock. A wall-clock default makes every re-run of
#: a *recurring* tool diff against the previous one, which destroys the only
#: property that makes re-running it useful. The only overrides are an explicit
#: `now` / `--now` and `SOURCE_DATE_EPOCH`; there is no code path anywhere in
#: this package that calls `datetime.now()` to produce an artifact timestamp.
DEFAULT_EPOCH_TIMESTAMP = "1980-01-01T00:00:00+00:00"
--- scripts/sbom-survey/sbom_survey/registry.py
"""The published-version seam — injectable, and honest when offline (REQ-9, REQ-10).

Design §5.4. `run_survey` takes `published` as a REQUIRED keyword-only argument
with no default, so there is no code path that reaches the network implicitly:
an online run is an explicit act of construction by the caller.
"""

from __future__ import annotations

import json
import urllib.parse
import urllib.request
from typing import Mapping, Protocol, runtime_checkable

__all__ = [
    "HttpRegistrySource",
    "OfflineSource",
    "PublishedVersionSource",
    "StaticSource",
    "source_kind",
]


@runtime_checkable
class PublishedVersionSource(Protocol):
    """`latest(ecosystem, name)` -> the published version, or None if unknown."""

    def latest(self, ecosystem: str, name: str) -> str | None: ...


class OfflineSource:
    """Always `None`. The default posture, `--offline`, and the whole test suite.

    `None` becomes `status="unknown"` — never `current`. Reporting an unchecked
    package as up-to-date is the single worst output this tool can produce: it
    would let a live advisory be closed as `CLOSE-satisfied` in the
    LIBRARY-BUMP-STEWARD §2 triage.
    """

    source_kind = "offline"

    def latest(self, ecosystem: str, name: str) -> str | None:
        return None


class StaticSource:
    """A fixed `{(ecosystem, name): version}` mapping; `None` for anything absent."""

    source_kind = "static"

    def __init__(self, mapping: Mapping[tuple[str, str], str]) -> None:
        self._mapping = dict(mapping)

    def latest(self, ecosystem: str, name: str) -> str | None:
        return self._mapping.get((ecosystem, name))


class HttpRegistrySource:
    """crates.io / npm registry / PyPI JSON, via stdlib `urllib.request`.

    Used by Slice 33's online run ONLY — never by the acceptance suite, which is
    hermetic by construction. Any exception degrades to `None`, which the survey
    turns into `status="unknown"` with the reason recorded; it never degrades to
    `current`.

    No HTTP client library is taken for three GET-JSON endpoints (§5.7): adding
    an HTTP stack to a dependency-hygiene tool is exactly the bloat this tool
    exists to expose.
    """

    source_kind = "http"

    _ENDPOINTS = {
        "cargo": "https://crates.io/api/v1/crates/{name}",
        "npm": "https://registry.npmjs.org/{name}",
        "pypi": "https://pypi.org/pypi/{name}/json",
    }

    def __init__(self, timeout: float = 10.0, user_agent: str = "fathomdb-sbom-survey") -> None:
        self.timeout = timeout
        self.user_agent = user_agent

    def latest(self, ecosystem: str, name: str) -> str | None:
        template = self._ENDPOINTS.get(ecosystem)
        if template is None:
            return None
        url = template.format(name=urllib.parse.quote(name, safe="@/"))
        request = urllib.request.Request(url, headers={"User-Agent": self.user_agent})
        with urllib.request.urlopen(request, timeout=self.timeout) as response:  # noqa: S310
            payload = json.load(response)
        if ecosystem == "cargo":
            return payload["crate"]["max_stable_version"]
        if ecosystem == "npm":
            return payload["dist-tags"]["latest"]
        return payload["info"]["version"]


def source_kind(published: object) -> str:
    """The `source` string recorded in `staleness.json` (§5.8).

    Read off the source object rather than guessed, so an `--offline` run is
    recorded as `offline` and an online one as `http`. Slice 33 uses it to tell
    a checked run from an unchecked one.
    """
    kind = getattr(published, "source_kind", None)
    return kind if isinstance(kind, str) else "custom"
--- scripts/sbom-survey/sbom_survey/report.py
"""Artifact writers — `sbom.cdx.json`, `staleness.json`, `staleness.md` (design §5.6, §5.8).

These files, not the in-process objects, are what Slice 33 opens, so the
consumer contract is fixed HERE as well: `staleness.json`'s envelope is exactly
`{generated, source, summary, rows}`, every row carries the Slice-33 field set,
and the rows are in the ruled `(tier, ecosystem, name, locked_version)` order.

Everything is written deterministically — sorted keys, a trailing newline, no
wall-clock stamp — so a recurring re-run diffs to nothing when nothing changed
(REQ-13).
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # pragma: no cover - typing only
    from .survey import Survey

__all__ = ["ARTIFACTS", "staleness_document", "staleness_markdown", "write_reports"]

ARTIFACTS = ("sbom.cdx.json", "staleness.json", "staleness.md")


def staleness_document(survey: Survey) -> dict:
    """The `staleness.json` envelope, exactly `{generated, source, summary, rows}`."""
    return {
        "generated": survey.timestamp,
        "source": survey.source,
        "summary": survey.summary(),
        "rows": [row.as_dict() for row in survey.staleness()],
    }


def staleness_markdown(survey: Survey) -> str:
    """A paste-able fragment for Slice 33's findings doc.

    The UNKNOWN count is in the header on purpose: an offline run must not be
    mistakable for a clean run at a glance (§5.4).
    """
    rows = survey.staleness()
    summary = survey.summary()
    lines = [
        "# Dependency staleness — `sbom-survey`",
        "",
        f"**Generated:** {survey.timestamp} · **Source:** {survey.source} ·"
        f" **Components:** {summary['components']}",
        "",
        f"**current:** {summary['current']} · **outdated:** {summary['outdated']} ·"
        f" **ahead:** {summary['ahead']} · **unknown:** {summary['unknown']}"
        f" of {summary['components']}",
        "",
    ]
    if survey.source == "offline":
        lines += [
            "> This was an **offline** run: no registry was consulted, so every row"
            " is `unknown`. An unknown latest is never reported as `current`.",
            "",
        ]
    lines += [
        "| ecosystem | name | tier | depth | locked | latest | status | edit sites |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for row in rows:
        sites = ", ".join(f"`{site}`" for site in row.edit_sites) or "—"
        lines.append(
            f"| {row.ecosystem} | `{row.name}` | {row.tier} | {row.depth} |"
            f" {row.locked_version or '—'} | {row.latest_version or '—'} |"
            f" {row.status} | {sites} |"
        )
    lines += [
        "",
        "## Excluded manifests",
        "",
    ]
    if survey.excluded:
        for excluded in survey.excluded:
            lines.append(f"- `{excluded.path}` — {excluded.reason}")
    else:
        lines.append("- none")
    lines.append("")
    return "\n".join(lines)


def _write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", newline="\n")


def write_reports(survey: Survey, out_dir: Path | str) -> list[Path]:
    """Write all three artifacts into `out_dir`, creating it if needed."""
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)

    sbom_path = out / "sbom.cdx.json"
    _write(sbom_path, survey.to_cyclonedx_json())

    staleness_path = out / "staleness.json"
    _write(
        staleness_path,
        json.dumps(staleness_document(survey), indent=2, sort_keys=True) + "\n",
    )

    markdown_path = out / "staleness.md"
    _write(markdown_path, staleness_markdown(survey))

    return [sbom_path, staleness_path, markdown_path]
--- scripts/sbom-survey/sbom_survey/survey.py
"""The survey itself — `run_survey`, `Survey`, `classify_status` (design §5.4, §5.5, §5.8).

This module is the INTEGRATION BOUNDARY. Everything the requirements demand has
to be true *here*, not merely inside the helper this is supposed to call:

* the candidate manifests come from `discovery.discover_manifests()`, i.e. from
  `git ls-files`. There is no `os.walk`, no `glob`, no `rglob` and no literal
  `python/…` path anywhere in this package;
* exclusion and tiering come from the injected/loaded `TierMap` and from
  nothing else — there is no `startswith("dev/release/fixtures/")` in code;
* `published.latest()` is called once for EVERY surveyed component, so the
  used-versus-published diff is really produced rather than defaulted to
  `unknown`;
* the tier stamped on a component is the tier the rules assign to its declaring
  manifest.
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable, Mapping

from packaging.utils import canonicalize_name

from . import TIER_VOCABULARY
from .constraints import matches as constraint_matches
from .cyclonedx import build_document
from .discovery import ManifestRef, discover_manifests
from .paths import DEFAULT_EPOCH_TIMESTAMP, tiers_file_for
from .parse import Declaration, LockPackage, ManifestParseError
from .parse import cargo as cargo_parse
from .parse import npm as npm_parse
from .parse import python as python_parse
from .registry import source_kind
from .tiers import TierMap, load_tier_map
from .util import TimestampFormatError, make_purl, normalize_timestamp

__all__ = [
    "ExcludedManifest",
    "Origin",
    "StalenessRow",
    "Survey",
    "SurveyComponent",
    "TimestampFormatError",
    "classify_status",
    "resolve_timestamp",
    "run_survey",
]

_TIER_RANK = {tier: rank for rank, tier in enumerate(TIER_VOCABULARY)}


# --------------------------------------------------------------------------- #
# version comparison (§5.4)
# --------------------------------------------------------------------------- #
def _parse_version(ecosystem: str, raw: str):
    """Parse with the comparator the ecosystem actually uses.

    Mixing the two is deliberate: PEP 440 rejects `1.2.3-rc.1` and semver
    rejects `1.2.3.post1`, and silently coercing either would produce wrong
    orderings — which lands back in the false-`current` failure mode this tool
    exists to avoid. Unparseable input yields `None` and therefore `unknown`.
    """
    try:
        if ecosystem == "pypi":
            from packaging.version import Version

            return Version(raw)
        import semver

        return semver.Version.parse(raw)
    except Exception:  # noqa: BLE001 - any parse failure means "we do not know"
        return None


def classify_status(ecosystem: str, locked: str | None, latest: str | None) -> str:
    """`outdated` / `current` / `ahead` / `unknown` — and `current` only honestly.

    `current` is reachable ONLY from a successfully parsed pair on both sides.
    There is no fallback, no default-to-current and no "assume current if we
    could not check": a false up-to-date would let a live advisory be closed as
    `CLOSE-satisfied` in LIBRARY-BUMP-STEWARD §2 triage.
    """
    if locked is None or latest is None:
        return "unknown"
    left = _parse_version(ecosystem, locked)
    right = _parse_version(ecosystem, latest)
    if left is None or right is None:
        return "unknown"
    if left == right:
        return "current"
    return "outdated" if left < right else "ahead"


# --------------------------------------------------------------------------- #
# the survey model
# --------------------------------------------------------------------------- #
@dataclass(frozen=True)
class ExcludedManifest:
    """A tracked manifest deliberately kept out of the BOM, with its reason.

    Carried in the survey output (and mirrored into the CycloneDX
    `metadata.properties`) so the exclusion is AUDITABLE rather than invisible.
    """

    path: str
    reason: str
    note: str | None = None


@dataclass(frozen=True)
class Origin:
    """One tracked manifest that DECLARES a component, with the declared range."""

    path: str
    constraint: str
    kind: str

    def as_dict(self) -> dict[str, str]:
        return {"path": self.path, "constraint": self.constraint, "kind": self.kind}


@dataclass
class SurveyComponent:
    ecosystem: str
    name: str
    version: str | None
    purl: str
    tier: str
    depth: str
    origins: list[Origin] = field(default_factory=list)
    depends_on: list[str] = field(default_factory=list)
    lock_derived_edges: bool = False
    #: Present only on an unresolved component whose declaring constraint could
    #: not be matched to any locked version. Surfaced in the BOM and in the
    #: staleness row's `lookup_error`, so "why is this unknown" is answerable
    #: without re-deriving anything (REQ-14).
    resolution_note: str | None = None

    @property
    def edit_sites(self) -> list[str]:
        """The exact manifest paths a bump would have to touch (§5.8).

        This is Slice 33's input to "would a surgical ~1-5 SLOC change land it";
        the tool states no verdict, it supplies the sites.
        """
        return sorted({origin.path for origin in self.origins})


@dataclass(frozen=True)
class StalenessRow:
    """One row of the Slice-33 consumer contract (§5.8, REQ-14)."""

    ecosystem: str
    name: str
    tier: str
    depth: str
    locked_version: str | None
    latest_version: str | None
    status: str
    lookup_error: str | None
    declared_in: list[dict[str, str]]
    edit_sites: list[str]
    edit_site_count: int

    def as_dict(self) -> dict[str, Any]:
        return {
            "ecosystem": self.ecosystem,
            "name": self.name,
            "tier": self.tier,
            "depth": self.depth,
            "locked_version": self.locked_version,
            "latest_version": self.latest_version,
            "status": self.status,
            "lookup_error": self.lookup_error,
            "declared_in": [dict(entry) for entry in self.declared_in],
            "edit_sites": list(self.edit_sites),
            "edit_site_count": self.edit_site_count,
        }

    @property
    def sort_key(self) -> tuple[str, str, str, str]:
        return (self.tier, self.ecosystem, self.name, self.locked_version or "")


@dataclass
class Survey:
    """The result of one run: components, exclusions and staleness rows."""

    repo_root: Path
    timestamp: str
    source: str
    components: list[SurveyComponent]
    excluded: list[ExcludedManifest]
    manifests: list[ManifestRef]
    rows: list[StalenessRow]
    _document: tuple[dict[str, Any], str] | None = field(
        default=None, repr=False, compare=False
    )

    def staleness(self) -> list[StalenessRow]:
        """The staleness rows, in the ruled `(tier, ecosystem, name, locked)` order."""
        return list(self.rows)

    def summary(self) -> dict[str, int]:
        counts = {"components": len(self.rows)}
        for status in ("current", "outdated", "ahead", "unknown"):
            counts[status] = sum(1 for row in self.rows if row.status == status)
        counts["direct"] = sum(1 for row in self.rows if row.depth == "direct")
        counts["transitive"] = sum(1 for row in self.rows if row.depth == "transitive")
        counts["excluded_manifests"] = len(self.excluded)
        return counts

    def _built(self) -> tuple[dict[str, Any], str]:
        if self._document is None:
            self._document = build_document(self)
        return self._document
--- scripts/sbom-survey/sbom_survey/tiers.py
"""Tier assignment from the tracked `tiers.toml` data file (REQ-3, REQ-4, REQ-5).

Design §5.2 and §5.3. Three properties this module exists to guarantee:

1. **Rules are DATA.** There is no path literal anywhere in this package that
   special-cases a subtree; `dev/release/fixtures/` reaches the code only as a
   `prefix` read out of `tiers.toml` (REQ-5).
2. **Matching is longest-prefix-wins**, so rule ORDER in the file is
   irrelevant and moving a block can never silently re-tier a manifest (§5.3).
3. **There is deliberately NO catch-all rule.** A tracked manifest matched by no
   rule raises `UntieredManifestError` naming the path. A default would convert
   "somebody added a manifest and nobody classified it" — the exact event this
   tool exists to catch — into a silent mis-tag (REQ-4).
"""

from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

from . import TIER_VOCABULARY
from .paths import TIERS_RELPATH

__all__ = [
    "DuplicateTierPrefixError",
    "TierMap",
    "TierRule",
    "TierRuleFileError",
    "TierRuleFileNotFoundError",
    "TierVerdict",
    "UntieredManifestError",
    "load_tier_map",
]

_ACTIONS = ("tier", "exclude")


class UntieredManifestError(Exception):
    """A tracked manifest matched no rule. Always names the offending path."""

    def __init__(self, path: str) -> None:
        self.path = path
        super().__init__(
            f"untiered manifest: {path!r} matches no rule in the tier map."
            " Every tracked manifest must carry exactly one tier from"
            f" {list(TIER_VOCABULARY)} (or an explicit exclusion rule) — add a"
            " rule to scripts/sbom-survey/tiers.toml. There is deliberately no"
            " catch-all: a default would silently mis-tag a manifest nobody"
            " classified."
        )


class TierRuleFileError(ValueError):
    """A `tiers.toml` that cannot be trusted — rejected at LOAD time."""


class TierRuleFileNotFoundError(TierRuleFileError):
    """The tier rule file could not be read.

    A tool whose entire purpose is to fail loudly on an unclassified manifest
    must not itself die with an unhandled stdlib `FileNotFoundError`, so this
    names the file it wanted, says where that path comes from, and says how to
    override it (codex §9 round 2 `[P1]`).
    """

    def __init__(self, path: str, cause: OSError) -> None:
        self.path = path
        self.cause = cause
        super().__init__(
            f"tier rules not readable: {path}\n"
            f"  ({type(cause).__name__}: {cause})\n"
            "  The tier/exclusion rules are TRACKED DATA ABOUT THE SURVEYED"
            " REPOSITORY, so they are read from"
            f" <repo>/{TIERS_RELPATH} — never from the installed package,"
            " which would describe whatever repository it was built from.\n"
            "  Fix: survey a repository that tracks that file, or pass an"
            " explicit `--tiers FILE`. There is deliberately no built-in"
            " default rule set: guessing tiers for an unknown repository is"
            " exactly the silent mis-tag this tool exists to prevent."
        )


class DuplicateTierPrefixError(TierRuleFileError):
    """The same `prefix` appears twice in a rule file (§5.3).

    Deliberately NOT an `UntieredManifestError`: that error means "no rule
    matched this path", and no path has been classified yet when a rule file is
    rejected. Resolving a duplicate silently (first-wins or last-wins) is the
    same ambiguity longest-prefix matching exists to remove, by another route.
    """

    def __init__(self, prefix: str, source: str) -> None:
        self.prefix = prefix
        super().__init__(
            f"duplicate tier rule prefix {prefix!r} in {source}: the same prefix"
            " appears more than once, so which rule wins would depend on file"
            " order. Remove or merge one of them."
        )


@dataclass(frozen=True)
class TierRule:
    """One rule from `tiers.toml`.

    `action = "tier"` carries a `tier`; `action = "exclude"` carries a `reason`.
    """

    prefix: str
    action: str
    tier: str | None = None
    reason: str | None = None
    note: str | None = None


@dataclass(frozen=True)
class TierVerdict:
    """`TierMap.classify()`'s answer for one path."""

    path: str
    action: str
    tier: str | None
    reason: str | None
    rule: TierRule


class TierMap:
    """An ordered-irrelevant, longest-prefix-wins set of tier rules."""

    def __init__(self, rules: Iterable[TierRule]) -> None:
        self.rules: list[TierRule] = list(rules)

    def __repr__(self) -> str:  # pragma: no cover - diagnostic only
        return f"TierMap({len(self.rules)} rules)"

    def classify(self, path: str) -> TierVerdict:
        """The verdict for `path`, or `UntieredManifestError` naming it.

        LONGEST-PREFIX-WINS: among every rule whose prefix the path starts
        with, the longest prefix is selected. Two distinct prefixes of equal
        length cannot both prefix the same string, so the winner is unique and
        the answer does not depend on the order of `self.rules` — which is what
        makes reordering `tiers.toml` provably safe (§5.3, AC-SBOM-23).
        """
        matches = [rule for rule in self.rules if path.startswith(rule.prefix)]
        if not matches:
            raise UntieredManifestError(path)
        best = max(matches, key=lambda rule: len(rule.prefix))
        return TierVerdict(
            path=path,
            action=best.action,
            tier=best.tier,
            reason=best.reason,
            rule=best,
        )


def load_tier_map(path: Path | str) -> TierMap:
    """Load and VALIDATE a `tiers.toml`.

    Everything that could make a rule set ambiguous is rejected here, at load
    time, before any path is classified: an unknown `action`, a tier outside the
    ruled vocabulary, an exclusion with no reason, and — §5.3 — a duplicated
    prefix, whose error message names the offending prefix.
    """
    source = str(path)
    try:
        with open(path, "rb") as handle:
            data = tomllib.load(handle)
    except OSError as exc:
        raise TierRuleFileNotFoundError(source, exc) from exc
    except tomllib.TOMLDecodeError as exc:
        raise TierRuleFileError(f"{source}: not valid TOML — {exc}") from exc

    schema = data.get("schema")
    if schema != 1:
        raise TierRuleFileError(
            f"{source}: unsupported tier-rule schema {schema!r}; expected 1"
        )

    raw_rules = data.get("rule")
    if not isinstance(raw_rules, list) or not raw_rules:
        raise TierRuleFileError(f"{source}: no [[rule]] entries")

    rules: list[TierRule] = []
    seen: set[str] = set()
    for entry in raw_rules:
        prefix = entry.get("prefix")
        if not isinstance(prefix, str) or not prefix:
            raise TierRuleFileError(f"{source}: a rule has no `prefix`")
        if prefix in seen:
            raise DuplicateTierPrefixError(prefix, source)
        seen.add(prefix)

        action = entry.get("action")
        if action not in _ACTIONS:
            raise TierRuleFileError(
                f"{source}: rule {prefix!r} has action {action!r};"
                f" expected one of {list(_ACTIONS)}"
            )

        tier = entry.get("tier")
        reason = entry.get("reason")
        if action == "tier":
            if tier not in TIER_VOCABULARY:
                raise TierRuleFileError(
                    f"{source}: rule {prefix!r} assigns tier {tier!r}, outside"
                    f" the ruled vocabulary {list(TIER_VOCABULARY)}"
                )
        else:
            if not isinstance(reason, str) or not reason:
                raise TierRuleFileError(
                    f"{source}: exclusion rule {prefix!r} carries no `reason`;"
                    " an exclusion must be auditable, never silent"
                )
            if tier is not None:
                raise TierRuleFileError(
                    f"{source}: exclusion rule {prefix!r} also assigns a tier"
                )
--- scripts/sbom-survey/sbom_survey/util.py
"""Shared primitives: component identity (§5.5) and the ONE timestamp door (§5.8)."""

from __future__ import annotations

from datetime import datetime, timezone

from packageurl import PackageURL

__all__ = [
    "TimestampFormatError",
    "make_purl",
    "normalize_timestamp",
    "parse_timestamp",
]


class TimestampFormatError(ValueError):
    """A timestamp input that is not ISO 8601. Never silently substituted."""

    #: What a well-formed value looks like. Parameterized because the two inputs
    #: that reach this error have DIFFERENT formats — `--now` is ISO 8601 and
    #: `SOURCE_DATE_EPOCH` is an integer second count — and an error message that
    #: names the wrong format is worse than terse.
    ISO_8601 = "an ISO 8601 timestamp, e.g. 1970-01-01T00:00:00Z or 2026-07-29T12:00:00+00:00"
    UNIX_SECONDS = "an integer number of seconds since the Unix epoch, e.g. 1700000000"

    def __init__(
        self,
        value: object,
        source: str,
        cause: BaseException | None = None,
        expected: str | None = None,
    ) -> None:
        self.value = value
        self.source = source
        detail = f" ({type(cause).__name__}: {cause})" if cause is not None else ""
        super().__init__(
            f"{source}: {value!r} is not a usable timestamp{detail}.\n"
            f"  Expected {expected or self.ISO_8601}.\n"
            "  It is REJECTED rather than substituted: this value is the"
            " provenance of the whole run, and design §5.6 makes the findings"
            " doc's provenance header load-bearing — 'without this the numbers"
            " are unfalsifiable'. Quietly falling back to a default would make a"
            " typo produce a plausible-looking artifact stamped with a time the"
            " run did not happen at."
        )


def parse_timestamp(value: str, *, source: str) -> datetime:
    """THE ONLY PLACE A TIMESTAMP STRING IS INTERPRETED.

    Every artifact's stamp flows through here, so `staleness.json`'s `generated`
    and the CycloneDX `metadata.timestamp` cannot disagree about when a run
    happened. They used to: a malformed `--now` was passed through verbatim into
    `survey.timestamp` (and so into `staleness.json`), while the CycloneDX
    builder failed to parse the same string and SILENTLY fell back to the fixed
    1980 epoch — one run, two artifacts, two different answers (codex §9 round 3
    `[P3]`).

    There is deliberately NO fallback here. A caller that cannot supply a
    parseable timestamp gets an error naming the value and the input it came
    from; it does not get a substitute.
    """
    if not isinstance(value, str):
        raise TimestampFormatError(value, source)
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise TimestampFormatError(value, source, exc) from exc
    if parsed.tzinfo is None:
        # Naive input is read as UTC rather than as local time: local time would
        # make the artifacts depend on the operating machine, which REQ-13
        # forbids.
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed


def normalize_timestamp(value: str, *, source: str) -> str:
    """`parse_timestamp` rendered back to a canonical ISO 8601 string."""
    return parse_timestamp(value, source=source).isoformat()

_PURL_TYPE = {"cargo": "cargo", "npm": "npm", "pypi": "pypi"}


def make_purl(ecosystem: str, name: str, version: str | None) -> str:
    """`pkg:<type>/<name>[@<version>]`, unique per (ecosystem, name, version).

    Built with `packageurl-python` rather than by string concatenation: purl has
    non-obvious encoding rules (namespaces, qualifiers, percent-encoding) — an
    npm scope becomes a purl NAMESPACE and its `@` is percent-encoded, so
    `@napi-rs/cli` is `pkg:npm/%40napi-rs/cli@2.18.4` and the version segment
    stays unambiguous.

    A component with no locked version carries a version-less purl. It is still
    a stable identity, and the component is separately marked
    `fathomdb:resolution = "unresolved"` so no consumer mistakes it for a
    resolved package at an unknown version.
    """
    purl_type = _PURL_TYPE.get(ecosystem)
    if purl_type is None:  # pragma: no cover - ecosystems are closed at discovery
        raise ValueError(f"unknown ecosystem {ecosystem!r}")

    namespace: str | None = None
    base = name
    if purl_type == "npm" and name.startswith("@") and "/" in name:
        namespace, _, base = name.partition("/")

    return PackageURL(
        type=purl_type,
        namespace=namespace,
        name=base,
        version=version or None,
    ).to_string()
--- scripts/sbom-survey/smoke-install-run.sh
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
#   B. PROVENANCE (RUN A) — under a scrubbed import environment, the installed
#      `sbom_survey` resolves inside the venv's site-packages and NOT under the
#      repo. An ambient `PYTHONPATH` would otherwise make the "installed" run
#      import the source tree, passing while the wheel is broken;
#   C. RUN A — `$VENV/bin/sbom-survey` (the real entry point) exits 0;
#   D. PROVENANCE (RUN B) — after uninstalling, `import sbom_survey` resolves
#      under the repo source tree, so RUN B genuinely exercises the tree and the
#      identity check below cannot be vacuously true against the still-installed
#      copy (TC-105: Slice 31's dominant defect class was a criterion graded
#      against a helper while the real boundary went ungraded);
#   E. RUN B — `python -m sbom_survey` from the source tree exits 0;
#   F. the artifact SETS are identical (an extra/missing file is caught too);
#   G. all three artifacts are byte-identical between the two runs;
#   H. VACUITY GUARD — two empty files are byte-identical, so the run is only
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
# CWD-INDEPENDENT. Both runs and every provenance probe execute (in subshells)
# with cwd set to the throwaway work dir, because cwd lands on `sys.path` for
# `python -c` and `python -m`. Every path handed to them is absolute, so this
# changes nothing about what is surveyed — only that it cannot matter where you
# invoked the smoke from. See §6a.
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

# --- 6a. PROVENANCE ASSERTION FOR RUN A — symmetric with RUN B's (codex §9 rd 2)
#
# RUN A inherits the caller's environment, and `PYTHONPATH` beats site-packages
# on `sys.path`. So an ambient `PYTHONPATH` pointing at THIS checkout (or any
# checkout carrying `sbom_survey`) makes the installed console script import the
# SOURCE TREE — and the smoke then passes while the installed wheel is broken,
# incomplete, or missing files entirely. That is a vacuous pass on the exact leg
# this script exists to prove.
#
# Two things are needed, and only the second is a guard:
#   * RUN A is invoked under `env -u PYTHONPATH -u PYTHONHOME` — scrubbed
#     PER-INVOCATION, never globally, because RUN B *needs* `PYTHONPATH`;
#   * and that arrangement is ASSERTED here. Unsetting only ARRANGES for the
#     right thing; the assertion PROVES it. RUN B's provenance was graded from
#     the start and RUN A's was not — that asymmetry was the finding.
#
# THE PROBE MUST REPLICATE RUN A'S IMPORT ENVIRONMENT, NOT APPROXIMATE IT
# (codex §9 round 3). `python -c` puts the CURRENT DIRECTORY on `sys.path[0]`
# (it is `''`), whereas a script executed BY PATH — which is what
# `$VENV/bin/sbom-survey` is — gets the SCRIPT's directory (`venv/bin`) there
# and never cwd. An earlier probe therefore graded a stricter, different import
# environment than the one it certified: run from `scripts/sbom-survey`, it
# reported a source-tree import that RUN A would never have performed, and the
# smoke failed before RUN A ever executed. A guard that cries wolf gets ignored,
# which costs exactly the protection `seq-172` left this script as the only
# source of.
#
# The fix is a NEUTRAL cwd: every probe below, and both runs, execute with cwd
# set to `$WORK`, which is already asserted to be outside the repo and contains
# no `sbom_survey`. `--repo`, `--out`, `$CONSOLE` and `$PROJECT` are all absolute,
# so cwd cannot change what is surveyed or where it is written — only which
# package would be importable, which is precisely what must not vary. (A neutral
# cwd is preferred over `python -P` / `PYTHONSAFEPATH=1` because it mirrors the
# console script's own situation directly and carries no interpreter-version
# caveat.) `cd` happens in SUBSHELLS so the script's own cwd never moves.
set +e
SITE_PACKAGES="$(cd "$WORK" && env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$SITE_PACKAGES" ]; then
    echo "smoke: FAIL — could not resolve the venv's site-packages (rc=$rc)." >&2
    exit 1
fi

set +e
RESOLVED_A="$(cd "$WORK" && env -u PYTHONPATH -u PYTHONHOME "$VENV/bin/python" -c 'import sbom_survey; print(sbom_survey.__file__)')"
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "smoke: FAIL — the INSTALLED sbom_survey is not importable from the venv (rc=$rc)." >&2
    echo "smoke:        pip install reported success, so the wheel does not contain" >&2
    echo "smoke:        an importable package. This is the install-path defect." >&2
    exit 1
fi
case "$RESOLVED_A" in
    "$REPO"/*)
        echo "smoke: FAIL — provenance (RUN A). The installed entry point resolves to:" >&2
        echo "smoke:        $RESOLVED_A" >&2
        echo "smoke:        which is INSIDE the repo ($REPO), not the venv's" >&2
        echo "smoke:        site-packages ($SITE_PACKAGES). RUN A would exercise the" >&2
        echo "smoke:        SOURCE TREE, so a PASS would say nothing about the wheel." >&2
        exit 1
        ;;
esac
case "$RESOLVED_A" in
    "$SITE_PACKAGES"/*)
        echo "smoke: provenance OK (RUN A) — installed sbom_survey resolves to $RESOLVED_A"
        ;;
    *)
        echo "smoke: FAIL — provenance (RUN A). The installed entry point resolves to:" >&2
        echo "smoke:        $RESOLVED_A" >&2
        echo "smoke:        expected a path under the venv's site-packages:" >&2
--- scripts/sbom-survey/tests/conftest.py
"""Shared support for the `sbom-survey` acceptance suite (0.8.20 Slice 31).

This suite is **RED by construction**: Slice 31 ships requirements, acceptance
criteria, design and tests; Slice 32 ships the code that turns them GREEN.

The one structural rule this file exists to enforce: **nothing imports
`sbom_survey` at module level.** A top-level import of a package that does not
exist yet raises at collection time, which aborts the whole module and hides
every criterion after the first. Tests call `require()` *inside the test body*
instead, so each acceptance criterion produces its own attributable FAILED with
the required behaviour restated in the message.

A `pytest.skip` would be a vacuous green and is used nowhere in this suite.
"""

from __future__ import annotations

import importlib
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

# <repo>/scripts/sbom-survey/tests/conftest.py
TESTS_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = TESTS_DIR.parent
REPO_ROOT = PROJECT_ROOT.parents[1]

# The mini-project is deliberately isolated (its own pyproject.toml, no
# workspace membership), so the suite must be runnable without installing it.
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

# Basenames the survey recognizes as dependency manifests / lockfiles.
#
# ⚠ THIS TUPLE MIRRORS THE RECOGNIZED-BASENAME TABLE IN THE DESIGN (§5.1) 1:1 AND
# MUST BE KEPT IN LOCKSTEP WITH IT. It is the oracle AC-SBOM-03 grades against:
# a name that is in the design table but missing here is a manifest the tool may
# silently skip while the suite still reports green (codex §9 round 1, fix-1
# finding 1 — `setup.cfg` was exactly that).
#
# Split in two: the first group matches tracked paths TODAY (§5.1's 29-path
# enumeration); the second group currently matches nothing and is present so
# that adding one of those files to the repo is DISCOVERED (and then fails REQ-4
# tiering loudly) rather than silently ignored. `requirements*.txt` is a glob and
# so lives in `tracked_manifest_paths()` below rather than in this tuple.
MANIFEST_BASENAMES = (
    # matched by tracked paths at cbb56212
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "package-lock.json",
    "pyproject.toml",
    "uv.lock",
    "setup.py",
    # recognized, forward-looking: nothing tracked matches these today
    "yarn.lock",
    "pnpm-lock.yaml",
    "poetry.lock",
    "Pipfile",
    "setup.cfg",
)

TIER_VOCABULARY = ("shipped", "dev-tooling", "eval-only")

FIXTURE_PREFIX = "dev/release/fixtures/"

# purl (`bom-ref`) prefix per ecosystem — the component identity an advisory feed
# matches against (REQ-6, design §5.5). Every ecosystem the repo tracks must be
# represented in the BOM, and no fourth purl type may appear.
PURL_PREFIX_BY_ECOSYSTEM = {
    "cargo": "pkg:cargo/",
    "npm": "pkg:npm/",
    "pypi": "pkg:pypi/",
}
PURL_PREFIXES = tuple(PURL_PREFIX_BY_ECOSYSTEM.values())

# Cargo crates present in the tracked `Cargo.lock` that are declared by NO
# dependency table of any tracked, non-excluded `Cargo.toml` — i.e. they exist
# ONLY because a lockfile-derived library<->library edge put them there. If none
# of these reaches the BOM tagged `transitive`, the implementation emitted the
# direct set and dropped the dependency graph (codex §9 round 1, fix-1
# finding 3). Verified lockfile-only at cbb56212; see design §5.5 for the
# drift note (a future direct adoption of one of these is a legitimate reason
# for it to leave this set, and the assertion only needs ONE survivor).
KNOWN_TRANSITIVE_ONLY_CARGO = (
    "proc-macro2",
    "quote",
    "syn",
    "unicode-ident",
    "serde_derive",
)

# The NEGATIVE CONTROL for AC-SBOM-10's independent schema oracle.
#
# CycloneDX 1.6 makes `type` REQUIRED on every component (the 1.6 component
# definition is `"required": ["type", "name"]`), so a conforming validator must
# reject this document. A validator that accepts it is not validating anything,
# and a clean result from it would prove nothing — which is the whole failure
# mode AC-SBOM-10 exists to stop.
KNOWN_INVALID_CYCLONEDX_DOC = {
    "bomFormat": "CycloneDX",
    "specVersion": "1.6",
    "version": 1,
    "components": [{"name": "component-with-no-type", "version": "1.0.0"}],
}


def purl_type(purl: object) -> str | None:
    """The purl `type` segment (`pkg:<type>/…`), or None if it is not a purl."""
    if not isinstance(purl, str) or not purl.startswith("pkg:"):
        return None
    return purl[len("pkg:") :].split("/", 1)[0]


def declared_in_origins(doc) -> set[str]:
    """Every `fathomdb:declared-in` value carried by a CycloneDX document.

    This is the only place the emitted document says **which tracked files the
    survey actually read**, which makes it the observable that lets a discovery
    criterion be graded at the `run_survey` boundary rather than against
    `discover_manifests()` alone. Components with no declaring manifest (the
    lockfile-only transitives of §5.5) simply contribute nothing here.
    """
    return {
        prop.get("value")
        for component in doc.get("components", [])
        for prop in component.get("properties", [])
        if prop.get("name") == "fathomdb:declared-in" and prop.get("value")
    }


def require(module: str, criterion: str, behaviour: str):
    """Import `module`, or FAIL naming the criterion and the missing behaviour.

    Never skips. A missing implementation is a failed acceptance criterion,
    not an absent test.
    """
    try:
        return importlib.import_module(module)
    except Exception as exc:  # noqa: BLE001 - any import-time failure is a RED result
        pytest.fail(
            f"{criterion} is NOT SATISFIED: `{module}` is not implemented yet"
            " (it lands in 0.8.20 Slice 32).\n"
            f"  REQUIRED BEHAVIOUR: {behaviour}\n"
            f"  import {module!r} raised: {exc!r}"
        )


def require_external(module: str, criterion: str, distribution: str, why: str):
    """Import a THIRD-PARTY oracle module, or FAIL. Never skips.

    Distinct from `require()`: that one imports the code UNDER TEST, and a
    missing module there is the expected Slice-31 RED. This one imports a
    module the test grades WITH, and a missing module there means the criterion
    would go **ungraded**.

    Which is why it must fail rather than skip. `pytest.skip` on a missing
    oracle is the textbook vacuous green: the suite still reports success while
    the assertion it was built for silently stopped running. Every distribution
    reached through this helper is a declared dependency of the mini-project
    (design §5.7), so its absence is a broken environment, not a licence to
    pass.
    """
    try:
        return importlib.import_module(module)
    except Exception as exc:  # noqa: BLE001 - any import failure means "ungraded"
        pytest.fail(
            f"{criterion} CANNOT BE GRADED: the independent oracle `{module}` is"
            " not importable, so the criterion would go UNGRADED.\n"
            f"  INSTALL: {distribution} — a declared dependency of this"
            " mini-project (design §5.7).\n"
            f"  WHY THE ORACLE IS REQUIRED: {why}\n"
            "  This is a FAILURE and never a skip: an oracle allowed to vanish"
            " turns the criterion into a vacuous green.\n"
            f"  import {module!r} raised: {exc!r}"
        )


def independent_cyclonedx_validator(criterion: str):
    """A CycloneDX 1.6 schema validator that does NOT come from `sbom_survey`.

    Returns `validate(doc) -> str | None` — `None` when `doc` is schema-valid,
    a diagnostic string otherwise.

    AC-SBOM-10 exists to stop a hand-rolled document that no consumer will
    validate. Grading it with the implementation's own
    `sbom_survey.cyclonedx.validate()` is **self-certification**: an
    implementation whose `validate()` returns `None` unconditionally would pass
    while emitting invalid JSON (codex §9 round 2, fix-2 finding 1). The oracle
    is therefore the upstream library's own validator, bound to the normative
    1.6 schema shipped by the `cyclonedx-python-lib[json-validation]` extra
    (design §5.7 already declares that dependency for exactly this purpose).
    """
    validation = require_external(
        "cyclonedx.validation.json",
        criterion,
        "cyclonedx-python-lib[json-validation]",
        "AC-SBOM-10 must be graded by a validator INDEPENDENT of the code under"
        " test; the implementation's own validate() cannot certify itself.",
    )
    schema = require_external(
        "cyclonedx.schema",
        criterion,
        "cyclonedx-python-lib[json-validation]",
        "SchemaVersion.V1_6 selects the normative 1.6 schema the emitted"
        " document is graded against.",
    )
    try:
        validator = validation.JsonStrictValidator(schema.SchemaVersion.V1_6)
    except Exception as exc:  # noqa: BLE001 - a validator we cannot build grades nothing
        pytest.fail(
            f"{criterion} CANNOT BE GRADED: cyclonedx-python-lib imported, but its"
            " JSON schema validator could not be constructed — the"
            " `json-validation` extra (the JSON-schema engine plus the bundled"
            " normative schemas) is almost certainly missing. Install"
            " `cyclonedx-python-lib[json-validation]`, per design §5.7."
--- scripts/sbom-survey/tests/test_cli.py
"""AC-SBOM-19 .. AC-SBOM-21 — the CLI contract.

REQ-4 (loud gaps, CLI surface) and REQ-12 (not CI-gating).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.9.

Every test here drives the tool OUT OF PROCESS. That is deliberate: an absent
entry point becomes an ordinary process result instead of a collection error,
so each criterion still reports its own FAILED.
"""

from __future__ import annotations

import json
from pathlib import Path

from conftest import (
    FIXTURE_PREFIX,
    REPO_ROOT,
    TIER_VOCABULARY,
    independent_cyclonedx_validator,
    run_cli,
    tracked_manifest_paths,
)


def test_tool_declares_non_ci_gating_and_is_absent_from_ci_wiring() -> None:
    """AC-SBOM-19.

    The tool is recurring by design but explicitly NOT CI-gating. It says so
    itself (`--describe`), and that declaration is cross-checked against the
    real wiring — a self-description nobody verifies is worthless.

    `--describe` also publishes the tier vocabulary (§5.9), which is how
    downstream tooling discovers the ruled values without importing the
    package. Asserting only `ci_gating` / `recurring` / `name` left that half
    of the contract ungraded — the command could omit `tiers` entirely, or emit
    the wrong values, and the criterion stayed green (codex §9 round 4). The
    field is therefore required to equal `TIER_VOCABULARY`, in the ruled order.
    """
    for wiring in ("scripts/agent-test.sh", ".github/workflows/ci.yml"):
        text = (REPO_ROOT / wiring).read_text(encoding="utf-8")
        assert "sbom-survey" not in text and "sbom_survey" not in text, (
            f"{wiring} references the survey tool. It is informational and must"
            " NOT gate CI (plan-0.8.20.md §3a)."
        )

    proc = run_cli(
        ["--describe"],
        "AC-SBOM-19",
        "`sbom-survey --describe` must print JSON declaring"
        ' {"name": "sbom-survey", "ci_gating": false, "recurring": true,'
        f' "tiers": {list(TIER_VOCABULARY)}}} and exit 0.',
    )
    assert proc.returncode == 0, f"--describe exited {proc.returncode}: {proc.stderr}"
    described = json.loads(proc.stdout)
    assert described["ci_gating"] is False
    assert described["recurring"] is True
    assert described["name"] == "sbom-survey"

    assert "tiers" in described, (
        "`--describe` published no `tiers` field, so downstream tooling cannot"
        " discover the ruled tier vocabulary without importing the package"
        f" (§5.9). Keys present: {sorted(described)}"
    )
    assert tuple(described["tiers"]) == TIER_VOCABULARY, (
        f"`--describe` published tiers {described['tiers']!r}; §5.9 and"
        f" sbom_survey.TIER_VOCABULARY make it exactly {list(TIER_VOCABULARY)},"
        " in that ruled order (shipped first — it is the one that outranks the"
        " others in Slice 33's triage)"
    )


def test_cli_writes_all_artifacts_and_exits_zero(tmp_path: Path) -> None:
    """AC-SBOM-20.

    The happy path: an offline run over the real repository writes all three
    artifacts into the requested output directory and exits 0.

    The CLI is the only path a real consumer takes, so the ARTIFACT IT WRITES is
    what has to be a CycloneDX 1.6 document. `AC-SBOM-10` grades
    `Survey.to_cyclonedx()` in process; checking only `bomFormat` /
    `specVersion` here left the written file able to be anything that carries
    those two strings. It is therefore put through the same INDEPENDENT
    upstream validator (which `independent_cyclonedx_validator` proves bites,
    on a known-invalid control, before returning).
    """
    out = tmp_path / "out"
    proc = run_cli(
        ["--repo", str(REPO_ROOT), "--offline", "--out", str(out)],
        "AC-SBOM-20",
        "`sbom-survey --repo R --offline --out DIR` must exit 0 and write"
        " sbom.cdx.json, staleness.json and staleness.md into DIR — and the"
        " sbom.cdx.json it writes must itself validate against the CycloneDX"
        " 1.6 schema.",
    )
    assert proc.returncode == 0, (
        f"offline survey exited {proc.returncode}\nstdout:\n{proc.stdout}\n"
        f"stderr:\n{proc.stderr}"
    )
    for artifact in ("sbom.cdx.json", "staleness.json", "staleness.md"):
        assert (out / artifact).is_file(), f"{artifact} was not written to {out}"

    doc = json.loads((out / "sbom.cdx.json").read_text(encoding="utf-8"))
    assert doc["bomFormat"] == "CycloneDX"
    assert doc["specVersion"] == "1.6"

    problem = independent_cyclonedx_validator("AC-SBOM-20")(doc)
    assert problem is None, (
        "the sbom.cdx.json the CLI WROTE fails CycloneDX 1.6 schema validation"
        f" (independent upstream validator): {problem}. Two `bomFormat` /"
        " `specVersion` strings do not make a document an SBOM, and this file —"
        " not the in-process object AC-SBOM-10 grades — is what a consumer"
        " reads."
    )


def test_cli_exits_two_naming_the_untiered_manifest(tmp_path: Path) -> None:
    """AC-SBOM-21.

    REQ-4 at the CLI boundary. A tier map that does not cover a tracked
    manifest must exit 2 and name the offending path on stderr — never exit 0
    with an untagged component.

    REQ-4 requires *an* offending path, not a particular one. Asserting the
    literal `Cargo.toml` (codex §9 round 5) was the inverse of every earlier
    finding: too STRICT rather than too permissive, and so able to reject a
    CORRECT implementation — one that walks discovered paths in `git ls-files`
    order legitimately fails first on `Cargo.lock`, names that, and satisfies
    REQ-4 in full. The oracle is therefore the SET of currently-untiered tracked
    manifests, derived from git via the same helper the rest of the suite uses;
    a second hand-written literal list is exactly what drifted in rounds 1-3.
    """
    incomplete = tmp_path / "tiers.toml"
    incomplete.write_text(
        "schema = 1\n\n"
        "[[rule]]\n"
        'prefix = "dev/release/fixtures/"\n'
        'action = "exclude"\n'
        'reason = "fixture"\n',
        encoding="utf-8",
    )

    out = tmp_path / "out"
    proc = run_cli(
        [
            "--repo",
            str(REPO_ROOT),
            "--offline",
            "--out",
            str(out),
            "--tiers",
            str(incomplete),
        ],
        "AC-SBOM-21",
        "with a tier map that covers no real manifest, the CLI must exit 2 and"
        " NAME the untiered path on stderr (never exit 0, never emit an"
        " untagged component).",
    )
    assert proc.returncode == 2, (
        f"expected exit 2 for an untiered manifest, got {proc.returncode}\n"
        f"stderr:\n{proc.stderr}"
    )
    # The map above assigns NO tier: its single rule excludes the fixture
    # prefix. So every tracked manifest outside that prefix is untiered, and any
    # one of them is a correct thing for the CLI to name.
    untiered = [p for p in tracked_manifest_paths() if not p.startswith(FIXTURE_PREFIX)]
    assert untiered, (
        "precondition changed: every tracked manifest now lies under"
        f" {FIXTURE_PREFIX!r}, so this tier map covers the whole repository and"
        " the criterion cannot be graded"
    )
    assert any(path in proc.stderr for path in untiered), (
        "stderr must NAME an offending untiered manifest path so the fix is"
        f" obvious, but none of the {len(untiered)} tracked manifests this tier"
        f" map leaves untiered appears in it. Expected one of {untiered};"
        f" got:\n{proc.stderr}"
    )
--- scripts/sbom-survey/tests/test_cyclonedx.py
"""AC-SBOM-10 .. AC-SBOM-13 — the CycloneDX document.

REQ-6 (CycloneDX 1.6), REQ-7 (resolved versions), REQ-8 (direct vs transitive).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.5.
"""

from __future__ import annotations

import re
from urllib.parse import unquote

from conftest import (
    KNOWN_INVALID_CYCLONEDX_DOC,
    KNOWN_TRANSITIVE_ONLY_CARGO,
    PROJECT_ROOT,
    PURL_PREFIX_BY_ECOSYSTEM,
    PURL_PREFIXES,
    REPO_ROOT,
    TIER_VOCABULARY,
    independent_cyclonedx_validator,
    purl_type,
    require,
    require_external,
)


def _offline_survey(criterion: str, behaviour: str):
    survey_mod = require("sbom_survey.survey", criterion, behaviour)
    registry = require("sbom_survey.registry", criterion, behaviour)
    return survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())


def test_cyclonedx_document_is_schema_valid_and_purl_identified() -> None:
    """AC-SBOM-10.

    The emitted document validates against the bundled CycloneDX 1.6 JSON
    schema. Hand-rolled JSON that no consumer will validate is not an SBOM.

    The schema verdict comes from an INDEPENDENT validator — the upstream
    cyclonedx-python-lib one, not `sbom_survey.cyclonedx.validate()` — with a
    known-invalid negative control proving the oracle actually rejects
    something (codex §9 round 2). The tool's own validate() is then required to
    agree with it in both directions rather than to be believed.

    Schema validity alone is NOT enough, and asserting only it was the gap
    codex §9 round 1 caught: `name` + `version` satisfy the 1.6 schema, so a
    purl-less document would have passed. REQ-6 requires a `purl` PER
    COMPONENT, and §5.5 makes the purl the `bom-ref` — the component identity
    an advisory feed matches a locked version against. A component without one
    is unmatchable, which defeats the reason the SBOM is produced.
    """
    survey = _offline_survey(
        "AC-SBOM-10",
        "Survey.to_cyclonedx() must produce a document that validates against"
        " the bundled CycloneDX 1.6 JSON schema"
        " (cyclonedx-python-lib[json-validation]), AND every component must"
        " carry a `purl` that IS its `bom-ref`, bears the ecosystem prefix"
        f" ({', '.join(PURL_PREFIXES)}) and encodes the component's own"
        " locked version.",
    )
    doc = survey.to_cyclonedx()

    assert doc.get("bomFormat") == "CycloneDX"
    assert doc.get("specVersion") == "1.6"
    assert str(doc.get("serialNumber", "")).startswith("urn:uuid:")
    assert doc.get("components"), "the BOM has no components"

    seen_types: set[str] = set()
    for component in doc["components"]:
        name = component.get("name")
        purl = component.get("purl")
        assert purl, (
            f"{name!r}: component has NO purl — REQ-6 requires one per component,"
            " and without it the component cannot be matched against an advisory"
        )
        assert purl.startswith(PURL_PREFIXES), (
            f"{name!r}: purl {purl!r} does not carry a recognized ecosystem"
            f" prefix (expected one of {PURL_PREFIXES})"
        )
        assert component.get("bom-ref") == purl, (
            f"{name!r}: bom-ref {component.get('bom-ref')!r} != purl {purl!r} —"
            " §5.5 makes the purl the bom-ref so refs are stable across runs"
        )
        seen_types.add(purl_type(purl) or "")

        props = {p["name"]: p["value"] for p in component.get("properties", [])}
        if props.get("fathomdb:resolution") != "unresolved":
            version = component.get("version")
            assert version, f"{purl}: resolved component has no version"
            assert "@" in purl, (
                f"{purl}: a resolved component's purl must pin its version"
            )
            assert unquote(purl.rsplit("@", 1)[-1]) == version, (
                f"{purl}: purl version does not match component version"
                f" {version!r} — the identity and the reported version disagree"
            )

    assert seen_types == set(PURL_PREFIX_BY_ECOSYSTEM), (
        "the BOM's purl types must be exactly"
        f" {sorted(PURL_PREFIX_BY_ECOSYSTEM)} — this repo tracks cargo, npm and"
        f" pypi manifests, so all three must be represented; got {sorted(seen_types)}"
    )

    # --- schema validity, graded by an INDEPENDENT oracle -------------------
    #
    # This half used to call `sbom_survey.cyclonedx.validate()` — a function
    # from the implementation under test — which is self-certification: a
    # `validate()` that returns None unconditionally passed while the tool
    # emitted invalid CycloneDX JSON, i.e. exactly the hand-rolled-SBOM failure
    # this criterion exists to stop (codex §9 round 2, fix-2 finding 1). The
    # oracle is now the upstream library's own 1.6 schema validator.
    schema_valid = independent_cyclonedx_validator("AC-SBOM-10")

    # Negative control FIRST — prove the oracle bites before trusting a clean
    # result from it. An "independent validator" that accepts anything is no
    # better than the self-certifying one it replaced.
    assert schema_valid(KNOWN_INVALID_CYCLONEDX_DOC) is not None, (
        "the independent CycloneDX 1.6 validator ACCEPTED a document whose only"
        " component carries no `type`, which the 1.6 schema requires. The"
        " oracle is not validating, so a clean verdict from it proves nothing."
    )

    problem = schema_valid(doc)
    assert problem is None, f"CycloneDX 1.6 schema validation failed: {problem}"

    # The tool's own validate() must AGREE with the independent oracle in BOTH
    # directions. The second assertion is the one that kills a stub: a
    # `validate()` that returns None unconditionally agrees on the valid
    # document and is caught here.
    validator_mod = require(
        "sbom_survey.cyclonedx",
        "AC-SBOM-10",
        "sbom_survey.cyclonedx.validate(doc) must really run the CycloneDX 1.6"
        " schema — returning None for a valid document AND a diagnostic for an"
        " invalid one. It is cross-checked against the independent"
        " cyclonedx-python-lib validator in both directions.",
    )
    assert validator_mod.validate(doc) is None, (
        "sbom_survey.cyclonedx.validate() rejected a document the independent"
        f" CycloneDX 1.6 validator accepts: {validator_mod.validate(doc)}"
    )
    assert validator_mod.validate(KNOWN_INVALID_CYCLONEDX_DOC) is not None, (
        "sbom_survey.cyclonedx.validate() returned None for a document the"
        " independent CycloneDX 1.6 validator REJECTS — it is not running the"
        " schema, so it certifies nothing and must never be the oracle for"
        " this criterion."
    )


def test_every_component_carries_a_tier_property() -> None:
    """AC-SBOM-11.

    Tier tagging is what made TC-93 a cheap call. Every component carries
    exactly one `fathomdb:tier` property, and its value is in the ruled
    vocabulary.

    Membership in the vocabulary is NOT on its own a grade of the value: an
    implementation that stamped `dev-tooling` on all 400 components would
    satisfy it while every tier in the BOM was wrong, and Slice 33 prioritises
    on exactly this field. So the tag is also required to AGREE with
    `TierMap.classify()` of the manifest that declares the component — which is
    the rule set `AC-SBOM-23` grades for longest-prefix correctness and
    `AC-SBOM-06` proves `run_survey()` consults. Only components with EXACTLY
    ONE declaring manifest have their tier VALUE checked: a package declared by
    two manifests of different tiers has no single ruled answer in §5.2/§5.3, so
    demanding one would invent a contract.

    A component with NO origin used to be skipped outright, which was codex §9
    round 6's finding. A `transitive` component legitimately has none — §5.5
    defines it as one whose name reaches no tracked dependency table — but a
    `direct` one CANNOT, by that same definition. Silently exempting it let
    UNTRACKED-MANIFEST LEAKAGE through both this oracle and the discovery
    boundary: `run_survey()` could read `python/pyproject.toml` (gitignored, the
    exact case `AC-SBOM-02` exists to prevent), have no tracked path to
    attribute its dependencies to, emit them originless, and still pass so long
    as one other component carried an origin. Zero-origin `direct` components
    are therefore a FAILURE; zero-origin `transitive` ones stay exempt.
    """
    survey = _offline_survey(
        "AC-SBOM-11",
        "every CycloneDX component must carry EXACTLY ONE `fathomdb:tier`"
        f" property whose value is one of {TIER_VOCABULARY!r}, and that value"
        " must EQUAL TierMap.classify(<its declaring manifest>).tier for every"
        " component declared by exactly one manifest. Every `direct` component"
        " must carry at least one `fathomdb:declared-in` origin — a direct"
        " package with none means an untracked manifest was read.",
    )
    doc = survey.to_cyclonedx()

    for component in doc["components"]:
        tiers = [
            p["value"]
            for p in component.get("properties", [])
            if p.get("name") == "fathomdb:tier"
        ]
        ref = component.get("bom-ref", component.get("name"))
        assert len(tiers) == 1, f"{ref}: expected one fathomdb:tier, got {tiers}"
        assert tiers[0] in TIER_VOCABULARY, f"{ref}: bad tier {tiers[0]!r}"

    # --- the value itself, against the tracked rules ------------------------
    tiers_mod = require(
        "sbom_survey.tiers",
        "AC-SBOM-11",
        "the tracked tiers.toml is the source of record for a component's tier;"
        " the property must carry the tier that map assigns to the declaring"
        " manifest, not merely some value from the vocabulary.",
    )
    tier_map = tiers_mod.load_tier_map(PROJECT_ROOT / "tiers.toml")

    graded = 0
    for component in doc["components"]:
        props = component.get("properties", [])
        origins = [
            p["value"] for p in props if p.get("name") == "fathomdb:declared-in"
        ]
        ref = component.get("bom-ref", component.get("name"))

        if not origins:
            depth = next(
                (p["value"] for p in props if p.get("name") == "fathomdb:depth"),
--- scripts/sbom-survey/tests/test_discovery.py
"""AC-SBOM-01 .. AC-SBOM-04 — manifest discovery.

REQ-1 (discovery is `git ls-files`-derived) and REQ-2 (completeness).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.1.
"""

from __future__ import annotations

from pathlib import Path

from conftest import (
    REPO_ROOT,
    declared_in_origins,
    require,
    tracked_manifest_paths,
    tracked_paths,
)


def test_discovery_uses_git_ls_files_not_a_filesystem_walk(tmp_path: Path) -> None:
    """AC-SBOM-01.

    Discovery takes its candidate set from an injected `git ls-files` runner.
    A manifest that exists on disk but is NOT reported by that runner must be
    absent from the result — that is what makes `python/pyproject.toml`,
    `target/**` and `node_modules/**` structurally unreachable.

    Proving it against `discover_manifests()` alone is not enough: Slice 32
    could honour the injected runner there and still walk the filesystem inside
    `run_survey()`, which is the function that actually has to call it. The
    second leg therefore drives a real survey and requires every manifest the
    document attributes a component to (`fathomdb:declared-in`) to be a path
    git tracks.
    """
    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-01",
        "discover_manifests(root, *, ls_files=...) must derive its candidate set"
        " from the injected `git ls-files` runner ONLY; a manifest present on"
        " disk but untracked must never appear in the result.",
    )

    # Tracked, per the injected runner.
    (tmp_path / "Cargo.toml").write_text("[package]\nname='x'\n", encoding="utf-8")
    # On disk but NOT reported by the runner -> must be invisible.
    untracked = tmp_path / "vendored"
    untracked.mkdir()
    (untracked / "package.json").write_text("{}", encoding="utf-8")

    calls: list[Path] = []

    def fake_ls_files(root: Path) -> list[str]:
        calls.append(root)
        return ["Cargo.toml"]

    refs = discovery.discover_manifests(tmp_path, ls_files=fake_ls_files)

    assert calls, "the injected ls_files runner was never called"
    found = {r.path for r in refs}
    assert found == {"Cargo.toml"}, (
        "discovery must return exactly what git reports; got "
        f"{sorted(found)} — an untracked on-disk manifest leaked in"
    )

    # --- and now at the SURVEY BOUNDARY, which is the caller that matters ----
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-01",
        "run_survey() must take its candidate manifests from the same"
        " git-derived discovery: every `fathomdb:declared-in` origin in the"
        " emitted document must be a path `git ls-files` reports, so an"
        " untracked on-disk manifest (target/**, node_modules/**, the"
        " gitignored python/ tree) can never contribute a component.",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-01",
        "an OfflineSource keeps the survey-boundary run hermetic.",
    )

    doc = survey_mod.run_survey(
        REPO_ROOT, published=registry.OfflineSource()
    ).to_cyclonedx()
    origins = declared_in_origins(doc)
    assert origins, (
        "vacuous-pass guard: not one component recorded a `fathomdb:declared-in`"
        " manifest, so this leg would grade nothing"
    )
    untracked_origins = sorted(origins - set(tracked_manifest_paths()))
    assert not untracked_origins, (
        "run_survey() attributed components to manifests that git does not"
        f" track: {untracked_origins}. Discovery is `git ls-files`-derived"
        " (REQ-1); an origin outside that set means the survey walked the"
        " filesystem instead of asking git."
    )


def test_gitignored_top_level_python_dir_is_out_of_scope() -> None:
    """AC-SBOM-02.

    `.gitignore` ignores `/python/` in its entirety (the sourceless recovered
    binding kept on disk for Memex). `python/pyproject.toml` is therefore
    UNTRACKED and out of BOM scope, even on a developer machine where the
    directory exists. This is the LBS charter §2 "noise against gitignored
    dev/eval lockfiles" trap.
    """
    assert not any(p.startswith("python/") for p in tracked_paths()), (
        "precondition changed: `python/` is now tracked — the design's scope"
        " ruling (§5.1) must be revisited before this criterion is re-graded"
    )

    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-02",
        "discover_manifests(REPO_ROOT) must return no path under `python/`,"
        " because `.gitignore` ignores the whole `/python/` tree and the SBOM"
        " scope is `everything tracked on main`.",
    )

    refs = discovery.discover_manifests(REPO_ROOT)
    leaked = sorted(r.path for r in refs if r.path.startswith("python/"))
    assert not leaked, f"gitignored `/python/` leaked into the BOM scope: {leaked}"

    # --- and at the SURVEY BOUNDARY: "out of BOM scope" is a statement about
    # the emitted BOM, so it is graded there too. discover_manifests() staying
    # clean does not stop run_survey() from reading `python/pyproject.toml`
    # directly.
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-02",
        "no component in the emitted document may be attributed to a manifest"
        " under `python/`: the whole tree is gitignored and therefore outside"
        " the BOM scope (§5.1).",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-02",
        "an OfflineSource keeps the survey-boundary run hermetic.",
    )

    doc = survey_mod.run_survey(
        REPO_ROOT, published=registry.OfflineSource()
    ).to_cyclonedx()
    leaked_origins = sorted(
        o for o in declared_in_origins(doc) if o.startswith("python/")
    )
    assert not leaked_origins, (
        "the gitignored `/python/` tree reached the emitted BOM through"
        f" run_survey(): {leaked_origins}"
    )


def test_every_tracked_manifest_is_discovered() -> None:
    """AC-SBOM-03.

    Rule-based, not a frozen list: every tracked path whose basename is a
    recognized manifest name must be discovered. Nothing tracked is silently
    missed (REQ-2).
    """
    expected = tracked_manifest_paths()
    assert expected, "vacuous-pass guard: git reported zero tracked manifests"

    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-03",
        "discover_manifests(REPO_ROOT) must return EVERY tracked path whose"
        " basename is a recognized manifest/lockfile name — computed from"
        " `git ls-files` at run time, not a frozen list.",
    )

    found = {r.path for r in discovery.discover_manifests(REPO_ROOT)}
    missing = sorted(set(expected) - found)
    assert not missing, f"tracked manifests not discovered: {missing}"


def test_ecosystem_and_kind_classification() -> None:
    """AC-SBOM-04.

    Each ref carries ecosystem in {cargo, npm, pypi} and kind in
    {manifest, lockfile}. Checked against four real tracked paths.
    """
    discovery = require(
        "sbom_survey.discovery",
        "AC-SBOM-04",
        "each ManifestRef must carry ecosystem in {cargo, npm, pypi} and kind"
        " in {manifest, lockfile}; e.g. Cargo.lock -> (cargo, lockfile),"
        " src/ts/package.json -> (npm, manifest), src/python/uv.lock ->"
        " (pypi, lockfile), tools/docs/requirements.txt -> (pypi, manifest).",
    )

    by_path = {r.path: r for r in discovery.discover_manifests(REPO_ROOT)}
    expected = {
        "Cargo.lock": ("cargo", "lockfile"),
        "src/ts/package.json": ("npm", "manifest"),
        "src/python/uv.lock": ("pypi", "lockfile"),
        "tools/docs/requirements.txt": ("pypi", "manifest"),
    }
    for path, (ecosystem, kind) in expected.items():
        ref = by_path.get(path)
        assert ref is not None, f"{path} was not discovered"
        assert (ref.ecosystem, ref.kind) == (ecosystem, kind), (
            f"{path}: expected ({ecosystem}, {kind}), got "
            f"({ref.ecosystem}, {ref.kind})"
        )
--- scripts/sbom-survey/tests/test_paths.py
"""AC-SBOM-18 — gitignored reports, tracked findings home.

REQ-11. Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.6.
"""

from __future__ import annotations

from conftest import REPO_ROOT, is_gitignored, require


def test_report_dir_is_gitignored_and_findings_home_is_tracked() -> None:
    """AC-SBOM-18.

    Generated reports are gitignored (HITL-ruled). Slice 33's *findings*
    therefore need a separate, deliberately NOT-ignored durable home — the raw
    tool output is not one.

    The home is `dev/plans/runs/` — the dominant house convention for a dated
    run report (`0.8.2-m1-FINDINGS.md`, `0.8.3-rerank-tune-FINDINGS.md`,
    `0.8.4-cost-probe-FINDINGS.md`), and where the slice's own `-output.json`
    already lands. See design §5.6 for the comparison against `dev/design/`
    and `dev/deps/`.
    """
    paths = require(
        "sbom_survey.paths",
        "AC-SBOM-18",
        "sbom_survey.paths must expose DEFAULT_REPORT_DIR (repo-relative,"
        " GITIGNORED — Slice 32 adds the `scripts/sbom-survey/out/` rule) and"
        " SLICE_33_FINDINGS_DOC (repo-relative, NOT ignored — the tracked"
        " durable home for the survey findings).",
    )

    report_dir = paths.DEFAULT_REPORT_DIR
    findings_doc = paths.SLICE_33_FINDINGS_DOC

    assert report_dir == "scripts/sbom-survey/out"
    assert findings_doc == "dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md"

    probe = f"{report_dir}/sbom.cdx.json"
    assert is_gitignored(probe), (
        f"{probe} is NOT gitignored — generated reports must never be"
        " committable (add `scripts/sbom-survey/out/` to .gitignore)"
    )
    assert not is_gitignored(findings_doc), (
        f"{findings_doc} is gitignored — it is the TRACKED durable home for"
        " Slice 33's findings and must be committable"
    )
    assert (REPO_ROOT / "dev" / "plans" / "runs").is_dir()
--- scripts/sbom-survey/tests/test_registry.py
"""AC-SBOM-14 .. AC-SBOM-17 — the published-version seam.

REQ-9 (injectable registry), REQ-10 (honest degradation).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.4.

A false "up-to-date" is the single worst output this tool can produce: it would
let a live advisory be closed as `CLOSE-satisfied` in the LIBRARY-BUMP-STEWARD
§2 triage. AC-SBOM-15 and AC-SBOM-16 are the two named guards against it.
"""

from __future__ import annotations

import re
import socket
from collections import Counter

import pytest

from conftest import REPO_ROOT, require

# --- AC-SBOM-17 survey-boundary probe data ----------------------------------
#
# A locked version safe to build sentinels around: three numeric segments (so
# semver parses it as well as PEP 440) with a major of at least 1 (so
# `_LOWER_SENTINEL` is unambiguously below it under both orderings).
_PROBE_VERSION = re.compile(r"^(\d+)\.\d+\.\d+$")

# Chosen to be beyond anything this repository could plausibly lock, and to
# parse under BOTH comparators (§5.4: semver for cargo/npm, PEP 440 for pypi),
# so the expected verdict does not depend on which package the probe lands on.
_HIGHER_SENTINEL = "9999.0.0"
_LOWER_SENTINEL = "0.0.1"


def test_survey_performs_no_network_io_by_default() -> None:
    """AC-SBOM-14.

    `run_survey` takes `published` as a REQUIRED keyword-only argument, so no
    code path reaches the network implicitly. With `OfflineSource` a full
    survey must open zero sockets — proved by making `socket.socket` explode.
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-14",
        "run_survey(root, *, published=...) must take the published-version"
        " source as a REQUIRED keyword-only argument and must perform ZERO"
        " socket I/O when given an OfflineSource.",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-14",
        "registry.OfflineSource must be a PublishedVersionSource that never"
        " touches the network.",
    )

    with pytest.raises(TypeError):
        survey_mod.run_survey(REPO_ROOT)  # `published` must not have a default

    real_socket = socket.socket
    opened: list[object] = []

    def exploding_socket(*args, **kwargs):
        opened.append(args)
        raise AssertionError("the offline survey opened a socket")

    socket.socket = exploding_socket  # type: ignore[assignment]
    try:
        survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())
    finally:
        socket.socket = real_socket  # type: ignore[assignment]

    assert not opened, "the offline survey opened a socket"


def test_unknown_latest_is_never_reported_current() -> None:
    """AC-SBOM-15.

    With no registry available, every row is `unknown`. NOT ONE row may be
    `current`: "we could not check" must never be rendered as "up to date".
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-15",
        "when the published-version source returns None for every lookup, every"
        " staleness row must have status == 'unknown' and latest_version is"
        " None. NO row may be 'current' — an unknown latest must never be"
        " rendered as up-to-date.",
    )
    registry = require("sbom_survey.registry", "AC-SBOM-15", "OfflineSource returns None for every lookup.")

    survey = survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())
    rows = survey.staleness()
    assert rows, "vacuous-pass guard: the survey produced zero staleness rows"

    falsely_current = [r.name for r in rows if r.status == "current"]
    assert not falsely_current, (
        "offline run reported packages as 'current' with no registry data: "
        f"{sorted(falsely_current)}"
    )
    for row in rows:
        assert row.status == "unknown", f"{row.name}: expected 'unknown', got {row.status!r}"
        assert row.latest_version is None, f"{row.name}: latest_version leaked {row.latest_version!r}"


def test_registry_error_degrades_to_unknown() -> None:
    """AC-SBOM-16.

    A source that RAISES (DNS failure, 503, timeout, malformed JSON) degrades
    to `unknown` with the reason recorded. It must not crash the run and must
    not fall back to `current`.
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-16",
        "a PublishedVersionSource that raises must degrade the affected rows to"
        " status='unknown' with `lookup_error` recorded — the run must not"
        " crash, and must not fall back to 'current'.",
    )
    require("sbom_survey.registry", "AC-SBOM-16", "the source protocol lives here.")

    class ExplodingSource:
        def latest(self, ecosystem: str, name: str) -> str | None:
            raise ConnectionError(f"registry unreachable for {ecosystem}/{name}")

    survey = survey_mod.run_survey(REPO_ROOT, published=ExplodingSource())
    rows = survey.staleness()
    assert rows, "vacuous-pass guard: the survey produced zero staleness rows"

    for row in rows:
        assert row.status == "unknown", f"{row.name}: expected 'unknown', got {row.status!r}"
        assert row.lookup_error, f"{row.name}: the failure reason was not recorded"


def test_static_source_classifies_current_outdated_ahead() -> None:
    """AC-SBOM-17.

    The positive path, with an injected source and no network: locked < latest
    is `outdated`, locked == latest is `current`, locked > latest is `ahead`.
    Both comparators are exercised (semver for cargo/npm, PEP 440 for pypi).

    TWO LEGS, and the second is the one that matters. Grading `classify_status()`
    alone left the **used-vs-published diff — the output this tool exists to
    produce — untested end to end** (codex §9 round 4): Slice 32 could ship a
    perfect comparator while `run_survey(…, published=…)` never called
    `published.latest()` at all, leaving every row `unknown`, and AC-SBOM-15/16
    would still pass because both of those inject a source that legitimately
    yields `unknown` for everything. Leg 2 therefore drives a POSITIVE
    `StaticSource` through `run_survey` and requires `current`, `outdated` and
    `ahead` to actually appear on the resulting staleness rows.

    Leg 1 survives because the comparator matrix — prereleases, PEP 440
    `.postN`, unparseable input, either side missing — cannot be provoked from
    whatever this repository happens to have locked.
    """
    staleness_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-17",
        "classify_status(ecosystem, locked, latest) must return 'outdated' when"
        " locked < latest, 'current' when equal, 'ahead' when locked > latest,"
        " and 'unknown' when either side is None or unparseable — using semver"
        " ordering for cargo/npm and PEP 440 ordering for pypi; AND"
        " run_survey(…, published=StaticSource(…)) must feed those verdicts"
        " onto the staleness rows.",
    )

    cases = [
        ("cargo", "1.0.0", "1.2.0", "outdated"),
        ("cargo", "1.2.0", "1.2.0", "current"),
        ("cargo", "1.3.0", "1.2.0", "ahead"),
        ("cargo", "1.2.3-rc.1", "1.2.3", "outdated"),
        ("npm", "0.22.1", "0.23.0", "outdated"),
        ("pypi", "1.6.1", "1.6.1", "current"),
        ("pypi", "1.6.1", "1.7.0", "outdated"),
        ("pypi", "2.0.0.post1", "2.0.0", "ahead"),
        ("cargo", None, "1.2.0", "unknown"),
        ("cargo", "1.2.0", None, "unknown"),
        ("cargo", "not-a-version", "1.2.0", "unknown"),
    ]
    for ecosystem, locked, latest, expected in cases:
        got = staleness_mod.classify_status(ecosystem, locked, latest)
        assert got == expected, (
            f"{ecosystem} locked={locked!r} latest={latest!r}: expected"
            f" {expected!r}, got {got!r}"
        )

    # --- LEG 2: the same three verdicts, THROUGH `run_survey` ----------------
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-17",
        "registry.StaticSource({(ecosystem, name): version}) must return the"
        " mapped version and None for anything absent (§5.4), and run_survey"
        " must call it once per surveyed package.",
    )

    # The probe versions are taken from a real offline run rather than hardcoded,
    # so the leg cannot rot when a lockfile moves.
    baseline_rows = staleness_mod.run_survey(
        REPO_ROOT, published=registry.OfflineSource()
    ).staleness()
    assert baseline_rows, "vacuous-pass guard: the survey produced zero staleness rows"

    occurrences = Counter((r.ecosystem, r.name) for r in baseline_rows)
    probes = [
        r
        for r in baseline_rows
        if occurrences[(r.ecosystem, r.name)] == 1
        and isinstance(r.locked_version, str)
        and _PROBE_VERSION.match(r.locked_version)
        and int(_PROBE_VERSION.match(r.locked_version).group(1)) >= 1
    ]
    assert len(probes) >= 3, (
        "vacuous-pass guard: fewer than three staleness rows carry a unique"
        " (ecosystem, name) and a simple `major.minor.patch` locked version with"
        f" major >= 1, so the three verdicts cannot be provoked; found"
        f" {len(probes)}"
    )

    current_row, outdated_row, ahead_row = probes[0], probes[1], probes[2]
    published = {
        (current_row.ecosystem, current_row.name): current_row.locked_version,
--- scripts/sbom-survey/tests/test_report.py
"""AC-SBOM-22 — the Slice-33 consumer contract.

REQ-13 (determinism) and REQ-14 (consumer fields).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.8.

Slice 33 answers exactly two questions and stops: what is stale, and would a
surgical ~1-5 SLOC change likely land it. This report must let it answer both
without re-deriving anything. Nothing in Slice 33 is built here.
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

import pytest

from conftest import REPO_ROOT, require, run_cli

SLICE_33_ROW_FIELDS = {
    "ecosystem",
    "name",
    "tier",
    "depth",
    "locked_version",
    "latest_version",
    "status",
    "lookup_error",
    "declared_in",
    "edit_sites",
    "edit_site_count",
}

# A default-path timestamp may not be wall-clock. §5.8 rules the default a FIXED
# epoch, so any stamp within a day of the real clock is a wall-clock default.
# The window is deliberately generous: it must not be tripped by a slow run, a
# timezone slip or a machine whose clock is a few hours out, only by an
# implementation that actually asks the operating system what time it is.
WALL_CLOCK_WINDOW_SECONDS = 24 * 60 * 60


def _assert_identical(first: dict[str, bytes], second: dict[str, bytes], what: str) -> None:
    assert first and second, f"no artifacts were written {what}"
    assert first.keys() == second.keys(), (
        f"a different artifact set was written {what}: {sorted(first)} vs {sorted(second)}"
    )
    for name in first:
        assert first[name] == second[name], (
            f"{name} is not byte-identical {what} — the report is"
            " non-deterministic, so a recurring re-run cannot diff cleanly"
        )


def test_staleness_rows_carry_slice_33_fields_and_are_deterministic(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """AC-SBOM-22.

    REQ-13 is about ORDINARY repeated runs, so the DEFAULT timestamp path is the
    one that has to be covered. Pinning `now` on both runs — all this test used
    to do — leaves an implementation that falls back to wall-clock whenever
    `now` / `--now` is omitted passing (codex §9 round 2, fix-2 finding 3).
    Three determinism legs are therefore run: pinned `now`, the in-process
    default, and the CLI default (argparse has a default of its own and could
    hand `run_survey` a wall-clock `now` explicitly, bypassing the other two).

    The consumer contract is then graded twice: once on `Survey.staleness()`,
    and once on the `staleness.json` the CLI WROTE — because that file, not the
    in-process object, is the thing Slice 33 opens.
    """
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-22",
        "each staleness row must carry exactly the Slice-33 consumer fields"
        f" {sorted(SLICE_33_ROW_FIELDS)}, sorted by"
        " (tier, ecosystem, name, locked_version); and two runs over identical"
        " inputs must write BYTE-IDENTICAL artifacts — WITH and WITHOUT an"
        " explicit `now`, in-process and through the CLI (UUIDv5 serialNumber,"
        " a FIXED default epoch, no wall-clock timestamp) so a re-run diffs to"
        " nothing.",
    )
    registry = require("sbom_survey.registry", "AC-SBOM-22", "OfflineSource keeps the run hermetic.")
    report = require(
        "sbom_survey.report",
        "AC-SBOM-22",
        "sbom_survey.report.write_reports(survey, out_dir) must emit"
        " sbom.cdx.json, staleness.json and staleness.md deterministically, and"
        ' staleness.json must be {"generated", "source", "summary", "rows"} with'
        " every row carrying the Slice-33 field set in the ruled sort order.",
    )

    # Cleared so the PURE built-in default is exercised, not an environment
    # override — and so the CLI subprocesses below inherit the same posture.
    monkeypatch.delenv("SOURCE_DATE_EPOCH", raising=False)

    def once(out: Path, **kwargs) -> dict[str, bytes]:
        survey = survey_mod.run_survey(
            REPO_ROOT, published=registry.OfflineSource(), **kwargs
        )
        report.write_reports(survey, out)
        return {p.name: p.read_bytes() for p in sorted(out.iterdir())}

    # Leg 1 — an explicit, pinned `now`.
    pinned = "1970-01-01T00:00:00Z"
    _assert_identical(
        once(tmp_path / "a", now=pinned),
        once(tmp_path / "b", now=pinned),
        "across two runs with an explicit `now`",
    )

    # Leg 2 — the DEFAULT path, which is what an ordinary re-run takes.
    default_first = once(tmp_path / "c")
    _assert_identical(
        default_first,
        once(tmp_path / "d"),
        "across two DEFAULT runs (no explicit `now`)",
    )

    # Byte-equality alone does not settle leg 2: two back-to-back runs can share
    # a wall-clock second and look identical by luck. The default stamp must
    # therefore be shown NOT to be wall-clock at all.
    stamp = json.loads(default_first["sbom.cdx.json"])["metadata"]["timestamp"]
    parsed = datetime.fromisoformat(str(stamp).replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    drift = abs((datetime.now(timezone.utc) - parsed).total_seconds())
    assert drift > WALL_CLOCK_WINDOW_SECONDS, (
        f"the DEFAULT run stamped metadata.timestamp {stamp!r}, which is within"
        " a day of the real clock. §5.8 rules the default a FIXED epoch"
        " (SOURCE_DATE_EPOCH or an explicit --now are the only overrides); a"
        " wall-clock default makes every re-run diff, and two runs inside the"
        " same second would still compare byte-identical, so the equality check"
        " above cannot catch it on its own."
    )

    # Leg 3 — the CLI default. `--now` could be defaulted to the current time in
    # the argument parser and passed explicitly to run_survey, which would sail
    # past both legs above.
    cli_runs: list[dict[str, bytes]] = []
    for name in ("cli-a", "cli-b"):
        out = tmp_path / name
        proc = run_cli(
            ["--repo", str(REPO_ROOT), "--offline", "--out", str(out)],
            "AC-SBOM-22",
            "two `sbom-survey --repo R --offline --out DIR` runs with NO --now"
            " must write byte-identical artifacts, and must stamp the same"
            " fixed default timestamp run_survey() uses in-process.",
        )
        assert proc.returncode == 0, (
            f"offline CLI run exited {proc.returncode}\nstderr:\n{proc.stderr}"
        )
        cli_runs.append({p.name: p.read_bytes() for p in sorted(out.iterdir())})

    _assert_identical(*cli_runs, what="across two CLI runs with no --now")

    cli_stamp = json.loads(cli_runs[0]["sbom.cdx.json"])["metadata"]["timestamp"]
    assert cli_stamp == stamp, (
        f"the CLI stamped {cli_stamp!r} where run_survey()'s own default is"
        f" {stamp!r} — `--now` is being defaulted in the argument parser, which"
        " bypasses the deterministic default proved above"
    )

    # --- the Slice-33 consumer field set ------------------------------------
    survey = survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())
    rows = [r.as_dict() for r in survey.staleness()]
    assert rows, "vacuous-pass guard: zero staleness rows"

    for row in rows:
        assert set(row) == SLICE_33_ROW_FIELDS, (
            "staleness row field set drifted from the Slice-33 contract:"
            f" missing={sorted(SLICE_33_ROW_FIELDS - set(row))}"
            f" unexpected={sorted(set(row) - SLICE_33_ROW_FIELDS)}"
        )
        assert isinstance(row["edit_sites"], list)
        assert row["edit_site_count"] == len(row["edit_sites"])
        for site in row["edit_sites"]:
            assert not site.startswith("/"), "edit_sites must be repo-relative"

    keys = [(r["tier"], r["ecosystem"], r["name"], r["locked_version"] or "") for r in rows]
    assert keys == sorted(keys), "staleness rows are not deterministically sorted"

    # --- and in the ARTIFACT, which is what Slice 33 actually opens ----------
    #
    # Everything above grades `Survey.staleness()` in process. Slice 33 never
    # calls that; it reads `staleness.json`, so `report.write_reports()` could
    # project a different, poorer row shape and the consumer contract would
    # still be green. §5.8 fixes both the envelope and the row.
    written = json.loads(cli_runs[0]["staleness.json"])
    assert set(written) == {"generated", "source", "summary", "rows"}, (
        "staleness.json's top-level envelope drifted from §5.8's"
        ' {"generated", "source", "summary", "rows"}: got'
        f" {sorted(written)}"
    )
    assert written["source"] == "offline", (
        "an --offline run must record source='offline' — Slice 33 uses it to"
        f" tell a checked run from an unchecked one; got {written['source']!r}"
    )
    assert written["rows"], "vacuous-pass guard: staleness.json carries zero rows"

    for row in written["rows"]:
        assert set(row) == SLICE_33_ROW_FIELDS, (
            "a staleness.json row does not carry the Slice-33 field set:"
            f" missing={sorted(SLICE_33_ROW_FIELDS - set(row))}"
            f" unexpected={sorted(set(row) - SLICE_33_ROW_FIELDS)}"
        )

    written_keys = [
        (r["tier"], r["ecosystem"], r["name"], r["locked_version"] or "")
        for r in written["rows"]
    ]
    assert written_keys == sorted(written_keys), (
        "staleness.json's rows are not in the (tier, ecosystem, name,"
        " locked_version) order §5.8 fixes, so a recurring re-run cannot diff"
        " cleanly even when the survey itself is stable"
    )
--- scripts/sbom-survey/tests/test_tiering.py
"""AC-SBOM-05 .. AC-SBOM-09 and AC-SBOM-23 — tiering, fixture exclusion, the
loud gap, and the longest-prefix matching rule.

REQ-3 (tiering), REQ-4 (loud gaps), REQ-5 (fixture exclusion).
Design: dev/design/0.8.20-slice-31-sbom-survey-tool.md §5.2 and §5.3.

`AC-SBOM-23` carries a criterion id out of file order: it was added at fix-3
(codex §9 round 3) and numbered last so that AC-SBOM-10..22 keep the ids the
design, the README and the closure JSON already cite. Its subject matter is
§5.3, which is why the test lives here beside AC-SBOM-05..09.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from conftest import (
    FIXTURE_PREFIX,
    PROJECT_ROOT,
    REPO_ROOT,
    TIER_VOCABULARY,
    require,
    tracked_manifest_paths,
)


def _fixture_manifests() -> list[str]:
    return [p for p in tracked_manifest_paths() if p.startswith(FIXTURE_PREFIX)]


# --- AC-SBOM-23 overlapping-rule fixture data (design §5.3) -----------------
#
# `_SPECIFIC_PREFIX` is a PROPER PREFIX EXTENSION of `_BROAD_PREFIX`, and the
# two map to DIFFERENT tiers, so first-match-wins and longest-prefix-wins give
# DIFFERENT answers for `_OVERLAP_PATH`. That divergence is the whole point:
# with two rules that agree, or that cannot both match, every matching strategy
# looks identical and the criterion would be vacuous.
#
# Neither path needs to exist on disk. `TierMap.classify()` is a pure function
# of the rule set and the path string, and deliberately so — a rule for a
# subtree that does not exist yet must still be expressible.
_BROAD_PREFIX = "dev/tools/"
_SPECIFIC_PREFIX = "dev/tools/vendored-shipped/"
_OVERLAP_PATH = _SPECIFIC_PREFIX + "Cargo.toml"
_BROAD_ONLY_PATH = _BROAD_PREFIX + "mermaid/package.json"

# A tier file with the SAME prefix twice — a load-time error per §5.3. Written
# to pytest's `tmp_path` scratch dir at test time; Slice 31 creates no tracked
# `tiers.toml` (that is a Slice 32 artifact, §6).
_DUPLICATE_PREFIX_TOML = f"""\
schema = 1

[[rule]]
prefix = "{_BROAD_PREFIX}"
action = "tier"
tier   = "dev-tooling"

[[rule]]
prefix = "{_BROAD_PREFIX}"
action = "tier"
tier   = "shipped"
"""


def test_release_fixtures_are_excluded_and_auditable() -> None:
    """AC-SBOM-05.

    `dev/release/fixtures/cargo-skew/**` and `dev/release/fixtures/pip-skew/**`
    are deliberately fake, deliberately skewed manifests that exist to make the
    release version-skew gates demonstrate their catch. They must contribute
    ZERO components, and the exclusion must be RECORDED (auditable), not silent.
    """
    fixtures = _fixture_manifests()
    assert len(fixtures) == 8, (
        "precondition changed: expected the 8 tracked skew fixtures, found "
        f"{len(fixtures)}: {fixtures}"
    )

    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-05",
        "run_survey() must exclude every dev/release/fixtures/** manifest from"
        " `components` AND record each one in `survey.excluded` with"
        " reason='fixture' — excluded, but auditable, never silently dropped.",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-05",
        "an OfflineSource is needed to run the survey without network.",
    )

    survey = survey_mod.run_survey(REPO_ROOT, published=registry.OfflineSource())

    excluded = {e.path: e.reason for e in survey.excluded}
    for path in fixtures:
        assert excluded.get(path) == "fixture", (
            f"{path} must appear in survey.excluded with reason='fixture';"
            f" got {excluded.get(path)!r}"
        )

    doc = survey.to_cyclonedx()
    origins = {
        prop.get("value")
        for component in doc.get("components", [])
        for prop in component.get("properties", [])
        if prop.get("name") == "fathomdb:declared-in"
    }
    leaked = sorted(o for o in origins if o and o.startswith(FIXTURE_PREFIX))
    assert not leaked, f"fixture manifests produced real components: {leaked}"


def test_fixture_exclusion_is_data_driven_not_hardcoded() -> None:
    """AC-SBOM-06.

    The exclusion must be a rule in the tracked `tiers.toml`, not an `if` in
    code. Proof: load a tier map with the fixture rule REMOVED and the fixture
    manifests must then flow through to tiering (and, having no rule, trip the
    REQ-4 loud failure). A hardcoded special case cannot satisfy this.

    Proving it at `TierMap.classify()` alone is NOT enough, and that was codex
    §9 round 2's second finding: Slice 32 could make classify() perfectly
    data-driven and still write `if path.startswith("dev/release/fixtures/")`
    into `run_survey()`, passing both AC-SBOM-05 and the classify-level
    assertions while REQ-5's "never by a hardcoded special case in code" went
    untested where it counts. Both tier maps are therefore driven THROUGH the
    survey boundary as well.
    """
    tiers_file = PROJECT_ROOT / "tiers.toml"
    tiers = require(
        "sbom_survey.tiers",
        "AC-SBOM-06",
        "the fixture exclusion must live as an `action = \"exclude\"` rule in the"
        " tracked scripts/sbom-survey/tiers.toml; removing that rule from a"
        " loaded TierMap must change the behaviour, proving the rule is data,"
        " not a hardcoded path check in code.",
    )

    assert tiers_file.is_file(), (
        f"{tiers_file} does not exist — the tier/exclusion rules must be tracked"
        " DATA (Slice 32 artifact), not code"
    )
    text = tiers_file.read_text(encoding="utf-8")
    assert FIXTURE_PREFIX in text, (
        f"{tiers_file} carries no rule for {FIXTURE_PREFIX!r}; the exclusion is"
        " not data-driven"
    )

    full = tiers.load_tier_map(tiers_file)
    assert full.classify(FIXTURE_PREFIX + "cargo-skew/Cargo.toml").action == "exclude"

    stripped = tiers.TierMap(
        [r for r in full.rules if not r.prefix.startswith(FIXTURE_PREFIX)]
    )
    with pytest.raises(tiers.UntieredManifestError):
        stripped.classify(FIXTURE_PREFIX + "cargo-skew/Cargo.toml")

    # --- and now at the SURVEY BOUNDARY, which is where REQ-5 actually bites -
    survey_mod = require(
        "sbom_survey.survey",
        "AC-SBOM-06",
        "run_survey(repo_root, *, published, tier_map=…) must take its exclusion"
        " decisions FROM the injected tier map: with the fixture rule present"
        " the fixture manifests are excluded; with that one rule removed they"
        " must reach tiering and raise UntieredManifestError naming a fixture"
        ' path. A hardcoded path.startswith("dev/release/fixtures/") inside'
        " run_survey() cannot satisfy both halves.",
    )
    registry = require(
        "sbom_survey.registry",
        "AC-SBOM-06",
        "an OfflineSource keeps the survey-boundary run hermetic.",
    )

    fixtures = _fixture_manifests()

    with_rule = survey_mod.run_survey(
        REPO_ROOT, published=registry.OfflineSource(), tier_map=full
    )
    still_excluded = {e.path for e in with_rule.excluded}
    missing = sorted(set(fixtures) - still_excluded)
    assert not missing, (
        "run_survey() did not honour the INJECTED tier map — these fixture"
        f" manifests were not excluded: {missing}"
    )

    with pytest.raises(tiers.UntieredManifestError) as excinfo:
        survey_mod.run_survey(
            REPO_ROOT, published=registry.OfflineSource(), tier_map=stripped
        )
    assert any(path in str(excinfo.value) for path in fixtures), (
        "run_survey() with the fixture rule REMOVED must fail loudly, naming a"
        " fixture manifest (REQ-4). If it excluded them anyway, the exclusion"
        " lives in CODE rather than in tiers.toml, which is exactly what REQ-5"
        f" forbids. Error was: {excinfo.value}"
    )


def test_untiered_manifest_raises_untiered_manifest_error() -> None:
    """AC-SBOM-07.

    A tracked manifest matched by NO rule is a hard error naming the path.
    There is deliberately no catch-all rule: a default would turn "somebody
    added a manifest and nobody classified it" — the event this tool exists to
    catch — into a silent mis-tag.
    """
    tiers = require(
        "sbom_survey.tiers",
        "AC-SBOM-07",
        "TierMap.classify() must raise UntieredManifestError NAMING THE PATH for"
        " a tracked manifest that matches no rule. It must never return a null"
        " tier, never fall back to a default tier, and never drop the manifest.",
    )

    tier_map = tiers.TierMap(
        [tiers.TierRule(prefix="src/rust/crates/", action="tier", tier="shipped")]
    )
    with pytest.raises(tiers.UntieredManifestError) as excinfo:
        tier_map.classify("some/brand/new/Cargo.toml")
--- scripts/sbom-survey/tiers.toml
schema = 1

[[rule]]
prefix = "dev/release/fixtures/"
action = "exclude"
reason = "fixture"
note   = "deliberately fake/skewed release-gate fixtures; not real dependencies"

[[rule]]
prefix = "src/rust/crates/"
action = "tier"
tier   = "shipped"

[[rule]]
prefix = "src/python/"
action = "tier"
tier   = "shipped"

[[rule]]
prefix = "src/ts/"
action = "tier"
tier   = "shipped"

[[rule]]
prefix = "Cargo."
action = "tier"
tier   = "shipped"
note   = "root workspace manifest + lockfile"

[[rule]]
prefix = "package"
action = "tier"
tier   = "dev-tooling"
note   = "root package.json/package-lock.json — name is literally fathomdb-dev-tooling, private, not published"

[[rule]]
prefix = "dev/tools/"
action = "tier"
tier   = "dev-tooling"

[[rule]]
prefix = "tools/docs/"
action = "tier"
tier   = "dev-tooling"

[[rule]]
prefix = "scripts/sbom-survey/"
action = "tier"
tier   = "dev-tooling"
note   = "this tool surveys itself"

exec
/bin/bash -lc "sed -n '1,200p' scripts/sbom-survey/pyproject.toml && grep -R \"def main\\|if __name__\\|entry\" -n scripts/sbom-survey/sbom_survey scripts/sbom-survey/pyproject.toml | head -50" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
# `sbom-survey` — the dependency-survey mini-project (0.8.20 Slice 32).
#
# ISOLATION (design §2). This project is deliberately standalone:
#   * NOT a member of the Cargo workspace,
#   * NOT referenced by `src/python/pyproject.toml`,
#   * NOT a dependency of the root `package.json`.
# It can therefore never enlarge the published dependency graph or the advisory
# backlog. Its own dependencies are surveyed BY the tool and tier `dev-tooling`
# — the tool appears in its own SBOM, which is the correct answer.
#
# TC-97 — THERE IS DELIBERATELY NO `[tool.pytest.ini_options]` TABLE HERE.
# pytest only treats a `pyproject.toml` as its *configfile* when that table is
# present. Adding one would make this file the configfile for
# `scripts/sbom-survey/tests` and start importing settings (the repo's only
# other pytest config, `src/python/pyproject.toml`, sets `pythonpath = ["."]`,
# which shadows an installed wheel). The suite must run with NO configfile: the
# pytest header prints `rootdir:` and no `configfile:` line. Do not add one.

[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[project]
name = "sbom-survey"
version = "0.1.0"
description = "CycloneDX 1.6 dependency survey over FathomDB's tracked manifests (Library Sweep #3)"
readme = "README.md"
requires-python = ">=3.11"
license = { text = "Apache-2.0" }

# Design §5.7 names every one of these and why stdlib is insufficient. The set
# is kept deliberately small: a dependency-hygiene tool with a bloated
# dependency set is self-refuting.
#
# Deliberately NOT taken (§5.7, binding):
#   * no HTTP client   — stdlib `urllib.request` covers three GET-JSON calls;
#   * no TOML library  — `tomllib` is stdlib from 3.11 (hence requires-python);
#   * no `jsonschema`  — the `json-validation` extra already binds a validator
#                        to the normative 1.6 schema; a second one would drift;
#   * no `GitPython`   — one `git ls-files -z` via `subprocess` is the whole
#                        git surface;
#   * no setup.py/AST tooling — §5.2.
dependencies = [
    "cyclonedx-python-lib[json-validation]>=8.0,<9.0",
    "packageurl-python>=0.15,<1.0",
    "packaging>=24.0,<26.0",
    "semver>=3.0,<4.0",
]

[project.optional-dependencies]
# Dev-only. `pytest` is NOT a runtime dependency of the tool.
dev = ["pytest>=8.0,<10.0"]

[project.scripts]
sbom-survey = "sbom_survey.cli:console_main"

[tool.setuptools]
packages = ["sbom_survey", "sbom_survey.parse"]
grep: scripts/sbom-survey/sbom_survey/parse/__pycache__/cargo.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/parse/__pycache__/npm.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/parse/__pycache__/python.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/survey.cpython-312.pyc: binary file matches
grep: scripts/sbom-survey/sbom_survey/__pycache__/__main__.cpython-312.pyc: binary file matches
scripts/sbom-survey/sbom_survey/survey.py:179:            "declared_in": [dict(entry) for entry in self.declared_in],
scripts/sbom-survey/sbom_survey/survey.py:360:        1. **No lock entry for the name** — unchanged: materialize an unresolved
scripts/sbom-survey/sbom_survey/survey.py:376:        entry carries no ambiguity to resolve. That reasoning is wrong whenever
scripts/sbom-survey/sbom_survey/survey.py:377:        the lone entry comes from *another project's* lockfile: this tool's own
scripts/sbom-survey/sbom_survey/cyclonedx.py:149:    # Every component gets a `dependencies` entry — a leaf takes an empty one.
scripts/sbom-survey/sbom_survey/tiers.py:188:    for entry in raw_rules:
scripts/sbom-survey/sbom_survey/tiers.py:189:        prefix = entry.get("prefix")
scripts/sbom-survey/sbom_survey/tiers.py:196:        action = entry.get("action")
scripts/sbom-survey/sbom_survey/tiers.py:203:        tier = entry.get("tier")
scripts/sbom-survey/sbom_survey/tiers.py:204:        reason = entry.get("reason")
scripts/sbom-survey/sbom_survey/tiers.py:228:                note=entry.get("note"),
scripts/sbom-survey/sbom_survey/parse/python.py:22:#: A lock entry whose source is one of these IS the project being locked, not a
scripts/sbom-survey/sbom_survey/parse/python.py:52:        canonicalize_name(entry["name"])
scripts/sbom-survey/sbom_survey/parse/python.py:53:        for entry in entries
scripts/sbom-survey/sbom_survey/parse/python.py:54:        if isinstance(entry, Mapping) and isinstance(entry.get("name"), str)
scripts/sbom-survey/sbom_survey/parse/python.py:58:    for entry in entries:
scripts/sbom-survey/sbom_survey/parse/python.py:59:        if not isinstance(entry, Mapping):
scripts/sbom-survey/sbom_survey/parse/python.py:61:        name = entry.get("name")
scripts/sbom-survey/sbom_survey/parse/python.py:62:        version = entry.get("version")
scripts/sbom-survey/sbom_survey/parse/python.py:65:        source = entry.get("source")
scripts/sbom-survey/sbom_survey/parse/python.py:72:                *_dep_names(entry.get("dependencies")),
scripts/sbom-survey/sbom_survey/parse/python.py:73:                *_dep_names(entry.get("optional-dependencies")),
scripts/sbom-survey/sbom_survey/parse/python.py:74:                *_dep_names(entry.get("dev-dependencies")),
scripts/sbom-survey/sbom_survey/parse/npm.py:28:def _name_of(key: str, entry: Mapping[str, Any]) -> str:
scripts/sbom-survey/sbom_survey/parse/npm.py:29:    name = entry.get("name")
scripts/sbom-survey/sbom_survey/parse/npm.py:42:    version, so `b` must resolve to the nested entry when reached through `a`
scripts/sbom-survey/sbom_survey/parse/npm.py:58:    """Every real entry in a `package-lock.json` v3 `packages` map."""
scripts/sbom-survey/sbom_survey/parse/npm.py:65:    for key, entry in packages.items():
scripts/sbom-survey/sbom_survey/parse/npm.py:66:        if key == "" or not isinstance(entry, Mapping):
scripts/sbom-survey/sbom_survey/parse/npm.py:68:        if entry.get("link") is True:
scripts/sbom-survey/sbom_survey/parse/npm.py:70:        version = entry.get("version")
scripts/sbom-survey/sbom_survey/parse/npm.py:73:        name = _name_of(key, entry)
scripts/sbom-survey/sbom_survey/parse/npm.py:77:            deps = entry.get(table)
scripts/sbom-survey/sbom_survey/parse/cargo.py:29:    A lock `dependencies` entry is either `"name"` or `"name version"` — cargo
scripts/sbom-survey/sbom_survey/parse/cargo.py:37:    for entry in entries:
scripts/sbom-survey/sbom_survey/parse/cargo.py:38:        name = entry.get("name")
scripts/sbom-survey/sbom_survey/parse/cargo.py:39:        version = entry.get("version")
scripts/sbom-survey/sbom_survey/parse/cargo.py:44:    for entry in entries:
scripts/sbom-survey/sbom_survey/parse/cargo.py:45:        name = entry.get("name")
scripts/sbom-survey/sbom_survey/parse/cargo.py:46:        version = entry.get("version")
scripts/sbom-survey/sbom_survey/parse/cargo.py:50:        for raw in entry.get("dependencies", []) or []:
scripts/sbom-survey/sbom_survey/cli.py:123:def main(argv: list[str] | None = None) -> int:
scrgrep: scripts/sbom-survey/sbom_survey/__pycache__/tiers.cpython-312.pyc: binary file matches
ipts/sbom-survey/sbom_survey/__main__.py:1:"""`python -m sbom_survey` — the entry point the acceptance suite drives."""
scripts/sbom-survey/sbom_survey/__main__.py:9:if __name__ == "__main__":

exec
/bin/bash -lc 'bash scripts/sbom-survey/smoke-install-run.sh' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 7076ms:
smoke: installing /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey into /tmp/tmp.qHHOUbDm8L/venv (needs PyPI) ...
Processing ./scripts/sbom-survey
  Installing build dependencies: started
  Installing build dependencies: finished with status 'done'
  Getting requirements to build wheel: started
  Getting requirements to build wheel: finished with status 'done'
  Preparing metadata (pyproject.toml): started
  Preparing metadata (pyproject.toml): finished with status 'done'
Collecting cyclonedx-python-lib<9.0,>=8.0 (from cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached cyclonedx_python_lib-8.9.0-py3-none-any.whl.metadata (6.5 kB)
Collecting packageurl-python<1.0,>=0.15 (from sbom-survey==0.1.0)
  Using cached packageurl_python-0.17.6-py3-none-any.whl.metadata (5.1 kB)
Collecting packaging<26.0,>=24.0 (from sbom-survey==0.1.0)
  Using cached packaging-25.0-py3-none-any.whl.metadata (3.3 kB)
Collecting semver<4.0,>=3.0 (from sbom-survey==0.1.0)
  Using cached semver-3.0.4-py3-none-any.whl.metadata (6.8 kB)
Collecting license-expression<31,>=30 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached license_expression-30.4.4-py3-none-any.whl.metadata (11 kB)
Collecting py-serializable<2.0.0,>=1.1.1 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached py_serializable-1.1.2-py3-none-any.whl.metadata (4.2 kB)
Collecting sortedcontainers<3.0.0,>=2.4.0 (from cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached sortedcontainers-2.4.0-py2.py3-none-any.whl.metadata (10 kB)
Collecting jsonschema<5.0,>=4.18 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonschema-4.26.0-py3-none-any.whl.metadata (7.6 kB)
Collecting attrs>=22.2.0 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached attrs-26.1.0-py3-none-any.whl.metadata (8.8 kB)
Collecting jsonschema-specifications>=2023.03.6 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonschema_specifications-2025.9.1-py3-none-any.whl.metadata (2.9 kB)
Collecting referencing>=0.28.4 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached referencing-0.37.0-py3-none-any.whl.metadata (2.8 kB)
Collecting rpds-py>=0.25.0 (from jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rpds_py-2026.6.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl.metadata (4.1 kB)
Collecting fqdn (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached fqdn-1.5.1-py3-none-any.whl.metadata (1.4 kB)
Collecting idna (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached idna-3.18-py3-none-any.whl.metadata (6.1 kB)
Collecting isoduration (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached isoduration-20.11.0-py3-none-any.whl.metadata (5.7 kB)
Collecting jsonpointer>1.13 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached jsonpointer-3.1.1-py3-none-any.whl.metadata (2.4 kB)
Collecting rfc3339-validator (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rfc3339_validator-0.1.4-py2.py3-none-any.whl.metadata (1.5 kB)
Collecting rfc3987 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached rfc3987-1.3.8-py2.py3-none-any.whl.metadata (7.5 kB)
Collecting uri-template (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached uri_template-1.3.0-py3-none-any.whl.metadata (8.8 kB)
Collecting webcolors>=1.11 (from jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached webcolors-25.10.0-py3-none-any.whl.metadata (2.2 kB)
Collecting boolean.py>=4.0 (from license-expression<31,>=30->cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached boolean_py-5.0-py3-none-any.whl.metadata (2.3 kB)
Collecting defusedxml<0.8.0,>=0.7.1 (from py-serializable<2.0.0,>=1.1.1->cyclonedx-python-lib<9.0,>=8.0->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached defusedxml-0.7.1-py2.py3-none-any.whl.metadata (32 kB)
Collecting typing-extensions>=4.4.0 (from referencing>=0.28.4->jsonschema<5.0,>=4.18->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached typing_extensions-4.16.0-py3-none-any.whl.metadata (3.3 kB)
Collecting arrow>=0.15.0 (from isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached arrow-1.4.0-py3-none-any.whl.metadata (7.7 kB)
Collecting six (from rfc3339-validator->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached six-1.17.0-py2.py3-none-any.whl.metadata (1.7 kB)
Collecting python-dateutil>=2.7.0 (from arrow>=0.15.0->isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached python_dateutil-2.9.0.post0-py2.py3-none-any.whl.metadata (8.4 kB)
Collecting tzdata (from arrow>=0.15.0->isoduration->jsonschema[format]<5.0,>=4.18; extra == "validation" or extra == "json-validation"->cyclonedx-python-lib[json-validation]<9.0,>=8.0->sbom-survey==0.1.0)
  Using cached tzdata-2026.3-py2.py3-none-any.whl.metadata (1.4 kB)
Using cached cyclonedx_python_lib-8.9.0-py3-none-any.whl (375 kB)
Using cached packageurl_python-0.17.6-py3-none-any.whl (36 kB)
Using cached packaging-25.0-py3-none-any.whl (66 kB)
Using cached semver-3.0.4-py3-none-any.whl (17 kB)
Using cached jsonschema-4.26.0-py3-none-any.whl (90 kB)
Using cached license_expression-30.4.4-py3-none-any.whl (120 kB)
Using cached py_serializable-1.1.2-py3-none-any.whl (22 kB)
Using cached sortedcontainers-2.4.0-py2.py3-none-any.whl (29 kB)
Using cached attrs-26.1.0-py3-none-any.whl (67 kB)
Using cached boolean_py-5.0-py3-none-any.whl (26 kB)
Using cached defusedxml-0.7.1-py2.py3-none-any.whl (25 kB)
Using cached jsonpointer-3.1.1-py3-none-any.whl (7.7 kB)
Using cached jsonschema_specifications-2025.9.1-py3-none-any.whl (18 kB)
Using cached referencing-0.37.0-py3-none-any.whl (26 kB)
Using cached rpds_py-2026.6.3-cp312-cp312-manylinux_2_17_x86_64.manylinux2014_x86_64.whl (366 kB)
Using cached webcolors-25.10.0-py3-none-any.whl (14 kB)
Using cached fqdn-1.5.1-py3-none-any.whl (9.1 kB)
Using cached idna-3.18-py3-none-any.whl (65 kB)
Using cached isoduration-20.11.0-py3-none-any.whl (11 kB)
Using cached rfc3339_validator-0.1.4-py2.py3-none-any.whl (3.5 kB)
Using cached rfc3987-1.3.8-py2.py3-none-any.whl (13 kB)
Using cached uri_template-1.3.0-py3-none-any.whl (11 kB)
Using cached arrow-1.4.0-py3-none-any.whl (68 kB)
Using cached typing_extensions-4.16.0-py3-none-any.whl (45 kB)
Using cached six-1.17.0-py2.py3-none-any.whl (11 kB)
Using cached python_dateutil-2.9.0.post0-py2.py3-none-any.whl (229 kB)
Using cached tzdata-2026.3-py2.py3-none-any.whl (348 kB)
Building wheels for collected packages: sbom-survey
  Building wheel for sbom-survey (pyproject.toml): started
  Building wheel for sbom-survey (pyproject.toml): finished with status 'done'
  Created wheel for sbom-survey: filename=sbom_survey-0.1.0-py3-none-any.whl size=46701 sha256=3579ff36a7e49bde544b899596a2e58d18528dca5022c92d9039f7ac5fcbf178
  Stored in directory: /home/coreyt/.cache/pip/wheels/a2/b8/35/86f9ad363fd09045aba9c835a340d81a3ade348eb64ebf4a03
Successfully built sbom-survey
Installing collected packages: sortedcontainers, rfc3987, boolean.py, webcolors, uri-template, tzdata, typing-extensions, six, semver, rpds-py, packaging, packageurl-python, license-expression, jsonpointer, idna, fqdn, defusedxml, attrs, rfc3339-validator, referencing, python-dateutil, py-serializable, jsonschema-specifications, cyclonedx-python-lib, arrow, jsonschema, isoduration, sbom-survey
Successfully installed arrow-1.4.0 attrs-26.1.0 boolean.py-5.0 cyclonedx-python-lib-8.9.0 defusedxml-0.7.1 fqdn-1.5.1 idna-3.18 isoduration-20.11.0 jsonpointer-3.1.1 jsonschema-4.26.0 jsonschema-specifications-2025.9.1 license-expression-30.4.4 packageurl-python-0.17.6 packaging-25.0 py-serializable-1.1.2 python-dateutil-2.9.0.post0 referencing-0.37.0 rfc3339-validator-0.1.4 rfc3987-1.3.8 rpds-py-2026.6.3 sbom-survey-0.1.0 semver-3.0.4 six-1.17.0 sortedcontainers-2.4.0 typing-extensions-4.16.0 tzdata-2026.3 uri-template-1.3.0 webcolors-25.10.0
smoke: console script present and executable: /tmp/tmp.qHHOUbDm8L/venv/bin/sbom-survey
smoke: provenance OK (RUN A) — installed sbom_survey resolves to /tmp/tmp.qHHOUbDm8L/venv/lib/python3.12/site-packages/sbom_survey/__init__.py
smoke: RUN A — installed console script
sbom-survey: 774 components (52 direct, 722 transitive), 774 unknown, 8 manifests excluded
  wrote /tmp/tmp.qHHOUbDm8L/out-installed/sbom.cdx.json
  wrote /tmp/tmp.qHHOUbDm8L/out-installed/staleness.json
  wrote /tmp/tmp.qHHOUbDm8L/out-installed/staleness.md
smoke: RUN A rc=0
Found existing installation: sbom-survey 0.1.0
Uninstalling sbom-survey-0.1.0:
  Successfully uninstalled sbom-survey-0.1.0
smoke: provenance OK (RUN B) — sbom_survey resolves to /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/sbom_survey/__init__.py
smoke: RUN B — source tree via python -m sbom_survey
sbom-survey: 774 components (52 direct, 722 transitive), 774 unknown, 8 manifests excluded
  wrote /tmp/tmp.qHHOUbDm8L/out-source/sbom.cdx.json
  wrote /tmp/tmp.qHHOUbDm8L/out-source/staleness.json
  wrote /tmp/tmp.qHHOUbDm8L/out-source/staleness.md
smoke: RUN B rc=0
smoke: artifact sets identical:
smoke:   sbom.cdx.json
smoke:   staleness.json
smoke:   staleness.md
smoke: byte-identical: sbom.cdx.json
smoke: byte-identical: staleness.json
smoke: byte-identical: staleness.md
smoke: PASS — installed run rc=0, source run rc=0, artifacts byte-identical over 774 components (sbom.cdx.json, staleness.json, staleness.md); both provenance guards (installed=site-packages, source=tree) and the vacuity guard held.

exec
/bin/bash -lc 'bash -n scripts/sbom-survey/smoke-install-run.sh && command -v shellcheck || true
git diff --check 29c2eae00ba12c2929e4cd138eefaf7e74db720c' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 exited 2 in 0ms:
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:64: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:66: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:68: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:135: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:137: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:139: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:165: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:167: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:169: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:220: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:222: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:227: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:232: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:234: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:242: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:248: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:256: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:260: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:262: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:267: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:275: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:277: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:299: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:321: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:324: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:329: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:335: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:337: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:342: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:347: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:349: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1662: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1719: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 2 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1743: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1747: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1755: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 28 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1809: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1876: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1910: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 2 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1916: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 4 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1937: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1984: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1997: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2013: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 226 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2028: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 27 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2038: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 130 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2064: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 2 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2078: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2159: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2201: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2241: trailing whitespace.
+�
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2285: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 17 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2286: trailing whitespace.
[SANITIZED TC-120: 1 line(s) / 27 bytes of binary __pycache__/*.pyc output removed - not review content]
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2970: trailing whitespace.
+    42	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2972: trailing whitespace.
+    44	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2977: trailing whitespace.
+    49	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2980: trailing whitespace.
+    52	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2996: trailing whitespace.
+    68	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:2999: trailing whitespace.
+    71	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3013: trailing whitespace.
+    85	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3024: trailing whitespace.
+    96	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3026: trailing whitespace.
+    98	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3036: trailing whitespace.
+   108	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3050: trailing whitespace.
+   122	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3063: trailing whitespace.
+   135	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3066: trailing whitespace.
+   138	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3078: trailing whitespace.
+   150	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3089: trailing whitespace.
+   161	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3114: trailing whitespace.
+   186	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3126: trailing whitespace.
+   198	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3144: trailing whitespace.
+   216	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3166: trailing whitespace.
+   238	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3175: trailing whitespace.
+   247	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3179: trailing whitespace.
+   251	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3183: trailing whitespace.
+   255	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3197: trailing whitespace.
+   269	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3210: trailing whitespace.
+   282	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3215: trailing whitespace.
+     2	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3220: trailing whitespace.
+     7	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3227: trailing whitespace.
+    14	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3229: trailing whitespace.
+    16	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3231: trailing whitespace.
+    18	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3245: trailing whitespace.
+    32	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3251: trailing whitespace.
+    38	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3255: trailing whitespace.
+    42	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3259: trailing whitespace.
+    46	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3263: trailing whitespace.
+    50	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3269: trailing whitespace.
+    56	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3273: trailing whitespace.
+    60	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3275: trailing whitespace.
+    62	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3277: trailing whitespace.
+    64	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3279: trailing whitespace.
+    66	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3285: trailing whitespace.
+    72	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3287: trailing whitespace.
+    74	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3293: trailing whitespace.
+    80	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3298: trailing whitespace.
+    85	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3300: trailing whitespace.
+    87	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3302: trailing whitespace.
+    89	
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:3309: trailing whitespace.
+    96	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:52: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:54: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:62: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:84: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2699: trailing whitespace.
+    42	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2701: trailing whitespace.
+    44	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2706: trailing whitespace.
+    49	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2709: trailing whitespace.
+    52	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2725: trailing whitespace.
+    68	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2728: trailing whitespace.
+    71	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2742: trailing whitespace.
+    85	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2753: trailing whitespace.
+    96	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2755: trailing whitespace.
+    98	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2765: trailing whitespace.
+   108	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2779: trailing whitespace.
+   122	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2792: trailing whitespace.
+   135	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2795: trailing whitespace.
+   138	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2807: trailing whitespace.
+   150	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2818: trailing whitespace.
+   161	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2843: trailing whitespace.
+   186	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2855: trailing whitespace.
+   198	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2873: trailing whitespace.
+   216	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2895: trailing whitespace.
+   238	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2904: trailing whitespace.
+   247	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2908: trailing whitespace.
+   251	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2912: trailing whitespace.
+   255	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2926: trailing whitespace.
+   269	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2939: trailing whitespace.
+   282	
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md:2951: trailing whitespace.
+normal -c sys.path[0] 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:91: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:93: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:95: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:162: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:164: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:166: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:192: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:194: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:196: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:247: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:249: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:254: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:259: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:261: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:269: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:275: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:283: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:287: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:289: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:294: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:302: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:304: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:326: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:350: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:353: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:358: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:364: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:366: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:371: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:376: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:378: trailing whitespace.
+ 
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:1177: trailing whitespace.
+   126	
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:1190: trailing whitespace.
+   139	
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:1193: trailing whitespace.
+   142	
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md:1217: trailing whitespace.
+   166	

exec
/bin/bash -lc "file dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md
python3 - <<'PY'
from pathlib import Path
for p in Path('dev/plans/runs/codex/0.8.20').glob('slice-33*.md'):
    data=p.read_bytes()
    print(p, len(data), 'nul', data.count(b'\\0'), 'ff', data.count(b'\\f'), 'nonutf?', end=' ')
    try: data.decode('utf-8'); print('utf8')
    except Exception as e: print(e)
PY
sed -n '1630,1670l' dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md | head -60" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:               unified diff output text, 1st line "OpenAI Codex v0.136.0", 2nd line "--------", 3rd line "workdir: /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run", Unicode text, UTF-8 text, with very long lines (392)
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md: unified diff output text, 1st line "OpenAI Codex v0.136.0", 2nd line "--------", 3rd line "workdir: /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run", Unicode text, UTF-8 text, with very long lines (371)
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md: unified diff output text, 1st line "OpenAI Codex v0.136.0", 2nd line "--------", 3rd line "workdir: /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run", Unicode text, UTF-8 text, with very long lines (392)
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md 222978 nul 15879 ff 250 nonutf? utf8
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md 144686 nul 0 ff 0 nonutf? utf8
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md 94019 nul 0 ff 0 nonutf? utf8
dev/plans/runs/codex/0.8.20/slice-33-round4-fix3-rereview-20260729T201456Z.md 260944 nul 227 ff 10 nonutf? utf8
    Used by Slice 33's online run ONLY \342\200\224 never by the acce\
ptance suite, which is$
    hermetic by construction. Any exception degrades to `None`, which\
 the survey$
    turns into `status="unknown"` with the reason recorded; it never \
degrades to$
    `current`.$
$
    No HTTP client library is taken for three GET-JSON endpoints (\
\302\2475.7): adding$
    an HTTP stack to a dependency-hygiene tool is exactly the bloat t\
his tool$
    exists to expose.$
    """$
$
    source_kind = "http"$
$
    _ENDPOINTS = {$
        "cargo": "https://crates.io/api/v1/crates/{name}",$
        "npm": "https://registry.npmjs.org/{name}",$
        "pypi": "https://pypi.org/pypi/{name}/json",$
    }$
$
    def __init__(self, timeout: float = 10.0, user_agent: str = "fath\
omdb-sbom-survey") -> None:$
        self.timeout = timeout$
scripts/sbom-survey/sbom_survey/__main__.py$
"""`python -m sbom_survey` \342\200\224 the entry point the acceptanc\
e suite drives."""$
$
from __future__ import annotations$
$
import sys$
$
from .cli import main$
$
if __name__ == "__main__":$
    sys.exit(main(sys.argv[1:]))$
scripts/sbom-survey/sbom_survey/__pycache__/survey.cpython-312.pyc$
\357\277\275\r\r$
\000\000\000\000\357\277\275Pjj\023s\000\000\357\277\275\000\000\000\
\000\000\000\000\000\000\000\000\000\006\000\000\000\000\000\000\001\
\357\277\275\002\000\000\357\277\275\000d\000Z\000d\001d\002l\001m\
\002Z\002\001\000d\001d\003l\003Z\003d\001d\004l\004m\005Z\005m\006Z\
\006\001\000d\001d\005l\am\aZ\am\bZ\b\001\000d\001d\006l\tm$
Z$
\001\000d\001d\al\vm\fZ\fm\rZ\rm\016Z\016\001\000d\001d\bl\017m\020Z\
\020\001\000d\td$
l\021m\022Z\022\001\000d\td\vl\023m\024Z\025\001\000d\td\fl\026m\027Z\
\027\001\000d\td\rl\030m\031Z\031m\032Z\032\001\000d\td\016l\033m\034\
Z\034m\035Z\035\001\000d\td\017l\036m\037Z\037m Z m!Z!\001\000d\td\
\020l\036m"Z#\001\000d\td\021l\036m$Z%\001\000d\td\022l\036m&Z'\001\
\000d\td\023l(m)Z)\001\000d\td\024l*m+Z+m,Z,\001\000d\td\025l-m.Z.m/Z\
/m0Z0\001\000g\000d\026\357\277\275\001Z1\002\000e2e\022\357\277\275\
\001\000\000\000\000\000\000D\000\357\277\275\000\357\277\275\001c\
\003i\000c\002]\b\000\000\\\002\000\000}\000}\001|\001|\000\357\277\
\275\002\357\277\275$
\004\000c\003}\001}\000Z3d/d\027\357\277\275\004Z4d0d\030\357\277\275\
\004Z5\002\000e\005d\031\357\277\275\032\357\277\275\001\000\000\000\
\000\000\000\002\000G\000d\033\357\277\275\000d\034\357\277\275\002\

exec
/bin/bash -lc 'git status --short && git ls-files dev/plans/runs/codex/0.8.20/slice-33-round4-fix3-rereview-20260729T201456Z.md || true' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
?? dev/plans/runs/codex/0.8.20/slice-33-round4-fix3-rereview-20260729T201456Z.md

exec
/bin/bash -lc ./scripts/agent-lint.sh in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 exited 1 in 12585ms:
FAIL lint-md-structure: markdownlint-cli2 not found at node_modules/.bin/markdownlint-cli2.
  A missing structural markdown linter must never report a silent pass (TC-37).
  Fix: run scripts/bootstrap.sh to install it, OR (inside a linked worktree)
  symlink the primary checkout's node_modules:
    ln -s /home/coreyt/projects/fathomdb/node_modules node_modules

exec
/bin/bash -lc "nl -ba dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md | sed -n '1648,1660p' | cat -v
nl -ba dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md | sed -n '1658,1665p' | cat -v" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
  1648	    def __init__(self, timeout: float = 10.0, user_agent: str = "fathomdb-sbom-survey") -> None:
  1649	        self.timeout = timeout
  1650	scripts/sbom-survey/sbom_survey/__main__.py
  1651	"""`python -m sbom_survey` M-bM-^@M-^T the entry point the acceptance suite drives."""
  1652	
  1653	from __future__ import annotations
  1654	
  1655	import sys
  1656	
  1657	from .cli import main
  1658	
  1659	if __name__ == "__main__":
  1660	    sys.exit(main(sys.argv[1:]))
  1658	
  1659	if __name__ == "__main__":
  1660	    sys.exit(main(sys.argv[1:]))
  1661	scripts/sbom-survey/sbom_survey/__pycache__/survey.cpython-312.pyc
  1662	M-oM-?M-=^M^M
  1663	^@^@^@^@M-oM-?M-=Pjj^Ss^@^@M-oM-?M-=^@^@^@^@^@^@^@^@^@^@^@^@^F^@^@^@^@^@^@^AM-oM-?M-=^B^@^@M-oM-?M-=^@d^@Z^@d^Ad^Bl^Am^BZ^B^A^@d^Ad^Cl^CZ^Cd^Ad^Dl^Dm^EZ^Em^FZ^F^A^@d^Ad^El^Gm^GZ^Gm^HZ^H^A^@d^Ad^Fl	m
  1664	Z
  1665	^A^@d^Ad^Gl^Km^LZ^Lm^MZ^Mm^NZ^N^A^@d^Ad^Hl^Om^PZ^P^A^@d	d

exec
/bin/bash -lc "grep -R \"markdownlint\\|dev/plans\" -n scripts/agent-lint.sh package.json .markdownlint* .markdownlint-cli2* 2>/dev/null" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
package.json:6:    "lint:md": "markdownlint-cli2",
package.json:9:  "comment-scripts": "0.8.9.1: prettier removed from the markdown path (lint:md-format / format:md). prettier's non-configurable *->_ emphasis reflow corrupts fragile markdown (see dev/tools/md-fix-corruption-ledger.md); markdownlint-cli2 --fix via the AST-guarded scripts/md-safe-fix.sh is the safe fixer. prettier kept as a devDependency only for any non-markdown use; remove if unused.",
package.json:11:    "markdownlint-cli2": "^0.23.0",
package.json:14:  "comment-overrides": "0.8.9 R-DEP-1: force transitive security patches for markdown-it (GHSA-6v5v-wf23-fmfq) and js-yaml (GHSA-h67p-54hq-rp68). markdownlint-cli2@0.22.1 pins the vulnerable 14.1.1/4.1.1 exactly and has no patched release yet; `npm audit fix --force` would downgrade markdownlint-cli2 (breaking). Both are backward-compatible minor bumps. Remove when markdownlint-cli2 ships patched deps.",
.markdownlint-cli2.jsonc:1:// markdownlint-cli2 globs and ignore patterns.
.markdownlint-cli2.jsonc:7:    "extends": ".markdownlint.jsonc"
.markdownlint-cli2.jsonc:45:    "dev/plans/runs/**",
.markdownlint-cli2.jsonc:49:    "dev/plans/prompts/**",
.markdownlint-cli2.jsonc:61:    // an agent, not published prose — markdownlint's structural rules (heading
.markdownlint.jsonc:1:// markdownlint config. https://github.com/DavidAnson/markdownlint/blob/main/doc/Rules.md
.markdownlint.jsonc:41:  // markdownlint (only prettier reformatted tables, and prettier is removed as a
.markdownlint-cli2.jsonc:1:// markdownlint-cli2 globs and ignore patterns.
.markdownlint-cli2.jsonc:7:    "extends": ".markdownlint.jsonc"
.markdownlint-cli2.jsonc:45:    "dev/plans/runs/**",
.markdownlint-cli2.jsonc:49:    "dev/plans/prompts/**",
.markdownlint-cli2.jsonc:61:    // an agent, not published prose — markdownlint's structural rules (heading

exec
/bin/bash -lc "sed -n '1,90p' .markdownlint-cli2.jsonc && sed -n '1,140p' scripts/agent-lint.sh" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
// markdownlint-cli2 globs and ignore patterns.
//
// Scope: all actively-maintained markdown. Only build outputs, vendored
// deps, tool caches, and the dev/archive historical record are excluded.
{
  "config": {
    "extends": ".markdownlint.jsonc"
  },
  "globs": ["**/*.md"],
  "ignores": [
    // Vendored / build outputs. `**/node_modules/**` catches node_modules at
    // ANY depth (root, src/ts/, typescript/, …) so a stray vendored tree can
    // never re-enter the lint scope.
    "**/node_modules/**",
    "node_modules/**",
    "src/ts/node_modules/**",
    "target/**",
    "site/**",
    "dist/**",
    "**/dist/**",
    ".venv/**",
    "**/.venv/**",
    ".cache/**",
    ".wake/**",

    // Gitignored eval corpus payloads (never committed; seeded locally for runs).
    "data/corpus-data/**",

    // repo-prune tool backups (staging dir for pruned files; not maintained prose).
    "scripts/repo-prune/backups/**",

    // Research substrate — bare URLs are intentional citations.
    "dev/tmp/**",
    "dev/tmp2/**",
    "dev/tmp3/**",

    // Memex-related scratch notes (gitignored, mirrors .gitignore dev/memex/).
    "dev/memex/**",

    // Archived / resolved artifacts — historical record, not actively maintained.
    "dev/archive/**",

    // Agent-emitted run logs + reviewer verdicts — verbatim subagent output;
    // not actively maintained, formatting drifts per agent.
    "dev/plans/runs/**",

    // Orchestrator/agent-emitted slice PROMPTS — same class as runs/** above
    // (verbatim emitted, formatting drifts per agent), not maintained prose.
    "dev/plans/prompts/**",

    // Research/experiment subtree — tool- and agent-emitted analysis reports and
    // generated artifact dumps (detector output, audit scorecards, adjudication
    // packs). Same class as runs/**: verbatim emitted, formatting drifts per agent,
    // not maintained prose. The authored design docs under dev/design/ stay linted.
    "dev/experiments/**",

    // Agent config + slash-command definitions. INTENTIONALLY IGNORED — keep it
    // that way (HITL 2026-07-25). NB the previous rationale here ("gitignored,
    // not part of repo") was FACTUALLY WRONG: `.claude/commands/*.md` IS tracked
    // and committed. The real reason is that these files are prompt text read by
    // an agent, not published prose — markdownlint's structural rules (heading
    // increments, fence languages, list indentation) police a shape that carries
    // no meaning here, and a lint failure would block a command edit for style.
    // Corrected so nobody "fixes" the ignore on the strength of a false premise.
    ".claude/**",

    // Public docs are gated by `mkdocs build --strict` already.
    "docs/**"
  ]
}
#!/usr/bin/env bash
# Lint all language surfaces. Pass-through diagnostics unparaphrased on failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Rust: clippy with -D warnings (treat warnings as errors)
run_capped lint-rust cargo clippy --workspace --all-targets --quiet -- -D warnings

# Rust: format check
run_capped lint-rustfmt cargo fmt --all --check

# Migration authoring policy
run_capped lint-migrations "$SCRIPT_DIR/agent-lint-migrations.sh"

# Python: ruff if available
ruff_bin=""
if [ -x .venv/bin/ruff ]; then
  ruff_bin=".venv/bin/ruff"
elif command -v ruff >/dev/null 2>&1; then
  ruff_bin="$(command -v ruff)"
fi

if [ -n "$ruff_bin" ]; then
  run_capped lint-python "$ruff_bin" check src/python
else
  skip_notice lint-python "ruff not installed"
fi

# TypeScript: ESLint not configured yet
skip_notice lint-ts "ESLint not configured"

# Workflows: actionlint is the canonical validator per feedback_workflow_validation
# (yaml.safe_load passes schema-invalid syntax GitHub silently rejects).
if command -v actionlint >/dev/null 2>&1; then
  run_capped lint-actions actionlint .github/workflows/*.yml
else
  skip_notice lint-actions "actionlint not installed (run scripts/bootstrap.sh)"
fi

# Markdown: structural + format + link integrity
"$SCRIPT_DIR/agent-lint-md.sh"

exec
/bin/bash -lc 'tmp=$(mktemp -d); (cd "$tmp" && bash /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/smoke-install-run.sh >/tmp/smokeabs.log && echo abs-ok); (cd "$tmp" && bash ../coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/smoke-install-run.sh >/tmp/smokerel.log 2>&1; echo rel-rc=$?; head -5 /tmp/smokerel.log); rm -rf "$tmp"' in /tmp
 succeeded in 6920ms:
abs-ok
rel-rc=127
bash: ../coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/smoke-install-run.sh: No such file or directory

exec
/bin/bash -lc 'tmp=$(mktemp -d); (cd "$tmp" && bash ../../home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/smoke-install-run.sh >/tmp/smokerel.log 2>&1; echo rel-rc=$?; head -10 /tmp/smokerel.log); rm -rf "$tmp"' in /tmp
 succeeded in 6898ms:
rel-rc=0
smoke: repo    = /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
smoke: project = /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey
smoke: scrubbed build/ and *.egg-info/ before install
smoke: work    = /tmp/tmp.oXfetTmA1g (verified outside the repo)
smoke: installing /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey into /tmp/tmp.oXfetTmA1g/venv (needs PyPI) ...
Processing /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey
  Installing build dependencies: started
  Installing build dependencies: finished with status 'done'
  Getting requirements to build wheel: started
  Getting requirements to build wheel: finished with status 'done'

exec
/bin/bash -lc "sed -n '1,220p' dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
# 0.8.20 Slice 33 — Library Sweep #3 findings (the `sbom-survey` run of record)

**Date:** 2026-07-29 · **Method:** `sbom-survey --repo . --online --out scripts/sbom-survey/out`, run from
the repo root of worktree `0.8.20-slice-33-sbom-run` at base `29c2eae0` · **Mode: ONLINE** (the first and
only online run) · **Cost: $0.** · **ASCERTAIN ONLY — this slice applied no dependency bump and edited no
manifest and no lockfile.**

This document is the tracked durable home ruled at
[`dev/design/0.8.20-slice-31-sbom-survey-tool.md`](../../design/0.8.20-slice-31-sbom-survey-tool.md) §5.6;
the generated reports under `scripts/sbom-survey/out/` are gitignored and are **not** a durable record.
It answers exactly two questions and stops: **(1)** what is actually stale across every tracked
Cargo / npm / Python manifest, and **(2)** would a **surgical ~1–5 SLOC** change *likely* land each
upgrade. **The output is an INPUT to 0.8.22. Nothing in 0.8.20 changes.**

---

## 1. Provenance

| Fact | Value |
|---|---|
| Repo commit surveyed | `29c2eae0` (== `origin/main` at run time) |
| Tool source last changed at | `b6b3ec8e` — *fix(0.8.20 Slice 32): fix-3 — one timestamp door* |
| Tool version | `sbom-survey 0.1.0` |
| Invocation | `sbom-survey --repo . --online --out scripts/sbom-survey/out` |
| Exit code | **0** |
| Report `source` field | `"http"` |
| Report `generated` field | `1980-01-01T00:00:00+00:00` — the ruled fixed epoch (REQ-13), **not** wall-clock |
| Components | **774** — 52 direct, 722 transitive |
| Manifests contributing components | 14 |
| Manifests excluded | **8** |
| Ecosystems | cargo 468 · npm 282 · pypi 24 |

**Install path.** The tool is not importable from a bare interpreter (TC-111). It was installed into a
throwaway venv **outside the repo tree** (`python3 -m venv` → `pip install '<repo>/scripts/sbom-survey[dev]'`),
and `scripts/sbom-survey/build/` and `*.egg-info/` were scrubbed before the verifying install so no stale
wheel could shadow the source (the trap that cost Slice 32 a verification cycle). The run of record was made
through the **installed console script**, not through `python -m`.

**Suite state at the run.** `python -m pytest scripts/sbom-survey/tests -q -rsE` from the repo root, under
the TC-111 recipe (install → `pip uninstall -y sbom-survey` → run against the source tree): **24 passed,
rc=0, 0 skipped.**

**Egress.** `HttpRegistrySource` issues plain `urllib.request` **GET**s against exactly three public
endpoints and sends no repository data (checkable at
`scripts/sbom-survey/sbom_survey/registry.py:74-89` — `Request(url, headers=…)` with no body):

- `https://crates.io/api/v1/crates/{name}`
- `https://registry.npmjs.org/{name}`
- `https://pypi.org/pypi/{name}/json`

**Excluded manifests (8) — all `dev/release/fixtures/**`, reason `fixture`:**
`cargo-skew/Cargo.toml`, `cargo-skew/mock-skew-api/Cargo.toml`, `cargo-skew/mock-skew-consumer-a/Cargo.toml`,
`cargo-skew/mock-skew-consumer-b/Cargo.toml`, `pip-skew/api-v1/setup.py`, `pip-skew/api-v2/setup.py`,
`pip-skew/probe-a/setup.py`, `pip-skew/probe-b/setup.py`. These are deliberately fake/skewed release-gate
fixtures; the exclusion is the data-driven rule in `scripts/sbom-survey/tiers.toml`, on disk since 0.8.11.1.

**The offline control.** An `--offline` run at the same commit exits 0 and returns **774 unknown / 0
resolved**, confirming that every resolved verdict below comes from the registries and none from a cached
or inferred default.

---

## 2. What is actually stale

### 2.1 Whole-BOM totals

| | current | outdated | ahead | unknown | total |
|---|---|---|---|---|---|
| **direct** | 14 | **28** | 0 | 10 | 52 |
| transitive | 415 | 303 | 1 | 3 | 722 |
| **all** | 429 | 331 | 1 | 13 | 774 |

**Lookup failures: 3** (rows carrying a non-null `lookup_error`) — **two cargo, one pypi**:

| row | `lookup_error` |
|---|---|
| `packaging` (direct, dev-tooling, pypi) | `constraint '<26.0,>=24.0' admits none of the 1 locked versions of 'packaging' (26.2)` |
| `fathomdb-napi 0.8.9` (transitive) | `HTTPError: HTTP Error 404: Not Found` |
| `fathomdb-py 0.8.9` (transitive) | `HTTPError: HTTP Error 404: Not Found` |

The two 404s are **correct and expected**: `fathomdb-napi` and `fathomdb-py` are this repo's own internal
binding crates and are not published to crates.io. Only **one** lookup failure is a genuine registry-side
gap, and it is a constraint/lock skew rather than a network error. **Registry availability was not a
limiting factor in this survey.**

The remaining **10** unknown rows carry **no** `lookup_error` at all — see §5 and **TC-117**.

### 2.2 The 52 direct rows in full

> ⚠ **How to read the `tier` column.** **`shipped` here means "declared in a shipped manifest", not
> "present in the shipped artifact".** The tier is assigned from the declaring manifest's **path prefix**
> (`tiers.toml`), and models nothing about optionality or feature-gating. `ort` is the clearest case: it is
> `optional = true` behind an `onnx-embedder` feature with `default = []`, so it is an opt-in, eval-only
> dependency — yet it tiers identically to `rusqlite`. A reader triaging by tier alone would over-prioritise
> it. (Also folded into **TC-119** so the qualifier survives outside this document.)

| ecosystem | name | tier | locked | latest | status | edit sites |
|---|---|---|---|---|---|---|
| cargo | `fathomdb` | shipped | 0.8.9 | 0.8.9 | **current** | 1 |
| cargo | `fathomdb-embedder` | shipped | 0.8.9 | 0.8.9 | **current** | 4 |
| cargo | `fathomdb-embedder-api` | shipped | 0.6.1 | 0.6.1 | **current** | 4 |
| cargo | `fathomdb-engine` | shipped | 0.8.9 | 0.8.9 | **current** | 3 |
| cargo | `fathomdb-query` | shipped | 0.8.9 | 0.8.9 | **current** | 1 |
| cargo | `fathomdb-schema` | shipped | 0.8.9 | 0.8.9 | **current** | 3 |
| cargo | `fs2` | shipped | 0.4.3 | 0.4.3 | **current** | 1 |
| cargo | `proptest` | shipped | 1.11.0 | 1.11.0 | **current** | 3 |
| cargo | `pyo3` | shipped | 0.29.0 | 0.29.0 | **current** | 1 |
| cargo | `sha2` | shipped | 0.11.0 | 0.11.0 | **current** | 3 |
| cargo | `tempfile` | shipped | 3.27.0 | 3.27.0 | **current** | 6 |
| cargo | `trybuild` | shipped | 1.0.118 | 1.0.118 | **current** | 1 |
| npm | `@mermaid-js/mermaid-cli` | dev-tooling | 11.16.0 | 11.16.0 | **current** | 1 |
| pypi | `mkdocs` | dev-tooling | 1.6.1 | 1.6.1 | **current** | 1 |
| cargo | `candle-core` | shipped | 0.10.2 | 0.11.0 | **outdated** | 1 |
| cargo | `candle-nn` | shipped | 0.10.2 | 0.11.0 | **outdated** | 1 |
| cargo | `candle-transformers` | shipped | 0.10.2 | 0.11.0 | **outdated** | 1 |
| cargo | `clap` | shipped | 4.6.1 | 4.6.4 | **outdated** | 1 |
| cargo | `dirs` | shipped | 5.0.1 | 6.0.0 | **outdated** | 1 |
| cargo | `httpmock` | shipped | 0.7.0 | 0.8.3 | **outdated** | 1 |
| cargo | `jsonschema` | shipped | 0.18.3 | 0.49.2 | **outdated** | 1 |
| cargo | `libc` | shipped | 0.2.186 | 0.2.189 | **outdated** | 1 |
| cargo | `napi` | shipped | 2.16.17 | 3.12.0 | **outdated** | 1 |
| cargo | `napi-build` | shipped | 2.3.2 | 2.4.0 | **outdated** | 1 |
| cargo | `napi-derive` | shipped | 2.16.13 | 3.6.1 | **outdated** | 1 |
| cargo | `rusqlite` | shipped | 0.31.0 | 0.40.1 | **outdated** | 3 |
| cargo | `serde` | shipped | 1.0.228 | 1.0.229 | **outdated** | 2 |
| cargo | `serde_json` | shipped | 1.0.149 | 1.0.151 | **outdated** | 5 |
| cargo | `sqlite-vec` | shipped | 0.1.7 | 0.1.9 | **outdated** | 2 |
| cargo | `thiserror` | shipped | 1.0.69 | 2.0.19 | **outdated** | 1 |
| cargo | `tokenizers` | shipped | 0.20.4 | 0.23.1 | **outdated** | 1 |
| cargo | `tokio` | shipped | 1.52.3 | 1.53.1 | **outdated** | 1 |
| cargo | `ureq` | shipped | 2.12.1 | 3.3.0 | **outdated** | 1 |
| npm | `@napi-rs/cli` | shipped | 2.18.4 | 3.8.0 | **outdated** | 1 |
| npm | `@types/node` | shipped | 26.1.0 | 26.1.2 | **outdated** | 1 |
| npm | `markdownlint-cli2` | dev-tooling | 0.23.0 | 0.23.2 | **outdated** | 1 |
| npm | `prettier` | dev-tooling | 3.9.4 | 3.9.6 | **outdated** | 1 |
| npm | `typescript` | shipped | 6.0.3 | 7.0.2 | **outdated** | 1 |
| pypi | `hypothesis` | shipped | 6.152.9 | 6.163.0 | **outdated** | 1 |
| pypi | `pyright` | shipped | 1.1.409 | 1.1.411 | **outdated** | 1 |
| pypi | `pytest` | shipped | 9.0.3 | 9.1.1 | **outdated** | 2 |
| pypi | `ruff` | shipped | 0.15.14 | 0.16.0 | **outdated** | 1 |
| cargo | `ort` | shipped | 2.0.0-rc.10 | — | **unknown** | 1 |
| pypi | `cyclonedx-python-lib` | dev-tooling | — | 11.11.0 | **unknown** | 1 |
| pypi | `maturin` | shipped | — | 1.14.1 | **unknown** | 1 |
| pypi | `networkx` | shipped | — | 3.6.1 | **unknown** | 1 |
| pypi | `numpy` | shipped | — | 2.5.1 | **unknown** | 1 |
| pypi | `packageurl-python` | dev-tooling | — | 0.17.6 | **unknown** | 1 |
| pypi | `packaging` | dev-tooling | — | 26.2 | **unknown** | 1 |
| pypi | `pyyaml` | shipped | — | 6.0.3 | **unknown** | 1 |
| pypi | `scipy` | shipped | — | 1.18.0 | **unknown** | 1 |
| pypi | `semver` | dev-tooling | — | 3.0.4 | **unknown** | 1 |

### 2.3 The decomposition that bounds the work — method, not just result

**28 direct dependencies are behind. Only 11 of them need a judgement.** The step that gets from 28 to 11
costs nothing and is worth stating as method, because it is the behaviour the three-slice tool investment
was supposed to produce: **reduce the work with data before spending tokens on judgement.**

The question is mechanical — *does the constraint we have ALREADY DECLARED admit `latest`?* It is computed
from `declared_in[].constraint` and `latest_version`, two fields REQ-14 put in the report precisely so
Slice 33 re-derives nothing, evaluated with each ecosystem's own comparator (semver for cargo/npm, PEP 440
for pypi). **0 of 28 constraints were unparseable.** Where the answer is *yes*, the upgrade needs **zero
SLOC of manifest change** — a re-lock lands it — and no changelog needs reading to say so.

| Bucket | n | What it means |
|---|---|---|
| **A — LOCKFILE-ONLY** | 13 | constraint already admits `latest`; **0 SLOC**, a re-lock lands it |
| **B — MANIFEST-EDIT-NEEDED, already owned by 0.8.22** | 4 | scheduled at `seq-151`; cited, not re-researched |
| **B — MANIFEST-EDIT-NEEDED, genuinely new** | 11 | the only rows where "surgical?" is a real question |

Without this step the survey would have been 28 changelog investigations. With it, 11 — and the 13 in
bucket A get a more useful answer (*"no manifest edit at all"*) than a per-item prose entry would have given.

#### Bucket A — LOCKFILE-ONLY (13). No manifest edit; a re-lock lands them.

`clap 4.6.1→4.6.4` · `libc 0.2.186→0.2.189` · `napi-build 2.3.2→2.4.0` · `serde 1.0.228→1.0.229` ·
`serde_json 1.0.149→1.0.151` · `tokio 1.52.3→1.53.1` (cargo) · `@types/node 26.1.0→26.1.2` ·
`markdownlint-cli2 0.23.0→0.23.2` · `prettier 3.9.4→3.9.6` (npm) · `hypothesis 6.152.9→6.163.0` ·
`pyright 1.1.409→1.1.411` · `pytest 9.0.3→9.1.1` · `ruff 0.15.14→0.16.0` (pypi).

⚠ *"Zero SLOC" is a statement about the manifest, not about risk.* `ruff 0.15→0.16` is a pre-1.0 minor
behind a `>=0.6` floor: the constraint admits it, but a lint-rule change can still turn the tree red. The
declared floors on the four pypi rows (`>=6`, `>=1.1.380`, `>=8`, `>=0.6`) are wide enough that the
lockfile, not the manifest, is the only thing pinning them.

#### Bucket B/owned — already scheduled at 0.8.22 (4). Not researched here.

`napi 2.16.17→3.12.0` · `napi-derive 2.16.13→3.6.1` · `rusqlite 0.31.0→0.40.1` (3 sites) ·
`sqlite-vec =0.1.7→0.1.9` (2 sites, an **exact** `=` pin). All four are owned by 0.8.22 per steward
`seq-151`; this survey confirms them stale and defers.

> ⚠ **Standing coupling warning, carried forward.** The `sqlite-vec` bump **must move together with**
> `src/rust/crates/fathomdb-engine/tests/tc76_vec0_long_metadata_delete.rs`, which asserts that the
> upstream `vec0` DELETE defect **still exists**. A *successful* upgrade turns that suite **red**. Do not
> land the bump without the test change in the same commit.

### 2.4 Transitive (722) — aggregate only

The LBS charter's own triage says a transitive bump is usually moot, so these are summarised rather than
enumerated. **No transitive row is actionable on its own**; each moves when its parent does, or on a re-lock.

| ecosystem | tier | current | outdated | ahead | unknown |
|---|---|---|---|---|---|
| cargo | shipped | 211 | 221 | 1 | 3 |
| npm | dev-tooling | 195 | 80 | — | — |
| npm | shipped | — | 1 | — | — |
| pypi | shipped | 9 | 1 | — | — |

Of the 303 outdated transitives, **73 are major-version jumps**; the rest are minor/patch drift that a
routine `cargo update` / `npm update` absorbs. The single **`ahead`** row is `smallvec 2.0.0-alpha.10`
against a published `1.15.2` — a *pre-release* is locked, which the comparator correctly reports as ahead
rather than as `current`; it is a lock-state observation, not an upgrade candidate.

The 3 unknown transitives are the two self-referential 404s above plus one further cargo row.

---

## 3. Surgical verdict — the 11 genuinely-new direct upgrades

**ASCERTAIN ONLY. Nothing here was attempted, built, or applied.** No manifest, lockfile or source file was
edited, and no trial upgrade was run — every verdict is from reading the repo's own call sites and upstream

exec
/bin/bash -lc 'grep -n "verdict" dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md | head; echo rc=${PIPESTATUS[0]}
grep -Iq . dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md; echo grepIq=$?' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
grep: dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md: binary file matches
rc=0
grepIq=1

exec
/bin/bash -lc "for f in dev/plans/runs/codex/0.8.20/slice-33-round{1,2,3}*.md; do printf '%s ' \""'$f"; grep -Iq . "$f"; echo $?; done' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md 1
dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md 0
dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md 0

codex
The functional smoke script passes locally, but the patch commits a Markdown run log containing binary pycache bytes. That degrades repository search/inspection for the recorded review artifact and should be cleaned before landing.

Review comment:

- [P2] Remove binary pycache dump from run log — /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1661-1663
  When this added Markdown log includes `__pycache__/survey.cpython-312.pyc`, it embeds NUL/control bytes in a tracked `.md` file; ordinary searches now treat the file as binary (`grep` reports only "binary file matches"), making the durable run record hard to inspect and maintain. Please sanitize the transcript or omit generated `__pycache__` output before committing the log.
The functional smoke script passes locally, but the patch commits a Markdown run log containing binary pycache bytes. That degrades repository search/inspection for the recorded review artifact and should be cleaned before landing.

Review comment:

- [P2] Remove binary pycache dump from run log — /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md:1661-1663
  When this added Markdown log includes `__pycache__/survey.cpython-312.pyc`, it embeds NUL/control bytes in a tracked `.md` file; ordinary searches now treat the file as binary (`grep` reports only "binary file matches"), making the durable run record hard to inspect and maintain. Please sanitize the transcript or omit generated `__pycache__` output before committing the log.
