# 0.8.22 CI contract fixtures — independent review

- Base: `4517f53ec36e8ee8b3fef94d752d6b8f96ba5238`
- Reviewed commits: `e28af1ba`, `d23a872a`, and `34442720`
- Reviewer: independent adversarial fallback

## Review rounds

1. **Concern:** the exact-matrix helper used `sort -u`, allowing a duplicate
   target row to pass; workflow comments still said macOS and Windows were
   deferred. **Resolved by `d23a872a`:** comparison retains duplicates, a
   duplicate-row negative fixture proves rejection, and comments describe the
   five-target contract accurately.
2. **Concern:** one remaining release-workflow comment described `next` as a
   partial-coverage policy. **Resolved by `34442720`:** it now states the
   actual smoke-gated staged-promotion policy.

The final review confirmed exact runner/target/label mappings, musl exclusion,
main-package-only promotion after all registry smokes, immutable dry-run
candidate checks, Node pins, and the TC-76 desired delete behavior. Production
workflow behavior was unchanged except for corrected explanatory comments.

## Verdict: PASS
