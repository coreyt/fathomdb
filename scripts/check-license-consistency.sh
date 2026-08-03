#!/usr/bin/env bash
# check-license-consistency.sh — license type + license-SHIPPING gate
# (0.8.20 Slice 39, R-20-DOC; HITL license ruling seq-193).
#
# WHY THIS EXISTS
#   Across the entire 0.8.x line the repo-root LICENSE said MIT while every
#   publishable manifest said Apache-2.0, and NO published artifact carried a
#   license file at all — not one of the 7 crates, not the wheel, not either npm
#   package. Nothing noticed, because nothing looked. Per standing policy, a
#   guardrail that was slipped gets a mechanism, not a note: this is that
#   mechanism.
#
#   crates.io versions are IMMUTABLE. Publishing a crate whose manifest declares
#   the wrong license, or that carries no license text, is not fixable after the
#   fact — only supersedable. That is why this is a gate and not a lint.
#
# WHAT IT ENFORCES — two halves, both required.
#
#   (i) TYPE AGREEMENT. The repo-root LICENSE is AUTHORITATIVE. Its type is
#       DERIVED from its own first line (not hardcoded), so relicensing the
#       project moves this gate with it instead of against it. Every publishable
#       manifest's license field must equal that type:
#         - [workspace.package].license in the root Cargo.toml
#         - every publishable workspace member (inherited or literal)
#         - src/python/pyproject.toml [project].license
#         - src/ts/package.json .license
#         - src/ts/package-lock.json .packages[""].license   <- the ROOT entry only
#         - every src/ts/npm/*/package.json .license
#
#       ⚠ src/ts/package-lock.json: ONLY the `""` root-package entry is checked
#       and only it may ever be edited. Every other "license" in that file is a
#       DEPENDENCY's own recorded licence — a factual record (node_modules/
#       typescript really is Apache-2.0). A global find-and-replace there would
#       corrupt the lockfile into lying about third-party licences.
#
#  (ii) THE LICENSE TEXT ACTUALLY SHIPS, asserted against the PACKAGING TOOL'S
#       OWN FILE LIST — never against the config that is supposed to cause it.
#       Grading a packaging change by re-reading the change is the failure mode
#       this whole slice exists to correct.
#
# COVERAGE HONESTY — which of the ten published units are checked how:
#
#   | unit                              | how (default run)                    |
#   |-----------------------------------|--------------------------------------|
#   | 7 publishable crates              | REAL `cargo package --list`          |
#   | npm `fathomdb`                    | REAL `npm pack --dry-run --json`     |
#   | npm `fathomdb-<triple>` | REAL `npm pack --dry-run --json`     |
#   | the PyPI wheel                    | MECHANISM PROXY (see below)          |
#
#   The wheel is the ONE unit not covered by real packaging output by default,
#   because building it compiles the whole Rust tree (tens of seconds warm,
#   minutes cold) and this runs in the routine test sweep. The proxy is exact
#   about what it does check: src/python/LICENSE exists as a regular file and is
#   byte-identical to the root LICENSE, and pyproject declares a PEP-639
#   `license-files` glob that matches it. Pass `--with-wheel` (CI / release
#   preflight) to replace the proxy with a REAL `maturin build` + a read of the
#   built wheel's `.dist-info/licenses/` and METADATA. Measured 2026-07-29 with
#   `--with-wheel`: `fathomdb-0.8.9.dist-info/licenses/LICENSE`, byte-identical,
#   `Metadata-Version: 2.4`, `License-Expression: MIT`.
#
# TWO MEASURED FACTS THE PREDICATE ENCODES (both were surprises; both are the
# reason the assertions look the way they do):
#
#   * npm SILENTLY IGNORES A LICENSE SYMLINK. `src/ts/LICENSE -> ../../LICENSE`
#     produced a tarball with NO license entry at all (npm pack, npm 10.x).
#     A REAL COPY is auto-included even though `files` is `["dist", ...]`.
#     So the npm and Python legs require a REGULAR FILE and assert byte-equality
#     with the root LICENSE — the copy is the drift risk, byte-equality is the
#     mitigation.
#   * CARGO NEEDS NO COPY. `[workspace.package] license-file = "LICENSE"` +
#     `license-file.workspace = true` in each crate resolves the path relative
#     to the WORKSPACE ROOT, and cargo copies the file into each `.crate` as
#     `LICENSE`. One real file, seven artifacts, nothing to drift.
#
# VACUOUS-PASS GUARD (this repo's named failure class, TC-37): every "cannot
# determine" path exits 2, never 0. Missing LICENSE, unrecognised license type,
# absent cargo/npm/python3, a workspace member whose manifest will not parse —
# all are environment/gate failures reported LOUDLY. A gate that cannot see its
# subject and reports green is an active false assurance.
#
# Usage:
#   scripts/check-license-consistency.sh [--root <dir>] [--only <legs>]
#                                        [--skip-packaging] [--with-wheel]
#                                        [--help]
#
#   --root <dir>       tree to check (default: git toplevel). Exists so the
#                      fixture tests can point at throwaway COPIES; the real
#                      manifests are never written by a test.
#   --only <legs>      comma list from {cargo,python,npm}. Default: all three.
#                      An ecosystem is never skipped IMPLICITLY — a missing
#                      manifest in a selected leg is a failure, not a skip.
#   --skip-packaging   run half (i) only. For fixture arms that mutate
#                      declarations and do not need the packaging tools.
#   --with-wheel       replace the wheel MECHANISM PROXY with a real
#                      `maturin build` of src/python (slow; needs maturin).
#
# Exit codes: 0 = every selected assertion holds
#             1 = a real divergence (wrong type, missing/duplicated license
#                 text, or a packaging tool that did not list a license file)
#             2 = usage error, or the gate could not run (missing tool,
#                 unparseable manifest, undeterminable license type) — NEVER
#                 reported as a pass
#
# Wiring: 0.8.20 Slice 39 ships the SCRIPT and registers it in
# scripts/agent-test.sh. CI wiring (.github/) is Slice 40's territory this
# release and has been handed off to it explicitly.
set -euo pipefail

SELF="$(basename "${BASH_SOURCE[0]}")"

usage() {
  cat <<EOF
Usage: scripts/$SELF [--root <dir>] [--only cargo,python,npm]
                     [--skip-packaging] [--with-wheel]

Fails when the repo-root LICENSE's type disagrees with any publishable
manifest, or when a packaging tool's own file list does not carry the license
text. See the header of this script for the full predicate and for exactly
which of the ten published units are covered by real packaging output.

  --root <dir>      tree to check (default: git toplevel)
  --only <legs>     comma list from {cargo,python,npm} (default: all)
  --skip-packaging  declaration half only
  --with-wheel      really build the wheel instead of the mechanism proxy
  --help            show this text

Exit codes: 0 = holds; 1 = divergence; 2 = usage/environment error.
EOF
}

ROOT=""
ONLY="cargo,python,npm"
SKIP_PACKAGING=0
WITH_WHEEL=0

while [ $# -gt 0 ]; do
  case "$1" in
    --root)           ROOT="${2:?--root needs a path}"; shift 2 ;;
    --only)           ONLY="${2:?--only needs a comma list}"; shift 2 ;;
    --skip-packaging) SKIP_PACKAGING=1; shift ;;
    --with-wheel)     WITH_WHEEL=1; shift ;;
    -h|--help)        usage; exit 0 ;;
    *) printf '%s: unknown arg %q\n' "${SELF%.sh}" "$1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$ROOT" ]; then
  if ! ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    printf 'check-license-consistency: no --root given and not inside a git repo\n' >&2
    exit 2
  fi
fi

if ! command -v python3 >/dev/null 2>&1; then
  printf 'check-license-consistency: python3 is required to parse TOML/JSON manifests and is not on PATH — refusing to report a pass it did not verify\n' >&2
  exit 2
fi

set +e
python3 - "$ROOT" "$ONLY" "$SKIP_PACKAGING" "$WITH_WHEEL" <<'PY'
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import zipfile

ROOT, ONLY, SKIP_PACKAGING, WITH_WHEEL = sys.argv[1], sys.argv[2], sys.argv[3] == "1", sys.argv[4] == "1"

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - environment guard
    print(
        "check-license-consistency: python3 has no tomllib (needs >= 3.11) — "
        "refusing to report a pass it did not verify"
    )
    sys.exit(2)

legs = [x.strip() for x in ONLY.split(",") if x.strip()]
known_legs = {"cargo", "python", "npm"}
unknown = sorted(set(legs) - known_legs)
if unknown or not legs:
    print("check-license-consistency: --only accepts cargo,python,npm; got %r" % ONLY)
    sys.exit(2)

failures = []


def fail(msg):
    failures.append(msg)
    print("FAIL  license-consistency: " + msg)


def die_env(msg):
    print("check-license-consistency: " + msg)
    sys.exit(2)


def p(*parts):
    return os.path.join(ROOT, *parts)


def read_bytes(path):
    with open(path, "rb") as fh:
        return fh.read()


def load_toml(path):
    try:
        with open(path, "rb") as fh:
            return tomllib.load(fh)
    except OSError as exc:
        die_env("cannot read %s: %s" % (path, exc))
    except tomllib.TOMLDecodeError as exc:
        die_env("cannot parse %s: %s" % (path, exc))


def load_json(path):
    try:
        with open(path, "rb") as fh:
            return json.loads(fh.read().decode("utf-8"))
    except OSError as exc:
        die_env("cannot read %s: %s" % (path, exc))
    except ValueError as exc:
        die_env("cannot parse %s: %s" % (path, exc))


def run(cmd, cwd):
    try:
        return subprocess.run(
            cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
        )
    except OSError as exc:
        die_env("cannot run %s in %s: %s" % (" ".join(cmd), cwd, exc))


# --------------------------------------------------------- (A) the LICENSE ---
# The repo-root LICENSE is authoritative and its TYPE is derived from its own
# content. Hardcoding "MIT" here would make the gate a restatement of the
# ruling rather than a check of the tree, and would silently go stale on any
# future relicense.
LICENSE_PATH = p("LICENSE")
FIRST_LINE_TO_SPDX = {
    "MIT License": "MIT",
    "Apache License": "Apache-2.0",
    "BSD 3-Clause License": "BSD-3-Clause",
    "BSD 2-Clause License": "BSD-2-Clause",
    "ISC License": "ISC",
    "Mozilla Public License Version 2.0": "MPL-2.0",
}

if not os.path.isfile(LICENSE_PATH) or os.path.islink(LICENSE_PATH):
    die_env(
        "%s is missing or is not a regular file — the authoritative license text "
        "is the subject of this gate; without it there is nothing to check"
        % LICENSE_PATH
    )

LICENSE_BYTES = read_bytes(LICENSE_PATH)
if not LICENSE_BYTES.strip():
    die_env("%s is empty" % LICENSE_PATH)
LICENSE_SHA = hashlib.sha256(LICENSE_BYTES).hexdigest()

first_line = ""
for line in LICENSE_BYTES.decode("utf-8", "replace").splitlines():
    if line.strip():
        first_line = line.strip()
        break

SPDX = FIRST_LINE_TO_SPDX.get(first_line)
if SPDX is None:
    die_env(
        "cannot determine the license type of %s from its first line %r "
        "(known: %s) — refusing to report a pass it did not verify"
        % (LICENSE_PATH, first_line, ", ".join(sorted(FIRST_LINE_TO_SPDX)))
    )

print("license-consistency: root LICENSE = %s (sha256 %s)" % (SPDX, LICENSE_SHA[:16]))


def assert_same_bytes(path, label):
    """A shipped license COPY must stay byte-identical to the authoritative one.

    Copies are the drift risk that the symlink/`license-file` mechanisms avoid;
    where a copy is unavoidable (npm and PEP-639 both refuse to reach outside
    the package root) this is the mitigation.
    """
    if not os.path.exists(path):
        fail("%s: %s does not exist" % (label, path))
        return False
    if os.path.islink(path):
        fail(
            "%s: %s is a SYMLINK. npm's automatic license inclusion silently "
            "SKIPS symlinks (measured: npm pack emitted no license entry at "
            "all), so a symlink here ships nothing. Use a real copy."
            % (label, path)
        )
        return False
    if not os.path.isfile(path):
        fail("%s: %s is not a regular file" % (label, path))
        return False
    if read_bytes(path) != LICENSE_BYTES:
        fail(
            "%s: %s has DRIFTED from the authoritative %s (sha256 %s vs %s)"
            % (
                label,
                path,
                LICENSE_PATH,
                hashlib.sha256(read_bytes(path)).hexdigest()[:16],
                LICENSE_SHA[:16],
            )
        )
        return False
    return True


# ------------------------------------------------------------- cargo leg -----
publishable = []  # (name, crate_dir, license_file_basename) — packaging-checkable
publishable_declared = 0  # members without `publish = false`, pass or fail

if "cargo" in legs:
    ws_path = p("Cargo.toml")
    ws = load_toml(ws_path)
    wp = ws.get("workspace", {}).get("package", {})

    ws_license = wp.get("license")
    if ws_license != SPDX:
        fail(
            "Cargo.toml [workspace.package].license is %r, root LICENSE is %s"
            % (ws_license, SPDX)
        )

    ws_license_file = wp.get("license-file")
    if not ws_license_file:
        fail(
            "Cargo.toml [workspace.package] has no `license-file`. Without it no "
            "`.crate` carries the license text: `cargo package --list` shipped no "
            "license file for any crate through the whole 0.8.x line."
        )
    else:
        # Inherited license-file paths resolve relative to the WORKSPACE root.
        wlf = p(ws_license_file)
        if os.path.realpath(wlf) != os.path.realpath(LICENSE_PATH):
            assert_same_bytes(wlf, "Cargo.toml [workspace.package].license-file")

    members = ws.get("workspace", {}).get("members", [])
    if not members:
        die_env("%s declares no [workspace].members" % ws_path)

    for m in members:
        cdir = p(m)
        cpath = os.path.join(cdir, "Cargo.toml")
        if not os.path.isfile(cpath):
            die_env("workspace member %s has no Cargo.toml at %s" % (m, cpath))
        cm = load_toml(cpath)
        pkg = cm.get("package", {})
        name = pkg.get("name")
        if not name:
            die_env("%s declares no [package].name" % cpath)

        lic = pkg.get("license")
        if isinstance(lic, dict) and lic.get("workspace") is True:
            resolved = ws_license
        else:
            resolved = lic
        if resolved != SPDX:
            fail("%s: license resolves to %r, root LICENSE is %s" % (cpath, resolved, SPDX))

        if pkg.get("publish") is False:
            # Not shipped: type must still agree (checked above) but no artifact
            # exists to carry a license file.
            continue

        publishable_declared += 1

        lf = pkg.get("license-file")
        if isinstance(lf, dict) and lf.get("workspace") is True:
            if not ws_license_file:
                fail("%s: license-file.workspace = true but the workspace sets none" % cpath)
                continue
            basename = os.path.basename(ws_license_file)
        elif isinstance(lf, str):
            basename = os.path.basename(lf)
            assert_same_bytes(os.path.join(cdir, lf), "%s license-file" % cpath)
        else:
            fail(
                "%s is PUBLISHABLE but declares no `license-file`, so its .crate "
                "tarball will carry no license text. Add "
                "`license-file.workspace = true`." % cpath
            )
            continue

        publishable.append((name, cdir, basename))

    # Vacuous-pass guard: a workspace where EVERY member is `publish = false`
    # means the cargo leg had nothing to assert, which is an environment fault,
    # not a pass. This counts members DECLARED publishable, deliberately — if a
    # member is publishable but failed the checks above, that is a divergence
    # (exit 1) and must not be masked by an exit-2 "nothing to check".
    if publishable_declared == 0:
        die_env("no publishable workspace members found under %s" % ws_path)

# ------------------------------------------------------------ python leg -----
if "python" in legs:
    py_path = p("src", "python", "pyproject.toml")
    if not os.path.isfile(py_path):
        die_env("%s does not exist (leg `python` was selected)" % py_path)
    pyproj = load_toml(py_path)
    proj = pyproj.get("project", {})

    lic = proj.get("license")
    if isinstance(lic, dict):
        fail(
            "%s: [project].license is the legacy table form %r. Use the PEP-639 "
            "SPDX string (`license = \"%s\"`); the table form does not produce a "
            "License-Expression and pairs badly with `license-files`."
            % (py_path, lic, SPDX)
        )
    elif lic != SPDX:
        fail("%s: [project].license is %r, root LICENSE is %s" % (py_path, lic, SPDX))

    globs = proj.get("license-files")
    if not globs:
        fail(
            "%s: [project] has no `license-files`, so the wheel carries no license "
            "text in .dist-info/licenses/." % py_path
        )
    else:
        import glob as globmod

        matched = []
        for g in globs:
            if ".." in g.split("/"):
                fail(
                    "%s: license-files glob %r escapes the project root; PEP 639 "
                    "forbids `..`, which is why src/python/LICENSE is a real copy."
                    % (py_path, g)
                )
                continue
            hits = sorted(globmod.glob(os.path.join(p("src", "python"), g)))
            if not hits:
                fail("%s: license-files glob %r matched no file" % (py_path, g))
            matched.extend(hits)
        for hit in matched:
            assert_same_bytes(hit, "%s license-files" % py_path)

# --------------------------------------------------------------- npm leg -----
npm_dirs = []
if "npm" in legs:
    main_ts = p("src", "ts")
    if not os.path.isfile(os.path.join(main_ts, "package.json")):
        die_env("%s/package.json does not exist (leg `npm` was selected)" % main_ts)
    npm_dirs.append(main_ts)

    npm_root = os.path.join(main_ts, "npm")
    if os.path.isdir(npm_root):
        for entry in sorted(os.listdir(npm_root)):
            d = os.path.join(npm_root, entry)
            if os.path.isfile(os.path.join(d, "package.json")):
                npm_dirs.append(d)

    for d in npm_dirs:
        pj_path = os.path.join(d, "package.json")
        pj = load_json(pj_path)
        if pj.get("private") is True:
            continue
        if pj.get("license") != SPDX:
            fail("%s: .license is %r, root LICENSE is %s" % (pj_path, pj.get("license"), SPDX))
        assert_same_bytes(os.path.join(d, "LICENSE"), pj_path)

    # The lockfile's ROOT entry only. Every other "license" in it is a
    # dependency's own factual record and must never be rewritten.
    lock_path = os.path.join(main_ts, "package-lock.json")
    if os.path.isfile(lock_path):
        lock = load_json(lock_path)
        root_entry = lock.get("packages", {}).get("", {})
        if "license" in root_entry and root_entry["license"] != SPDX:
            fail(
                "%s: .packages[\"\"].license (the ROOT package entry, NOT a "
                "dependency) is %r, root LICENSE is %s"
                % (lock_path, root_entry["license"], SPDX)
            )

# ------------------------------------------- (ii) THE PACKAGED ARTEFACTS -----
if not SKIP_PACKAGING:
    if "cargo" in legs:
        if subprocess.run(["sh", "-c", "command -v cargo >/dev/null 2>&1"]).returncode != 0:
            die_env("cargo is not on PATH — cannot read any .crate file list")
        for name, cdir, basename in publishable:
            res = run(
                ["cargo", "package", "--list", "--allow-dirty", "-p", name], cwd=ROOT
            )
            if res.returncode != 0:
                fail(
                    "cargo package --list -p %s exited %d; its file list could not "
                    "be read, so the license file in it is UNVERIFIED.\n%s"
                    % (name, res.returncode, res.stderr.strip()[:2000])
                )
                continue
            listed = res.stdout.splitlines()
            if basename not in listed:
                fail(
                    "crate %s: `cargo package --list` does NOT contain %r — the "
                    "published .crate would carry no license text. Listed %d files."
                    % (name, basename, len(listed))
                )
            else:
                print("  ok  crate %s ships %s" % (name, basename))

    if "npm" in legs:
        if subprocess.run(["sh", "-c", "command -v npm >/dev/null 2>&1"]).returncode != 0:
            die_env("npm is not on PATH — cannot read any npm tarball file list")
        for d in npm_dirs:
            pj = load_json(os.path.join(d, "package.json"))
            if pj.get("private") is True:
                continue
            res = run(["npm", "pack", "--dry-run", "--json"], cwd=d)
            if res.returncode != 0:
                fail(
                    "npm pack --dry-run in %s exited %d; its file list could not be "
                    "read, so the license file in it is UNVERIFIED.\n%s"
                    % (d, res.returncode, res.stderr.strip()[:2000])
                )
                continue
            try:
                packs = json.loads(res.stdout)
            except ValueError as exc:
                fail("npm pack --dry-run --json in %s emitted unparseable JSON: %s" % (d, exc))
                continue
            paths = [f["path"] for pack in packs for f in pack.get("files", [])]
            if "LICENSE" not in paths:
                fail(
                    "npm package %s: `npm pack --dry-run` file list does NOT contain "
                    "LICENSE (%d files listed) — the published tarball would carry "
                    "no license text." % (pj.get("name", d), len(paths))
                )
            else:
                print("  ok  npm %s ships LICENSE" % pj.get("name", d))

    if "python" in legs:
        if not WITH_WHEEL:
            # MECHANISM PROXY, stated plainly. The declaration half already
            # asserted src/python/LICENSE is a real, byte-identical copy and that
            # `license-files` matches it; that is the mechanism maturin uses. The
            # REAL wheel is read only under --with-wheel because building it
            # compiles the whole Rust tree.
            print(
                "  ~   wheel: MECHANISM PROXY only (src/python/LICENSE + "
                "license-files). Run with --with-wheel to read a real wheel."
            )
        else:
            if subprocess.run(
                ["sh", "-c", "command -v maturin >/dev/null 2>&1"]
            ).returncode != 0:
                die_env("--with-wheel was requested but maturin is not on PATH")
            with tempfile.TemporaryDirectory() as td:
                res = run(["maturin", "build", "--out", td], cwd=p("src", "python"))
                if res.returncode != 0:
                    fail(
                        "maturin build exited %d; the wheel's license content is "
                        "UNVERIFIED.\n%s" % (res.returncode, res.stderr.strip()[:2000])
                    )
                else:
                    wheels = [w for w in sorted(os.listdir(td)) if w.endswith(".whl")]
                    if not wheels:
                        fail("maturin build produced no .whl in %s" % td)
                    for w in wheels:
                        with zipfile.ZipFile(os.path.join(td, w)) as z:
                            names = z.namelist()
                            lics = [n for n in names if "/licenses/" in n]
                            if not lics:
                                fail(
                                    "wheel %s carries no .dist-info/licenses/ entry "
                                    "(%d entries)" % (w, len(names))
                                )
                            for n in lics:
                                if z.read(n) != LICENSE_BYTES:
                                    fail(
                                        "wheel %s: %s differs from the root LICENSE"
                                        % (w, n)
                                    )
                            meta = [n for n in names if n.endswith(".dist-info/METADATA")]
                            if not meta:
                                fail("wheel %s has no METADATA" % w)
                            for n in meta:
                                text = z.read(n).decode("utf-8", "replace")
                                decl = [
                                    ln
                                    for ln in text.splitlines()
                                    if ln.startswith(("License:", "License-Expression:"))
                                ]
                                if not any(ln.endswith(" " + SPDX) for ln in decl):
                                    fail(
                                        "wheel %s METADATA declares %r, root LICENSE "
                                        "is %s" % (w, decl, SPDX)
                                    )
                            if not failures:
                                print("  ok  wheel %s ships %s" % (w, ", ".join(lics)))

if failures:
    print("")
    print("check-license-consistency: %d failure(s)" % len(failures))
    sys.exit(1)

print("check-license-consistency: OK (%s)" % SPDX)
sys.exit(0)
PY
rc=$?
set -e
exit "$rc"
