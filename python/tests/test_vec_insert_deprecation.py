"""Pack G: VecInsert deprecation test.

The managed vector projection (configure_embedding + configure_vec_kind)
is the supported path for populating ``vec_<kind>`` tables. Raw
``VecInsert`` remains importable during the transition window but must
raise :class:`DeprecationWarning` on instantiation.
"""

from __future__ import annotations

import warnings

import pytest


def test_vec_insert_import_emits_deprecation_warning() -> None:
    """Instantiating ``VecInsert`` from the public package raises
    ``DeprecationWarning`` pointing at the managed vector projection."""
    from fathomdb import VecInsert

    with pytest.warns(DeprecationWarning, match="configure_vec"):
        _ = VecInsert(chunk_id="c1", embedding=[0.0, 0.0, 0.0, 0.0])


def test_vec_insert_is_still_usable_after_warning() -> None:
    """Deprecation must be a warning, not a removal — the struct still
    serialises to a wire dict with the expected shape so callers mid-
    migration continue to work."""
    from fathomdb import VecInsert

    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        v = VecInsert(chunk_id="c1", embedding=[1.0, 2.0, 3.0, 4.0])
    wire = v.to_wire()
    assert wire == {"chunk_id": "c1", "embedding": [1.0, 2.0, 3.0, 4.0]}
