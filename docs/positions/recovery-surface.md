# Recovery Surface

Recovery is an operator concern. The public posture is that automated recovery
flows belong to the CLI surface rather than the application SDK surface.

## The line, as of 0.8.20

No SDK verb is named `recover`, `restore`, `repair`, `fix` or `rebuild` — that
five-name denylist is asserted by the conformance suites in all three
languages, and by a compile-fail proof on the Rust facade. In Rust the whole
operator seam sits behind the `operator` cargo feature, which only
`fathomdb-cli` enables; the default facade resolves no recovery-named and no
raw-SQL method. Gating, not deletion: engine behaviour is identical either way.

## Erasure is not recovery

**Deletion on request is an application obligation, not an operator one.**
Since 0.8.20 the SDK ships `purge` (one governed node, by `logical_id`) and
`erase_source` (every row carrying one provenance), so an embedded consumer
with no `fathomdb` binary on `PATH` can discharge a deletion obligation
directly. Neither carries a denylist name and neither is a recovery flow.

Two erasure capabilities remain CLI-only, deliberately:

- the engine's reserved `_`-prefixed provenance namespace, reachable only
  through `fathomdb recover --accept-data-loss --excise-source`;
- op-store record erasure
  (`--excise-collection` / `--excise-record-key`).

See [Erasure](../operations/erasure.md).
