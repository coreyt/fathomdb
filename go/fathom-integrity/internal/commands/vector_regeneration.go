// Vector regeneration via the admin bridge was removed in fathomdb 0.4.0.
// See Engine::regenerate_vector_embeddings in the Rust crate for the
// replacement surface; the bridge cannot carry the required embedder
// reference across its JSON-over-stdio boundary.
package commands
