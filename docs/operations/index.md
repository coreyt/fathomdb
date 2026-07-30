# Operations

Operations docs are public-facing operator guidance for the 0.8.20 surface.

- [Erasure](erasure.md) — what `erase_source` / `purge` guarantee, what they
  do not, the erasure-audit record, the non-PII `source_id` rule, and
  `fathomdb doctor orphan-provenance`.

See also [Reference — CLI](../reference/cli.md) for the full `doctor` /
`recover` verb tables, and
[Positions — recovery surface](../positions/recovery-surface.md) for why
recovery is CLI-only while erasure is not.
