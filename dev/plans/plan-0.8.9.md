# FathomDB 0.8.9 — Plan (state-machine ladder) · **CI integrity micro (OUT-OF-BAND)**

> **Plan-as-state-machine.** Mod-5 slice ladder + reserved-gap policy + "Immediate Next Slice".
> Authoritative contracts → `0.8.9-implementation.md`; live state → `runs/STATUS-0.8.9.md`;
> deps/decision record → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (§3 OOB, §5 Q3). Run via
> `/goal complete 0.8.9` as an **orchestrator** session (`prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`).
>
> **OUT-OF-BAND (odd micro).** Two small, self-contained CI-integrity fixes with no feature coupling,
> run **in parallel** to the even main line. Done OOB and early so the boards stop lying (vacuous-green
> gates) **without** drawing experiment tokens. **$0** — CI/test-harness work only.
>
> **Footprint.** CI/CD. No change to the library query path; no priced runs.

---

## 1. Goal & scope

- **#12 — Un-mask the perf gates.** `ac_012 / ac_013 / ac_013b / ac_019 / ac_020` are `AGENT_LONG`-gated,
  so they **never run in per-push CI** — they are vacuously green. Worse, `ac_013b` asserts the **0.90
  recall floor against a ~0.73 synthetic embedder** (the real 0.937/0.896 is eu7, report-only). Fix the
  gate *semantics*: either run them where they can run, or **honestly re-scope + relabel** so the board
  no longer implies a floor CI never checks. The deliverable is an honest gate map, not a fabricated
  pass. (`perf-recall-gates-masked-and-ac013b-conflation`.)
- **#14 — AC-037 CI wiring + AC-050c cleanup.** Durably wire the AC-037 `netns-deny-egress` agent-
  security gate on a **userns-permissive runner** (`ubuntu-22.04`) — it was machine-confirmed GREEN
  **once** on windchill3 (2026-06-02) but is not durably in per-push CI. Separately, fix the pre-existing
  **AC-050c removal-detect** baseline failure (a standalone cleanup, not a regression).
- **Dependency-vulnerability hygiene (Dependabot backlog).** Resolve the remaining open GitHub Dependabot
  alerts **after 0.8.8 takes pyo3 (the 2 HIGH)**: **npm** `markdown-it` + `js-yaml` (moderate,
  quadratic-complexity DoS; transitive/tooling), and **pip** `idna` (moderate) + `torch` (low) in the
  eval env (`python/uv.lock`). Bump the lockfiles, re-run the affected suites, and **reconcile
  `.github/dependabot.yml` directory coverage** — the root `package-lock.json` and `python/uv.lock` are
  not under any configured version-update directory today, so their security PRs never auto-open.
  Footprint: npm = CI/tooling; `torch`/`idna` = **EVAL-ONLY** (not in the shipped library query path) —
  the low-sev `torch.jit.script` issue may be dismissed-with-rationale rather than chased. **No
  auto-merge** (`allow_auto_merge=false` confirmed; no auto-merge workflow) — every bump rides the normal
  TDD/gate discipline like any slice.
- **Bootstrap un-mask (CI PREREQUISITE — found by the steward's diagnosis 2026-06-27).** `scripts/
  bootstrap.sh`'s "Installing Python dev tooling" step exits 1 on **all** branches incl. `main`, so the
  `verify`/`security` jobs **abort before any gate runs** — the AC-037 catch, the recall predicate, and
  the perf gates never execute in CI (vacuously skipped, not green). Root cause (HIGH confidence): two
  **unguarded `httpx` imports** (`eval/graph_arm_recall.py:36`, `eval/p0a_batch_e2e.py:287`; httpx is not
  in the `[dev]` extras) fail `pyright` in a clean CI venv — and `pyright … >/dev/null` + pip `--quiet`
  **mask** the diagnostics, so the failure is invisible. This is the 0.8.9 CI-integrity mandate applied to
  bootstrap itself. **Not** the pyo3 0.29 work (the Rust build is healthy). Fix: **(A)** add `# type:
  ignore[import-not-found]` to the two imports (matching the `gold_gen.py`/`elps_live_harness.py` siblings;
  zero new deps, zero `.venv` impact) + **(C)** drop `>/dev/null` (line 22) and `--quiet` (lines 19–20) so
  a future masked failure surfaces in seconds.

*Why OOB / why paired:* both are mechanism-only CI hygiene with zero feature coupling and no upstream/
downstream code deps; they belong off the experiment critical path. Pairing them is purely batching —
one orchestrated micro-release that makes the gate surface honest.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-PG-1 | Honest gate map: which gates run per-push vs AGENT_LONG vs report-only | A documented table (`design/perf-gates.md` updated) of every ac_012/013/013b/019/020 gate, where it runs, and what embedder/corpus it asserts against |
| R-PG-2 | No gate asserts a floor it cannot honestly check | `ac_013b`'s synthetic-embedder assertion is re-scoped/relabeled (report-only or moved to the real eu7 path); a RED test proves the old vacuous-green is gone |
| R-PG-3 | Per-push CI runs the gates that *can* run cheaply | The subset runnable without AGENT_LONG runs per-push and can fail; AGENT_LONG-only gates are labeled as such on the board |
| R-037-1 | AC-037 `netns-deny-egress` runs durably in CI | `ci.yml` `security` job on `ubuntu-22.04` (userns-permissive) runs the no-egress proof; it can fail (RED proof) |
| R-037-2 | The gate is wired, not asserted-by-memory | A deliberately-egressing fixture trips the gate in CI (demonstrate the catch) |
| R-050c-1 | AC-050c removal-detect baseline failure cleared | `ac_050c` passes on a clean baseline; the cause is documented |
| R-DEP-1 | Remaining Dependabot alerts resolved (post-0.8.8 pyo3) | npm (`markdown-it`/`js-yaml`) + pip (`idna`/`torch`) lockfiles bumped off the open advisories; affected suites GREEN; `gh api .../dependabot/alerts` shows the npm/pip set closed (or low-sev `torch` dismissed-with-rationale) |
| R-DEP-2 | `dependabot.yml` covers the manifests that actually carry alerts | a version-update directory is added for the root `package-lock.json` and for `python/uv.lock` (the alert manifests today's `/src/ts` + `/src/python` directories miss); after the fix, no manifest with an open alert is left without coverage |
| R-DEP-3 | No mechanical auto-merge of security/version PRs | `allow_auto_merge=false` confirmed; no auto-merge workflow present; bumps land via gated slices only |
| R-BOOT-1 | `bootstrap.sh` dev-tooling step is green on a CLEAN `[dev]` venv | a fresh `[dev]`-only venv runs the python step to exit 0; `pyright -p src/python` passes (the two `httpx` imports no longer error); `verify`/`security` reach `agent-security.sh` |
| R-BOOT-2 | bootstrap no longer MASKS failures (demonstrate the catch) | `>/dev/null` (line 22) + `--quiet` (19–20) removed; a deliberately-broken import fails `bootstrap.sh` **visibly** in CI logs (RED proof), per `conformance-rewrite-vacuous-green-trap` |

New ACs: none expected (these *fix* existing gates); any new gate id is minted at Slice 0 only if HITL
elects, per the locked-acceptance policy.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + audit — board; **map the current gate reality** (which of ac_012/013/013b/019/020 run where, asserting against which embedder/corpus); design the honest re-scope + the AC-037 CI-wiring approach; confirm the post-0.8.8 Dependabot backlog | design-adr | — |
| **1** *(reserved-gap)* | **Bootstrap un-mask (CI PREREQUISITE)** — Fix A (`# type: ignore[import-not-found]` on the two `httpx` imports) + Fix C (drop `>/dev/null` line 22 + `--quiet` lines 19–20); clean-`[dev]`-venv green proof + a demonstrate-the-catch RED. **Lands first — until it is green, none of Slices 5/10/15's gates actually run in CI.** | implementation (CI) | 0 |
| **5** | **Perf-gate honesty (#12)** — re-scope/relabel `ac_013b` off the synthetic floor; run the cheap subset per-push; RED proof that the old vacuous-green is gone; update `design/perf-gates.md` | implementation (CI) | 0, 1 |
| **10** | **AC-037 wiring + AC-050c cleanup (#14)** — `security` job on `ubuntu-22.04` with a RED egress-trip proof; clear the AC-050c baseline failure | implementation (CI) | 0 |
| **15** | **Dependency-vuln hygiene (Dependabot)** — **manually** bump npm (`markdown-it`/`js-yaml`, root `package-lock.json`) + pip (`idna`/`torch`, `python/uv.lock`) off the open advisories (these have **no auto-PR** — their manifests aren't under a configured `dependabot.yml` directory); reconcile `.github/dependabot.yml` directory coverage (configured pip `/src/python` + npm `/src/ts` miss the alert manifests `python/uv.lock` + root `package-lock.json`); re-run affected suites | implementation (deps) | 0 |
| **20** *(reserved-gap, F-9)* | **pyo3 mac/win cargo-test link fix (#lying-gate)** — the `rust-macos`/`rust-windows` `cargo test --workspace` red is **pre-existing since `80ffa6bd` (2026-05-21), NOT a 0.8.8 regression**: `fathomdb-py/Cargo.toml` hard-codes `pyo3` `extension-module` **always-on**, so the standalone workspace test binary links libpython-less and strict macOS/Windows linkers fail (Linux tolerates it). Fix = drop `extension-module` from the always-on `[dependencies]` features (keep `abi3-py310`). **Verified safe:** wheels (`release.yml:103`, `ci.yml:287/296`), `maturin develop` (`Cargo.toml:30`), and the pytest editable rebuild (`conftest.py`) all pass `--features pyo3/extension-module` **explicitly** → shipped artifact + dev/test flow unchanged. RED = mac/win link-fail today; GREEN = links (CI-confirmed). Lands **after #93** as its own $0 PR so #93 stays CI-only. | implementation (CI) | 0 |
| **40** | **Verification + Release Readiness (0.8.9)** — X1/X2/X3 + R-PG/R-037/R-050c/R-DEP AC gate; confirm the honest gate map is reflected on every board | verification | 5,10,15,20 |

**Keystones / hard gates.** **Reserved-gap Slice 1 (bootstrap un-mask) is a hard prerequisite** — until
`bootstrap.sh` reaches `agent-security.sh`, the AC-037 catch (R-037), the perf gates (R-PG-3), and the
recall test never run in CI (they are vacuously skipped, not green); do Slice 1 first. **R-PG-2
demonstrate-the-catch is a hard gate** — the fix must include a RED test proving the previously-vacuous
gate now fails when it should (`conformance-rewrite-vacuous-green-trap`: a green rewrite can be vacuously
green). Same for R-037-2 (a real egress fixture must trip the gate) and R-BOOT-2 (a broken import must
fail bootstrap *visibly*). A fix that only flips labels without a demonstrated catch is NOT done.

**Tracks (parallelizable).** Perf-gate track **5** ∥ security/cleanup track **10** ∥ dependency-hygiene
track **15**, all off Slice 0.

**Slice 20 landed AFTER Slice 40 — honesty note (F-9).** Slice 40's verification verdict already shipped
in PR #93 (merged `f20059e9`) with the `rust-macos`/`rust-windows` pyo3 red **documented as external and
waived**. Because 40 logically depends on 20, Slice 20 cannot close under an already-"done" 40: it carries
a **Slice 40 re-verify addendum** — after Slice 20 lands on main, re-confirm `rust-macos`/`rust-windows`
are *actually green on main* (the thing 40 waived), and the board must show **0.8.9 is NOT fully complete
until Slice 20 + that re-verify land**. Otherwise the board claims a green 40 over a red only 20 clears.

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering).

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

- **X1 — SDK parity.** No SDK surface change expected; if a gate touches a binding-visible behavior, it
  lands in both Py + TS. The default assertion is that no library API changes.
- **X2 — `mkdocs build` stays green.** `design/perf-gates.md` and any CI-doc updates keep nav green.
- **X3 — docs + `dev/DOC-INDEX.md` maintained** in the closing docs commit; the corrected gate map is
  reflected wherever the boards quote the floor.

`runs/STATUS-0.8.9.md` carries the per-slice X column.

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked (`acceptance-md-locked-no-feature-acs`). These are gate-*correctness* fixes on
existing AC ids (037/050c/012/013/013b/019/020) — **do not re-number or invent ids**; correct the
enforcement and the prose to match (the AC-050c removal-detect is a known separate cleanup, and
ac_013b's 0.90-on-synthetic conflation is the known defect).

## 7. Prerequisites

1. **A userns-permissive CI runner** (`ubuntu-22.04`) available for the AC-037 `unshare -rUn` proof —
   the sandbox lacks rootless userns, so this gate only runs on a permissive runner (HITL decision
   2026-06-02).
2. No upstream release dependency — OOB; can open immediately in parallel with the even line.
3. Worktrees off `$(git rev-parse main)`.

## 8. Out-of-band / parallel notes

- **Runs in parallel to the even line** and shares no files with feature work — pure CI/test-harness.
- **Recommended-before 0.8.16's GA verification** (which leans on these gates being honest), but not a
  hard gate for the intervening feature releases.
- Cross-check every "green" claim against real exit codes (`background-exit-masks-real-exit`): a gate
  job's trailing `echo` is not the gate's exit.

## 9. Immediate next slice

**Slice 0 — map the gate reality.** Stand up `runs/STATUS-0.8.9.md`; produce the honest table of where
each perf gate runs and what it asserts against; design the AC-037 CI wiring + the AC-050c fix; confirm
the post-0.8.8 Dependabot backlog (pyo3 already taken by 0.8.8). Then fan out Slices 5 ∥ 10 ∥ 15.
