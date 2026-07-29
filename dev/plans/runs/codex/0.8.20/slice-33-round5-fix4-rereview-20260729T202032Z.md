OpenAI Codex v0.136.0
--------
workdir: /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
model: gpt-5.5
provider: openai
approval: never
sandbox: danger-full-access
reasoning effort: medium
reasoning summaries: none
session id: 019faf89-333a-7800-aad4-419bf47943ad
--------
user
changes against '29c2eae0'
exec
/bin/bash -lc "ls && cat MEMORY.md && for f in feedback_*.md; do echo '---' "'$f; cat "$f"; done' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 exited 1 in 0ms:
AGENTS.md
CHANGELOG.md
Cargo.lock
Cargo.toml
LICENSE
README.md
deny.toml
dev
docs
experiments
lychee.toml
mkdocs.yml
package-lock.json
package.json
rust-toolchain.toml
rustfmt.toml
scripts
src
target
test
tests
tools
cat: MEMORY.md: No such file or directory

exec
/bin/bash -lc "find .. -name MEMORY.md -o -name 'feedback_*.md' | sed 's#"'^#/#'"' | head -50 && git status --short && git diff --stat 29c2eae00ba12c2929e4cd138eefaf7e74db720c" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
?? dev/plans/runs/codex/0.8.20/slice-33-round5-fix4-rereview-20260729T202032Z.md
 dev/DOC-INDEX.md                                   |    2 +
 .../0.8.20-slice-33-library-sweep-3-FINDINGS.md    |  587 ++
 .../0.8.20/slice-33-round1-20260729T193509Z.md     | 3069 ++++++++++
 ...ice-33-round2-fix1-rereview-20260729T193712Z.md | 3067 ++++++++++
 ...ice-33-round3-fix2-rereview-20260729T200429Z.md | 1307 ++++
 ...ice-33-round4-fix3-rereview-20260729T201456Z.md | 6394 ++++++++++++++++++++
 scripts/sbom-survey/README.md                      |   22 +
 scripts/sbom-survey/smoke-install-run.sh           |  385 ++
 8 files changed, 14833 insertions(+)

exec
/bin/bash -lc 'git diff --name-status 29c2eae00ba12c2929e4cd138eefaf7e74db720c && git diff -- scripts/sbom-survey/smoke-install-run.sh scripts/sbom-survey/README.md dev/DOC-INDEX.md 29c2eae00ba12c2929e4cd138eefaf7e74db720c' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
M	dev/DOC-INDEX.md
A	dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md
A	dev/plans/runs/codex/0.8.20/slice-33-round1-20260729T193509Z.md
A	dev/plans/runs/codex/0.8.20/slice-33-round2-fix1-rereview-20260729T193712Z.md
A	dev/plans/runs/codex/0.8.20/slice-33-round3-fix2-rereview-20260729T200429Z.md
A	dev/plans/runs/codex/0.8.20/slice-33-round4-fix3-rereview-20260729T201456Z.md
M	scripts/sbom-survey/README.md
A	scripts/sbom-survey/smoke-install-run.sh

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
/bin/bash -lc "find scripts/sbom-survey -maxdepth 3 -type f -print | sort && sed -n '1,220p' scripts/sbom-survey/pyproject.toml && find scripts/sbom-survey/sbom_survey -maxdepth 2 -type f -print -exec sed -n '1,80p' {} \\;" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
scripts/sbom-survey/.pytest_cache/.gitignore
scripts/sbom-survey/.pytest_cache/CACHEDIR.TAG
scripts/sbom-survey/.pytest_cache/README.md
scripts/sbom-survey/README.md
scripts/sbom-survey/out/sbom.cdx.json
scripts/sbom-survey/out/staleness.json
scripts/sbom-survey/out/staleness.md
scripts/sbom-survey/pyproject.toml
scripts/sbom-survey/sbom_survey/__init__.py
scripts/sbom-survey/sbom_survey/__main__.py
scripts/sbom-survey/sbom_survey/cli.py
scripts/sbom-survey/sbom_survey/constraints.py
scripts/sbom-survey/sbom_survey/cyclonedx.py
scripts/sbom-survey/sbom_survey/discovery.py
scripts/sbom-survey/sbom_survey/parse/__init__.py
scripts/sbom-survey/sbom_survey/parse/cargo.py
scripts/sbom-survey/sbom_survey/parse/npm.py
scripts/sbom-survey/sbom_survey/parse/python.py
scripts/sbom-survey/sbom_survey/paths.py
scripts/sbom-survey/sbom_survey/registry.py
scripts/sbom-survey/sbom_survey/report.py
scripts/sbom-survey/sbom_survey/survey.py
scripts/sbom-survey/sbom_survey/tiers.py
scripts/sbom-survey/sbom_survey/util.py
scripts/sbom-survey/smoke-install-run.sh
scripts/sbom-survey/tests/__pycache__/conftest.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_cli.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_cyclonedx.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_discovery.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_paths.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_registry.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_report.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/__pycache__/test_tiering.cpython-312-pytest-9.1.1.pyc
scripts/sbom-survey/tests/conftest.py
scripts/sbom-survey/tests/test_cli.py
scripts/sbom-survey/tests/test_cyclonedx.py
scripts/sbom-survey/tests/test_discovery.py
scripts/sbom-survey/tests/test_paths.py
scripts/sbom-survey/tests/test_registry.py
scripts/sbom-survey/tests/test_report.py
scripts/sbom-survey/tests/test_tiering.py
scripts/sbom-survey/tiers.toml
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
scripts/sbom-survey/sbom_survey/paths.py
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
scripts/sbom-survey/sbom_survey/survey.py
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
scripts/sbom-survey/sbom_survey/cyclonedx.py
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
scripts/sbom-survey/sbom_survey/__init__.py
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
scripts/sbom-survey/sbom_survey/tiers.py
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
scripts/sbom-survey/sbom_survey/constraints.py
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
scripts/sbom-survey/sbom_survey/parse/__init__.py
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
scripts/sbom-survey/sbom_survey/parse/python.py
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
scripts/sbom-survey/sbom_survey/parse/npm.py
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
scripts/sbom-survey/sbom_survey/parse/cargo.py
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
scripts/sbom-survey/sbom_survey/cli.py
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
scripts/sbom-survey/sbom_survey/registry.py
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
scripts/sbom-survey/sbom_survey/__main__.py
"""`python -m sbom_survey` — the entry point the acceptance suite drives."""

from __future__ import annotations

import sys

from .cli import main

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
scripts/sbom-survey/sbom_survey/discovery.py
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
scripts/sbom-survey/sbom_survey/report.py
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
scripts/sbom-survey/sbom_survey/util.py
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

exec
/bin/bash -lc 'bash scripts/sbom-survey/smoke-install-run.sh' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 7130ms:
smoke: installing /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey into /tmp/tmp.J9X5Ekb1Li/venv (needs PyPI) ...
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
  Created wheel for sbom-survey: filename=sbom_survey-0.1.0-py3-none-any.whl size=46701 sha256=ac4f9bb61ada69d578eb28414ac1b3396009be07abcda71cd3c3430fd5b374ec
  Stored in directory: /home/coreyt/.cache/pip/wheels/a2/b8/35/86f9ad363fd09045aba9c835a340d81a3ade348eb64ebf4a03
Successfully built sbom-survey
Installing collected packages: sortedcontainers, rfc3987, boolean.py, webcolors, uri-template, tzdata, typing-extensions, six, semver, rpds-py, packaging, packageurl-python, license-expression, jsonpointer, idna, fqdn, defusedxml, attrs, rfc3339-validator, referencing, python-dateutil, py-serializable, jsonschema-specifications, cyclonedx-python-lib, arrow, jsonschema, isoduration, sbom-survey
Successfully installed arrow-1.4.0 attrs-26.1.0 boolean.py-5.0 cyclonedx-python-lib-8.9.0 defusedxml-0.7.1 fqdn-1.5.1 idna-3.18 isoduration-20.11.0 jsonpointer-3.1.1 jsonschema-4.26.0 jsonschema-specifications-2025.9.1 license-expression-30.4.4 packageurl-python-0.17.6 packaging-25.0 py-serializable-1.1.2 python-dateutil-2.9.0.post0 referencing-0.37.0 rfc3339-validator-0.1.4 rfc3987-1.3.8 rpds-py-2026.6.3 sbom-survey-0.1.0 semver-3.0.4 six-1.17.0 sortedcontainers-2.4.0 typing-extensions-4.16.0 tzdata-2026.3 uri-template-1.3.0 webcolors-25.10.0
smoke: console script present and executable: /tmp/tmp.J9X5Ekb1Li/venv/bin/sbom-survey
smoke: provenance OK (RUN A) — installed sbom_survey resolves to /tmp/tmp.J9X5Ekb1Li/venv/lib/python3.12/site-packages/sbom_survey/__init__.py
smoke: RUN A — installed console script
sbom-survey: 774 components (52 direct, 722 transitive), 774 unknown, 8 manifests excluded
  wrote /tmp/tmp.J9X5Ekb1Li/out-installed/sbom.cdx.json
  wrote /tmp/tmp.J9X5Ekb1Li/out-installed/staleness.json
  wrote /tmp/tmp.J9X5Ekb1Li/out-installed/staleness.md
smoke: RUN A rc=0
Found existing installation: sbom-survey 0.1.0
Uninstalling sbom-survey-0.1.0:
  Successfully uninstalled sbom-survey-0.1.0
smoke: provenance OK (RUN B) — sbom_survey resolves to /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run/scripts/sbom-survey/sbom_survey/__init__.py
smoke: RUN B — source tree via python -m sbom_survey
sbom-survey: 774 components (52 direct, 722 transitive), 774 unknown, 8 manifests excluded
  wrote /tmp/tmp.J9X5Ekb1Li/out-source/sbom.cdx.json
  wrote /tmp/tmp.J9X5Ekb1Li/out-source/staleness.json
  wrote /tmp/tmp.J9X5Ekb1Li/out-source/staleness.md
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
/bin/bash -lc 'shellcheck scripts/sbom-survey/smoke-install-run.sh || true' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
/bin/bash: line 1: shellcheck: command not found

exec
/bin/bash -lc "git diff 29c2eae00ba12c2929e4cd138eefaf7e74db720c -- dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md | sed -n '1,220p'" in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
diff --git a/dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md b/dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md
new file mode 100644
index 00000000..6ed06663
--- /dev/null
+++ b/dev/plans/runs/0.8.20-slice-33-library-sweep-3-FINDINGS.md
@@ -0,0 +1,587 @@
+# 0.8.20 Slice 33 — Library Sweep #3 findings (the `sbom-survey` run of record)
+
+**Date:** 2026-07-29 · **Method:** `sbom-survey --repo . --online --out scripts/sbom-survey/out`, run from
+the repo root of worktree `0.8.20-slice-33-sbom-run` at base `29c2eae0` · **Mode: ONLINE** (the first and
+only online run) · **Cost: $0.** · **ASCERTAIN ONLY — this slice applied no dependency bump and edited no
+manifest and no lockfile.**
+
+This document is the tracked durable home ruled at
+[`dev/design/0.8.20-slice-31-sbom-survey-tool.md`](../../design/0.8.20-slice-31-sbom-survey-tool.md) §5.6;
+the generated reports under `scripts/sbom-survey/out/` are gitignored and are **not** a durable record.
+It answers exactly two questions and stops: **(1)** what is actually stale across every tracked
+Cargo / npm / Python manifest, and **(2)** would a **surgical ~1–5 SLOC** change *likely* land each
+upgrade. **The output is an INPUT to 0.8.22. Nothing in 0.8.20 changes.**
+
+---
+
+## 1. Provenance
+
+| Fact | Value |
+|---|---|
+| Repo commit surveyed | `29c2eae0` (== `origin/main` at run time) |
+| Tool source last changed at | `b6b3ec8e` — *fix(0.8.20 Slice 32): fix-3 — one timestamp door* |
+| Tool version | `sbom-survey 0.1.0` |
+| Invocation | `sbom-survey --repo . --online --out scripts/sbom-survey/out` |
+| Exit code | **0** |
+| Report `source` field | `"http"` |
+| Report `generated` field | `1980-01-01T00:00:00+00:00` — the ruled fixed epoch (REQ-13), **not** wall-clock |
+| Components | **774** — 52 direct, 722 transitive |
+| Manifests contributing components | 14 |
+| Manifests excluded | **8** |
+| Ecosystems | cargo 468 · npm 282 · pypi 24 |
+
+**Install path.** The tool is not importable from a bare interpreter (TC-111). It was installed into a
+throwaway venv **outside the repo tree** (`python3 -m venv` → `pip install '<repo>/scripts/sbom-survey[dev]'`),
+and `scripts/sbom-survey/build/` and `*.egg-info/` were scrubbed before the verifying install so no stale
+wheel could shadow the source (the trap that cost Slice 32 a verification cycle). The run of record was made
+through the **installed console script**, not through `python -m`.
+
+**Suite state at the run.** `python -m pytest scripts/sbom-survey/tests -q -rsE` from the repo root, under
+the TC-111 recipe (install → `pip uninstall -y sbom-survey` → run against the source tree): **24 passed,
+rc=0, 0 skipped.**
+
+**Egress.** `HttpRegistrySource` issues plain `urllib.request` **GET**s against exactly three public
+endpoints and sends no repository data (checkable at
+`scripts/sbom-survey/sbom_survey/registry.py:74-89` — `Request(url, headers=…)` with no body):
+
+- `https://crates.io/api/v1/crates/{name}`
+- `https://registry.npmjs.org/{name}`
+- `https://pypi.org/pypi/{name}/json`
+
+**Excluded manifests (8) — all `dev/release/fixtures/**`, reason `fixture`:**
+`cargo-skew/Cargo.toml`, `cargo-skew/mock-skew-api/Cargo.toml`, `cargo-skew/mock-skew-consumer-a/Cargo.toml`,
+`cargo-skew/mock-skew-consumer-b/Cargo.toml`, `pip-skew/api-v1/setup.py`, `pip-skew/api-v2/setup.py`,
+`pip-skew/probe-a/setup.py`, `pip-skew/probe-b/setup.py`. These are deliberately fake/skewed release-gate
+fixtures; the exclusion is the data-driven rule in `scripts/sbom-survey/tiers.toml`, on disk since 0.8.11.1.
+
+**The offline control.** An `--offline` run at the same commit exits 0 and returns **774 unknown / 0
+resolved**, confirming that every resolved verdict below comes from the registries and none from a cached
+or inferred default.
+
+---
+
+## 2. What is actually stale
+
+### 2.1 Whole-BOM totals
+
+| | current | outdated | ahead | unknown | total |
+|---|---|---|---|---|---|
+| **direct** | 14 | **28** | 0 | 10 | 52 |
+| transitive | 415 | 303 | 1 | 3 | 722 |
+| **all** | 429 | 331 | 1 | 13 | 774 |
+
+**Lookup failures: 3** (rows carrying a non-null `lookup_error`) — **two cargo, one pypi**:
+
+| row | `lookup_error` |
+|---|---|
+| `packaging` (direct, dev-tooling, pypi) | `constraint '<26.0,>=24.0' admits none of the 1 locked versions of 'packaging' (26.2)` |
+| `fathomdb-napi 0.8.9` (transitive) | `HTTPError: HTTP Error 404: Not Found` |
+| `fathomdb-py 0.8.9` (transitive) | `HTTPError: HTTP Error 404: Not Found` |
+
+The two 404s are **correct and expected**: `fathomdb-napi` and `fathomdb-py` are this repo's own internal
+binding crates and are not published to crates.io. Only **one** lookup failure is a genuine registry-side
+gap, and it is a constraint/lock skew rather than a network error. **Registry availability was not a
+limiting factor in this survey.**
+
+The remaining **10** unknown rows carry **no** `lookup_error` at all — see §5 and **TC-117**.
+
+### 2.2 The 52 direct rows in full
+
+> ⚠ **How to read the `tier` column.** **`shipped` here means "declared in a shipped manifest", not
+> "present in the shipped artifact".** The tier is assigned from the declaring manifest's **path prefix**
+> (`tiers.toml`), and models nothing about optionality or feature-gating. `ort` is the clearest case: it is
+> `optional = true` behind an `onnx-embedder` feature with `default = []`, so it is an opt-in, eval-only
+> dependency — yet it tiers identically to `rusqlite`. A reader triaging by tier alone would over-prioritise
+> it. (Also folded into **TC-119** so the qualifier survives outside this document.)
+
+| ecosystem | name | tier | locked | latest | status | edit sites |
+|---|---|---|---|---|---|---|
+| cargo | `fathomdb` | shipped | 0.8.9 | 0.8.9 | **current** | 1 |
+| cargo | `fathomdb-embedder` | shipped | 0.8.9 | 0.8.9 | **current** | 4 |
+| cargo | `fathomdb-embedder-api` | shipped | 0.6.1 | 0.6.1 | **current** | 4 |
+| cargo | `fathomdb-engine` | shipped | 0.8.9 | 0.8.9 | **current** | 3 |
+| cargo | `fathomdb-query` | shipped | 0.8.9 | 0.8.9 | **current** | 1 |
+| cargo | `fathomdb-schema` | shipped | 0.8.9 | 0.8.9 | **current** | 3 |
+| cargo | `fs2` | shipped | 0.4.3 | 0.4.3 | **current** | 1 |
+| cargo | `proptest` | shipped | 1.11.0 | 1.11.0 | **current** | 3 |
+| cargo | `pyo3` | shipped | 0.29.0 | 0.29.0 | **current** | 1 |
+| cargo | `sha2` | shipped | 0.11.0 | 0.11.0 | **current** | 3 |
+| cargo | `tempfile` | shipped | 3.27.0 | 3.27.0 | **current** | 6 |
+| cargo | `trybuild` | shipped | 1.0.118 | 1.0.118 | **current** | 1 |
+| npm | `@mermaid-js/mermaid-cli` | dev-tooling | 11.16.0 | 11.16.0 | **current** | 1 |
+| pypi | `mkdocs` | dev-tooling | 1.6.1 | 1.6.1 | **current** | 1 |
+| cargo | `candle-core` | shipped | 0.10.2 | 0.11.0 | **outdated** | 1 |
+| cargo | `candle-nn` | shipped | 0.10.2 | 0.11.0 | **outdated** | 1 |
+| cargo | `candle-transformers` | shipped | 0.10.2 | 0.11.0 | **outdated** | 1 |
+| cargo | `clap` | shipped | 4.6.1 | 4.6.4 | **outdated** | 1 |
+| cargo | `dirs` | shipped | 5.0.1 | 6.0.0 | **outdated** | 1 |
+| cargo | `httpmock` | shipped | 0.7.0 | 0.8.3 | **outdated** | 1 |
+| cargo | `jsonschema` | shipped | 0.18.3 | 0.49.2 | **outdated** | 1 |
+| cargo | `libc` | shipped | 0.2.186 | 0.2.189 | **outdated** | 1 |
+| cargo | `napi` | shipped | 2.16.17 | 3.12.0 | **outdated** | 1 |
+| cargo | `napi-build` | shipped | 2.3.2 | 2.4.0 | **outdated** | 1 |
+| cargo | `napi-derive` | shipped | 2.16.13 | 3.6.1 | **outdated** | 1 |
+| cargo | `rusqlite` | shipped | 0.31.0 | 0.40.1 | **outdated** | 3 |
+| cargo | `serde` | shipped | 1.0.228 | 1.0.229 | **outdated** | 2 |
+| cargo | `serde_json` | shipped | 1.0.149 | 1.0.151 | **outdated** | 5 |
+| cargo | `sqlite-vec` | shipped | 0.1.7 | 0.1.9 | **outdated** | 2 |
+| cargo | `thiserror` | shipped | 1.0.69 | 2.0.19 | **outdated** | 1 |
+| cargo | `tokenizers` | shipped | 0.20.4 | 0.23.1 | **outdated** | 1 |
+| cargo | `tokio` | shipped | 1.52.3 | 1.53.1 | **outdated** | 1 |
+| cargo | `ureq` | shipped | 2.12.1 | 3.3.0 | **outdated** | 1 |
+| npm | `@napi-rs/cli` | shipped | 2.18.4 | 3.8.0 | **outdated** | 1 |
+| npm | `@types/node` | shipped | 26.1.0 | 26.1.2 | **outdated** | 1 |
+| npm | `markdownlint-cli2` | dev-tooling | 0.23.0 | 0.23.2 | **outdated** | 1 |
+| npm | `prettier` | dev-tooling | 3.9.4 | 3.9.6 | **outdated** | 1 |
+| npm | `typescript` | shipped | 6.0.3 | 7.0.2 | **outdated** | 1 |
+| pypi | `hypothesis` | shipped | 6.152.9 | 6.163.0 | **outdated** | 1 |
+| pypi | `pyright` | shipped | 1.1.409 | 1.1.411 | **outdated** | 1 |
+| pypi | `pytest` | shipped | 9.0.3 | 9.1.1 | **outdated** | 2 |
+| pypi | `ruff` | shipped | 0.15.14 | 0.16.0 | **outdated** | 1 |
+| cargo | `ort` | shipped | 2.0.0-rc.10 | — | **unknown** | 1 |
+| pypi | `cyclonedx-python-lib` | dev-tooling | — | 11.11.0 | **unknown** | 1 |
+| pypi | `maturin` | shipped | — | 1.14.1 | **unknown** | 1 |
+| pypi | `networkx` | shipped | — | 3.6.1 | **unknown** | 1 |
+| pypi | `numpy` | shipped | — | 2.5.1 | **unknown** | 1 |
+| pypi | `packageurl-python` | dev-tooling | — | 0.17.6 | **unknown** | 1 |
+| pypi | `packaging` | dev-tooling | — | 26.2 | **unknown** | 1 |
+| pypi | `pyyaml` | shipped | — | 6.0.3 | **unknown** | 1 |
+| pypi | `scipy` | shipped | — | 1.18.0 | **unknown** | 1 |
+| pypi | `semver` | dev-tooling | — | 3.0.4 | **unknown** | 1 |
+
+### 2.3 The decomposition that bounds the work — method, not just result
+
+**28 direct dependencies are behind. Only 11 of them need a judgement.** The step that gets from 28 to 11
+costs nothing and is worth stating as method, because it is the behaviour the three-slice tool investment
+was supposed to produce: **reduce the work with data before spending tokens on judgement.**
+
+The question is mechanical — *does the constraint we have ALREADY DECLARED admit `latest`?* It is computed
+from `declared_in[].constraint` and `latest_version`, two fields REQ-14 put in the report precisely so
+Slice 33 re-derives nothing, evaluated with each ecosystem's own comparator (semver for cargo/npm, PEP 440
+for pypi). **0 of 28 constraints were unparseable.** Where the answer is *yes*, the upgrade needs **zero
+SLOC of manifest change** — a re-lock lands it — and no changelog needs reading to say so.
+
+| Bucket | n | What it means |
+|---|---|---|
+| **A — LOCKFILE-ONLY** | 13 | constraint already admits `latest`; **0 SLOC**, a re-lock lands it |
+| **B — MANIFEST-EDIT-NEEDED, already owned by 0.8.22** | 4 | scheduled at `seq-151`; cited, not re-researched |
+| **B — MANIFEST-EDIT-NEEDED, genuinely new** | 11 | the only rows where "surgical?" is a real question |
+
+Without this step the survey would have been 28 changelog investigations. With it, 11 — and the 13 in
+bucket A get a more useful answer (*"no manifest edit at all"*) than a per-item prose entry would have given.
+
+#### Bucket A — LOCKFILE-ONLY (13). No manifest edit; a re-lock lands them.
+
+`clap 4.6.1→4.6.4` · `libc 0.2.186→0.2.189` · `napi-build 2.3.2→2.4.0` · `serde 1.0.228→1.0.229` ·
+`serde_json 1.0.149→1.0.151` · `tokio 1.52.3→1.53.1` (cargo) · `@types/node 26.1.0→26.1.2` ·
+`markdownlint-cli2 0.23.0→0.23.2` · `prettier 3.9.4→3.9.6` (npm) · `hypothesis 6.152.9→6.163.0` ·
+`pyright 1.1.409→1.1.411` · `pytest 9.0.3→9.1.1` · `ruff 0.15.14→0.16.0` (pypi).
+
+⚠ *"Zero SLOC" is a statement about the manifest, not about risk.* `ruff 0.15→0.16` is a pre-1.0 minor
+behind a `>=0.6` floor: the constraint admits it, but a lint-rule change can still turn the tree red. The
+declared floors on the four pypi rows (`>=6`, `>=1.1.380`, `>=8`, `>=0.6`) are wide enough that the
+lockfile, not the manifest, is the only thing pinning them.
+
+#### Bucket B/owned — already scheduled at 0.8.22 (4). Not researched here.
+
+`napi 2.16.17→3.12.0` · `napi-derive 2.16.13→3.6.1` · `rusqlite 0.31.0→0.40.1` (3 sites) ·
+`sqlite-vec =0.1.7→0.1.9` (2 sites, an **exact** `=` pin). All four are owned by 0.8.22 per steward
+`seq-151`; this survey confirms them stale and defers.
+
+> ⚠ **Standing coupling warning, carried forward.** The `sqlite-vec` bump **must move together with**
+> `src/rust/crates/fathomdb-engine/tests/tc76_vec0_long_metadata_delete.rs`, which asserts that the
+> upstream `vec0` DELETE defect **still exists**. A *successful* upgrade turns that suite **red**. Do not
+> land the bump without the test change in the same commit.
+
+### 2.4 Transitive (722) — aggregate only
+
+The LBS charter's own triage says a transitive bump is usually moot, so these are summarised rather than
+enumerated. **No transitive row is actionable on its own**; each moves when its parent does, or on a re-lock.
+
+| ecosystem | tier | current | outdated | ahead | unknown |
+|---|---|---|---|---|---|
+| cargo | shipped | 211 | 221 | 1 | 3 |
+| npm | dev-tooling | 195 | 80 | — | — |
+| npm | shipped | — | 1 | — | — |
+| pypi | shipped | 9 | 1 | — | — |
+
+Of the 303 outdated transitives, **73 are major-version jumps**; the rest are minor/patch drift that a
+routine `cargo update` / `npm update` absorbs. The single **`ahead`** row is `smallvec 2.0.0-alpha.10`
+against a published `1.15.2` — a *pre-release* is locked, which the comparator correctly reports as ahead
+rather than as `current`; it is a lock-state observation, not an upgrade candidate.
+
+The 3 unknown transitives are the two self-referential 404s above plus one further cargo row.
+

exec
/bin/bash -lc 'git status --short' in /home/coreyt/projects/fathomdb-worktrees/slice-33-sbom-run
 succeeded in 0ms:
?? dev/plans/runs/codex/0.8.20/slice-33-round5-fix4-rereview-20260729T202032Z.md

codex
The added smoke script and documentation appear consistent with the intended install-then-run verification flow, and the script passes in this checkout. I did not identify any actionable defects introduced by the patch.
The added smoke script and documentation appear consistent with the intended install-then-run verification flow, and the script passes in this checkout. I did not identify any actionable defects introduced by the patch.
