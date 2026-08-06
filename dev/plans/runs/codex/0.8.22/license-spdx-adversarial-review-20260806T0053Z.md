# 0.8.22 SPDX-only Cargo metadata — independent review

- Base: `4517f53ec36e8ee8b3fef94d752d6b8f96ba5238`
- Reviewed commit: `ced8d810` (`fix(release): use SPDX-only Cargo license metadata`)
- Reviewer: independent adversarial fallback
- Reason for fallback: the prescribed Codex review wrapper stopped during its
  broad instruction-file discovery on permission-denied temporary directories,
  before producing a verdict. Its raw terminal record remains in the isolated
  review worktree and was not treated as approval.

## Findings

- MIT is a valid SPDX expression; all seven publishable crates inherit
  `license = "MIT"` and declare no `license-file`.
- Each of the seven has a regular package-root `LICENSE`, byte-identical to
  the root license; both non-publishable workspace crates remain excluded.
- The guard rejects workspace- or crate-level `license-file`, checks every
  local copy, and verifies real `cargo package --list` output for each
  publishable package.
- Regression arms prove dual metadata and missing package-root license text
  fail, so the guard is non-vacuous.

## Verdict: PASS
