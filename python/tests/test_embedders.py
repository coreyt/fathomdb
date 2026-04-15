"""Tests for Pack D: Python embedding adapters."""

import struct
import sys
from unittest.mock import MagicMock, patch

import pytest


# ---------------------------------------------------------------------------
# 1. EmbedderIdentity fields and immutability
# ---------------------------------------------------------------------------


def test_embedder_identity_fields():
    from fathomdb.embedders import EmbedderIdentity

    eid = EmbedderIdentity(
        model_identity="bge-small-en-v1.5",
        model_version="1.5",
        dimensions=384,
        normalization_policy="l2",
    )
    assert eid.model_identity == "bge-small-en-v1.5"
    assert eid.model_version == "1.5"
    assert eid.dimensions == 384
    assert eid.normalization_policy == "l2"
    # frozen dataclass must reject mutation
    with pytest.raises((AttributeError, TypeError)):
        eid.dimensions = 768  # type: ignore[misc]


# ---------------------------------------------------------------------------
# 2. OpenAIEmbedder.identity()
# ---------------------------------------------------------------------------


def test_openai_identity():
    from fathomdb.embedders._openai import OpenAIEmbedder

    emb = OpenAIEmbedder(
        model="text-embedding-3-small", api_key="sk-test", dimensions=1536
    )
    eid = emb.identity()
    assert eid.model_identity == "text-embedding-3-small"
    assert eid.dimensions == 1536
    assert eid.model_version is None
    assert eid.normalization_policy == "none"


# ---------------------------------------------------------------------------
# 3. OpenAIEmbedder.embed() with mocked httpx
# ---------------------------------------------------------------------------


def test_openai_embed_mocked():
    from fathomdb.embedders._openai import OpenAIEmbedder

    fake_vec = [0.1, 0.2, 0.3]
    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()
    mock_response.json.return_value = {"data": [{"embedding": fake_vec}]}

    with patch("httpx.Client") as mock_client_cls:
        mock_client = MagicMock()
        mock_client_cls.return_value.__enter__ = MagicMock(return_value=mock_client)
        mock_client_cls.return_value.__exit__ = MagicMock(return_value=False)
        mock_client.post.return_value = mock_response

        emb = OpenAIEmbedder(
            model="text-embedding-3-small", api_key="sk-test", dimensions=3
        )
        result = emb.embed("hello world")

    assert isinstance(result, list)
    assert all(isinstance(v, float) for v in result)
    assert result == fake_vec


# ---------------------------------------------------------------------------
# 4. OpenAIEmbedder cache: same text → HTTP called only once
# ---------------------------------------------------------------------------


def test_openai_cache_hit():
    from fathomdb.embedders._openai import OpenAIEmbedder

    fake_vec = [0.1, 0.2, 0.3]
    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()
    mock_response.json.return_value = {"data": [{"embedding": fake_vec}]}

    call_count = 0

    def fake_post(*args, **kwargs):
        nonlocal call_count
        call_count += 1
        return mock_response

    with patch("httpx.Client") as mock_client_cls:
        mock_client = MagicMock()
        mock_client_cls.return_value.__enter__ = MagicMock(return_value=mock_client)
        mock_client_cls.return_value.__exit__ = MagicMock(return_value=False)
        mock_client.post.side_effect = fake_post

        emb = OpenAIEmbedder(
            model="text-embedding-3-small", api_key="sk-test", dimensions=3
        )
        emb.embed("hello")
        emb.embed("hello")  # should hit cache

    assert call_count == 1


# ---------------------------------------------------------------------------
# 5. JinaEmbedder.identity()
# ---------------------------------------------------------------------------


def test_jina_identity():
    from fathomdb.embedders._jina import JinaEmbedder

    emb = JinaEmbedder(api_key="jina-test-key")
    eid = emb.identity()
    assert eid.model_identity == "jina-embeddings-v2-base-en"
    assert eid.dimensions == 768
    assert eid.model_version is None
    assert eid.normalization_policy == "none"


# ---------------------------------------------------------------------------
# 6. JinaEmbedder.embed() with mocked httpx
# ---------------------------------------------------------------------------


def test_jina_embed_mocked():
    from fathomdb.embedders._jina import JinaEmbedder

    fake_vec = [float(i) / 768 for i in range(768)]
    mock_response = MagicMock()
    mock_response.raise_for_status = MagicMock()
    mock_response.json.return_value = {"data": [{"embedding": fake_vec}]}

    with patch("httpx.Client") as mock_client_cls:
        mock_client = MagicMock()
        mock_client_cls.return_value.__enter__ = MagicMock(return_value=mock_client)
        mock_client_cls.return_value.__exit__ = MagicMock(return_value=False)
        mock_client.post.return_value = mock_response

        emb = JinaEmbedder(api_key="jina-test-key")
        result = emb.embed("hello world")

    assert isinstance(result, list)
    assert len(result) == 768
    assert all(isinstance(v, float) for v in result)


# ---------------------------------------------------------------------------
# 7. StellaEmbedder truncation + L2 norm
# ---------------------------------------------------------------------------


def test_stella_truncation_and_l2_norm():
    from fathomdb.embedders._stella import StellaEmbedder

    # Raw 1024-dim all-ones vector; after truncation to 512 then L2-norm
    # each element should be 1/sqrt(512)
    import math

    raw_embedding = [1.0] * 1024

    # Build a fake SentenceTransformer that returns our raw_embedding
    fake_model = MagicMock()
    fake_model.encode.return_value = raw_embedding

    fake_st_module = MagicMock()
    fake_st_module.SentenceTransformer.return_value = fake_model

    with patch.dict("sys.modules", {"sentence_transformers": fake_st_module}):
        emb = StellaEmbedder(model_name="dunzhang/stella_en_400M_v5", dimensions=512)
        result = emb.embed("hello world")

    assert len(result) == 512
    norm = math.sqrt(sum(v * v for v in result))
    assert abs(norm - 1.0) < 1e-5


# ---------------------------------------------------------------------------
# 8. SubprocessEmbedder with a real Python echo subprocess
# ---------------------------------------------------------------------------


def test_subprocess_echo():
    from fathomdb.embedders._subprocess import SubprocessEmbedder

    N = 4
    # This subprocess reads one line from stdin, then writes N*4 zero bytes
    cmd = [
        sys.executable,
        "-c",
        f"import sys, struct; line = sys.stdin.readline(); sys.stdout.buffer.write(struct.pack('{N}f', *([0.0]*{N}))); sys.stdout.buffer.flush()",
    ]
    emb = SubprocessEmbedder(command=cmd, dimensions=N)
    result = emb.embed("test input")
    assert isinstance(result, list)
    assert len(result) == N
    assert all(isinstance(v, float) for v in result)


# ---------------------------------------------------------------------------
# 9. Importing fathomdb.embedders does NOT require httpx or sentence_transformers
# ---------------------------------------------------------------------------


def test_import_without_optional_deps():
    # Remove optional deps from sys.modules to simulate absence
    saved = {}
    for key in list(sys.modules.keys()):
        if key == "httpx" or key.startswith("sentence_transformers"):
            saved[key] = sys.modules.pop(key)

    # Also temporarily make them un-importable
    import builtins

    real_import = builtins.__import__

    blocked = {"httpx", "sentence_transformers"}

    def guarded_import(name, *args, **kwargs):
        if name in blocked:
            raise ImportError(f"Simulated absence of {name}")
        return real_import(name, *args, **kwargs)

    builtins.__import__ = guarded_import
    try:
        # Remove cached module so re-import runs
        for key in list(sys.modules.keys()):
            if key.startswith("fathomdb.embedders"):
                del sys.modules[key]
        import fathomdb.embedders  # noqa: F401 — must not raise
        from fathomdb.embedders import EmbedderIdentity, QueryEmbedder  # noqa: F401
    finally:
        builtins.__import__ = real_import
        sys.modules.update(saved)


# ---------------------------------------------------------------------------
# 10. QueryEmbedder is abstract — cannot be instantiated directly
# ---------------------------------------------------------------------------


def test_query_embedder_abc():
    from fathomdb.embedders import QueryEmbedder

    with pytest.raises(TypeError):
        QueryEmbedder()  # type: ignore[abstract]
