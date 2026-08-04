---
title: Pinned-override rot guard
status: PROPOSED
---

# Pinned-override rot guard

A dependency pin added to *fix* an advisory can silently become the *cause* of
the next one. This proposes a mechanical guard, because the failure is invisible
by construction: the pin still looks like a fix, and its comment still says so.

## The case that motivated it

`package.json` carried this, added in 0.8.9:

```json
"comment-overrides": "0.8.9 R-DEP-1: force transitive security patches for
  markdown-it (GHSA-6v5v-wf23-fmfq) and js-yaml (GHSA-h67p-54hq-rp68) …",
"overrides": { "markdown-it": "14.2.0", "js-yaml": "4.2.0" }
```

On 2026-08-04, GHSA-52cp-r559-cp3m landed against js-yaml with vulnerable range
`>= 4.0.0, < 4.3.0`. **The pinned 4.2.0 was inside it.** The line written to
close a js-yaml advisory had become the js-yaml exposure, while its comment
still advertised it as the remedy. Nothing in the repository disagreed with that
comment.

Three properties make this a guard-shaped problem rather than a
remember-to-check problem:

1. **The artifact lies with confidence.** A reader auditing `overrides` sees a
   named GHSA and a justification. Nothing signals that the justification has
   expired.
2. **A pin defeats the normal remedy.** `overrides` forces a version regardless
   of what upstream ships, so the usual "a patched release exists, take it"
   path does not apply. The pin must be revisited *by hand* or not at all.
3. **The detection channel was off.** Dependabot PR creation is paused
   (`.github/dependabot.yml`), so the advisory produced no pull request. It
   surfaced only as a push-time banner, which is not a gate.

A second, quieter rot exists in the same block: the comment says *"Remove when
markdownlint-cli2 ships patched deps."* Nothing checks whether that day has
come, so a pin outlives its reason and silently holds the tree back.

## What the guard must assert

- **R1 — no pin is itself vulnerable.** For every entry in `overrides`, the
  pinned version must not fall in any known advisory's vulnerable range. This is
  the js-yaml case, and it is the load-bearing requirement.
- **R2 — no pin is obsolete.** If the natural resolution *without* the override
  would already satisfy every advisory and the range the dependents ask for,
  the pin has outlived its reason and should be reported so it can be removed
  deliberately.
- **R3 — every pin states why it exists.** A pin without a recorded reason
  cannot be evaluated by a future reader. The existing `comment-overrides`
  convention is the shape; the guard should require *a* rationale, not a
  particular prose style.

## Design constraints

**Must not be advisory-only.** The repository already learned that a
permanently-red or purely-informational gate trains readers to discount it. This
either gates or is not worth building.

**Must not require network access at verify time.** `verify` runs offline-ish
and must stay deterministic and fast. Prefer resolving against the lockfile plus
a checked-in advisory snapshot, or gate the network probe behind its own
always-on-but-separate job that is allowed to be the only network consumer.
Whichever is chosen, a network outage must produce a *loud failure*, never a
silent pass — an unreachable advisory database means **unverified**, not
**clean**.

**Must not duplicate `npm audit`.** `npm audit` answers "is the resolved tree
vulnerable". It does **not** answer "is this pin still the right pin", which is
R2, nor does it distinguish a vulnerability the pin *caused* from one it merely
failed to prevent. Use `npm audit` as one input; do not stop there.

**Scope beyond npm, deliberately.** The same rot applies to the Cargo
workspace's pinned dependencies and to `scripts/governed-surface-pin.json`,
which pins a commit rather than a version. The slice should decide explicitly
whether to cover them now or record them as out of scope with a reason — not
leave the question unasked.

## Notes for implementation

- The four advisories that remain open at authoring time (`torch`, `idna`,
  `setuptools` in `python/uv.lock`; `dompurify` in the mermaid tool) are
  **documented as intentionally uncovered** in `.github/dependabot.yml`. The
  guard must not re-litigate accepted risk; an accepted exception needs a place
  to live, or the gate will be turned off within a week.
- Red-first is required: the test must construct a pin that has become
  vulnerable and prove the guard fails on it. Re-pinning js-yaml back to 4.2.0
  in a fixture is the exact historical case and makes the regression concrete.
- The guard runs where it can act — this is cheap and static, so it belongs in
  the fast, always-on tier alongside the other governance gates, not behind the
  ~35-minute `verify`.
