"""Property-test scaffold for the Python surface.

Real targets (per ADR-0.6.0-python-api-shape):
  - Python API round-trip: open(close(...)) idempotence on a temp dir
  - JSON-Schema validation: parse(serialize(s)) == s for canonical schemas
  - filter_json_fused_*: invalid schema raises BuilderValidationError (per
    project_fused_json_filters_contract.md — must not silently degrade)

Replace this trivial property when real Python API surface lands.
"""

import pytest

try:
    from hypothesis import given, strategies as st
except ImportError:  # pragma: no cover
    pytest.skip("hypothesis not installed; install with `pip install -e src/python[test]`",
                allow_module_level=True)


@given(st.integers())
def test_placeholder_identity(x: int) -> None:
    assert x == x
