# ADR-0.8.18 — Full publish pipeline (#11-full)

- **Status: ACCEPTED (HITL SIGNED 2026-07-09).** Design review CLEAN after 4 codex rounds (BLOCKs resolved +
  re-confirmed, not overridden); Steward-verified. npm dist-tag label + publish deferred to Slice 20/40 (per-
  `x.y.z` HITL gate). Full requirements + ACs + design in
  `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md` §U2.

## Decision (proposed — rulings applied)

1. **Scope reconciliation (verify-design-against-code):** `.github/workflows/release.yml` (549 ln) already
   implements most of #11-full — maturin+napi matrices, the `all-builds-passed` cross-ecosystem gate, tiered
   `publish-rust-t1..t7` (index-propagation + idempotency), parallel pypi/npm, post-publish smoke, co-tagging,
   github-release; two-axis `set-version.sh` + `verify-release-gates.sh`. **Slice 20 = reconcile + gap-fill +
   dry-run + harden, NOT author** (R-REL-4a).
2. **D5 ✅ platform scope — critical path = `x86_64-unknown-linux-gnu` ONLY.** Slice-20 hardening + dry-run is
   scoped to x86_64-linux (R-REL-4b). The other declared targets (aarch64 / darwin / windows / musl, py+napi)
   **stay supported but are OFF the critical path** — delivered by a **separate follow-on orchestrator** after
   x86_64-linux GA works. Cross-platform matrix completion is **deferred-to-follow-on (noted, not dropped)**
   (R-REL-4d) — no CI matrix expansion in 0.8.18.
3. **D6 ✅ harden for the 0.8.20 OPP-12 breaking-pair publish (F-19/F-21):** #11-full is the *prerequisite* for
   the coordinated Memex `0.5.x-successor` breaking-pair publish — harden atomicity / index-propagation waits /
   idempotent re-runs of the paired publish (R-REL-4c), not just standalone GA.
4. **Verification must EXERCISE the tiered paths** (codex U2-a): a bare `--dry-run` SKIPS dependent Rust crates +
   PyPI + smoke + co-tagging → insufficient; use a staging/test index (R-REL-4b). **Gate the 0.8.18 GA tag's matrix
   to x86_64-linux ONLY** (codex U2-b, R-REL-4e) — `release.yml` currently fires the full macOS/Windows/aarch64/napi
   matrix on any tag; exclude the deferred platforms from this tag (re-enabled by the follow-on orchestrator). Drop
   the **atomicity** claim (codex U2-c): specify preflight + ordered commits + per-registry retry/no-op idempotency
   (npm/PyPI included) + poll-for-resolvability (not the 60 s sleep) + rollback-forward (R-REL-4c). **Gate the npm artifact/platform contract** (codex R2 U2-1, R-REL-4f): matrix-gating CI is not enough —
   `src/ts/package.json` has no `os`/`cpu`, so a linux-x64-only artifact under `latest` breaks mac/win at runtime;
   use napi per-platform `optionalDependencies` + `os`/`cpu` + a **non-`latest` dist-tag** until the follow-on
   completes the matrix (dist-tag = HITL). Real `v*` tag = **Slice 40, HITL-gated** (fires the REAL 8-tier publish).

## Consequences

GA = **release-engineering GA** (publish machinery + frozen eu7/latency gates); pre-1.0.0 = beta — **no
scale-envelope label** (F-17/F-20). The real tag tags the whole 0.8.x line → coordinate with the experiment
program's stopping point. Cross-platform GA is a distinct follow-on deliverable.
