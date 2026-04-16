from ._base import EmbedderIdentity, QueryEmbedder

# These constants MUST stay in sync with the Rust builtin embedder:
#   crates/fathomdb-engine/src/embedder/builtin.rs
#   MODEL_ID = "BAAI/bge-small-en-v1.5"
#   MODEL_REVISION = "main"
#   MODEL_DIMENSION = 384
_MODEL_IDENTITY = "BAAI/bge-small-en-v1.5"
_MODEL_VERSION = "main"
_DIMENSIONS = 384


class BuiltinEmbedder(QueryEmbedder):
    r"""Identity proxy for the FathomDB built-in Candle/BGE-small embedder.

    Use this when the engine was opened with ``embedder="builtin"`` and you
    want to call ``configure_vec`` to record the correct ``VecProfile``
    in the database.  It provides the exact identity the Rust engine uses:

    - ``model_identity``: ``"BAAI/bge-small-en-v1.5"``
    - ``model_version``:  ``"main"``
    - ``dimensions``:     ``384``
    - ``normalization_policy``: ``"l2"``

    Example::

        db = FathomDB.open("store.db", embedder="builtin")
        db.admin.configure_vec(BuiltinEmbedder(), agree_to_rebuild_impact=True)
        # Rebuild is performed by the Rust engine, not by Python:
        db.admin.regenerate_vector_embeddings(config)

    .. note::
        :meth:`embed` raises ``NotImplementedError``.  Actual embedding is
        performed by the Rust Candle runtime when the engine is opened with
        ``embedder="builtin"``.  Do **not** pass this object to code that
        calls ``embed()`` directly (e.g. a custom retrieval loop).
    """

    def identity(self) -> EmbedderIdentity:
        """Return the identity matching the Rust built-in BGE-small embedder."""
        return EmbedderIdentity(
            model_identity=_MODEL_IDENTITY,
            model_version=_MODEL_VERSION,
            dimensions=_DIMENSIONS,
            normalization_policy="l2",
        )

    def embed(self, text: str) -> list[float]:
        """Not implemented — embedding is handled by the Rust engine.

        Raises
        ------
        NotImplementedError
            Always.  Use ``Engine.open(embedder="builtin")`` for actual
            embedding; ``BuiltinEmbedder`` is an identity-only proxy.
        """
        raise NotImplementedError(
            "BuiltinEmbedder cannot embed from Python. "
            "Open the engine with embedder='builtin' and use "
            "admin.regenerate_vector_embeddings() for rebuilds."
        )
