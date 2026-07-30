# fathomdb-embedder-api

The embedder contract for **FathomDB**, a local-first embedded retrieval engine built on SQLite.

This crate is the seam between FathomDB and whatever produces your vectors. It contains a trait, an
identity struct, an error enum and a type alias — nothing else, and no dependencies. Implement
`Embedder` here and the engine can use your model without knowing anything about it.

## Its own version axis

`fathomdb-embedder-api` deliberately does **not** move in lockstep with the rest of the FathomDB
workspace. It carries an independent version that bumps only when the trait surface itself changes,
so an out-of-tree embedder implementation is not forced to churn on every engine release. Expect
this crate's version number to be well behind the engine's, and to stay there.

That said: the workspace as a whole is **pre-1.0 and beta**, and this crate is no exception. The
surface is small and intended to be stable, but it has not yet been declared frozen.

## The surface

```rust
pub type Vector = Vec<f32>;

pub struct EmbedderIdentity {
    pub name: String,
    pub revision: String,
    pub dimension: u32,
}

pub enum EmbedderError {
    Failed { message: String },
    Timeout,
}

pub trait Embedder: Send + Sync {
    fn identity(&self) -> EmbedderIdentity;
    fn embed(&self, input: &str) -> Result<Vector, EmbedderError>;

    // Default implementation loops `embed`.
    fn embed_batch(&self, inputs: &[&str]) -> Result<Vec<Vector>, EmbedderError>;
}
```

Two contracts are worth stating out loud:

- **`identity` is load-bearing.** FathomDB records the `(name, revision, dimension)` of the embedder
  that produced a database's vectors and refuses to silently mix vectors from different embedders.
  Change the model, change the identity.
- **`embed_batch` must match `embed`.** Overriding it is how a backend amortises per-call overhead
  and saturates a GPU, but the results must be numerically equivalent (within float tolerance) to
  calling `embed` on each input — so a caller can turn batching on without changing the vectors
  already written to an index.

## Install

```bash
cargo add fathomdb-embedder-api
```

## Related crates

| Crate | Role |
| --- | --- |
| [`fathomdb-embedder`](https://crates.io/crates/fathomdb-embedder) | Built-in implementations of this trait |
| [`fathomdb`](https://crates.io/crates/fathomdb) | The facade most applications should use |

## License

MIT. See the `LICENSE` file shipped in this crate.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
