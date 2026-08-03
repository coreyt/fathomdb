# Release policy

`dev/release/` is the canonical home for the release workflow, checklists, and
fixtures. This compatibility file preserves historical citations to
`dev/release-policy.md` while the policy is maintained in
[`dev/release/README.md`](release/README.md), `scripts/set-version.sh`, and
`scripts/verify-release-gates.sh`.

## Version source of truth

Use `scripts/set-version.sh --check-files` before a release. It validates the
Axis W lockstep version across Rust, Python, and TypeScript and deliberately
does not change the independently versioned embedder API.

## Release gates

The release workflow verifies the tagged version, publishes in dependency
order, and performs registry-installed smoke tests. A release is not complete
until the published artifacts and their required smoke checks agree with the
platform-capability manifest.
