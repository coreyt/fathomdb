"""ARCH-006: TOKENIZER_PRESETS is computed from the Rust constant via FFI.

The Python SDK must NOT hand-maintain a copy of the preset dict; it is
populated at module load time from ``_fathomdb.list_tokenizer_presets()``.
"""

from __future__ import annotations


def test_tokenizer_presets_match_rust_source_of_truth() -> None:
    """The five well-known presets flow from Rust via the native extension."""
    from fathomdb._admin import TOKENIZER_PRESETS

    assert TOKENIZER_PRESETS == {
        "recall-optimized-english": "porter unicode61 remove_diacritics 2",
        "precision-optimized": "unicode61 remove_diacritics 2",
        "global-cjk": "icu",
        "substring-trigram": "trigram",
        "source-code": "unicode61 tokenchars '._-$@'",
    }


def test_tokenizer_presets_runtime_type_is_plain_dict() -> None:
    """The cross-language wire shape is a ``dict[str, str]``."""
    from fathomdb._admin import TOKENIZER_PRESETS

    assert isinstance(TOKENIZER_PRESETS, dict)
    for name, value in TOKENIZER_PRESETS.items():
        assert isinstance(name, str)
        assert isinstance(value, str)
