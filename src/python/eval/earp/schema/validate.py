"""A pure-stdlib JSON Schema walker, total over `earp.config.v1.schema.json`.

Deliberately not `jsonschema`. That package is importable in many environments
but is declared in none of `pyproject.toml`'s extras, which is exactly how this
repo previously shipped a harness that failed a clean install -- the numpy
declaration still carries the codex §9 [P1] note recording it.

The point is not to reimplement JSON Schema. It is to make the resolver's
known-key set **derived from the schema** rather than transcribed beside it, so
adding a schema key without wiring it is a red test rather than a latent lie.

Interpreted keywords -- the exact set this schema uses:
    type, enum, const, required, properties, additionalProperties (false only),
    items, minimum, maximum, minItems, uniqueItems, pattern

Ignored as annotations: $schema, $id, title, description.

Any other keyword is a hard error. That is what makes totality load-bearing
rather than decorative: a schema that grows an `if`/`allOf` fails loudly here
instead of being silently under-validated.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any, Iterator, Mapping

_INTERPRETED = frozenset(
    {
        "type",
        "enum",
        "const",
        "required",
        "properties",
        "additionalProperties",
        "items",
        "minimum",
        "maximum",
        "minItems",
        "uniqueItems",
        "pattern",
        # Added for the sidecar schemas. The result schema puts nearly all its
        # structure inside `$defs`, and the per-query schema's `if`/`then` is
        # what makes `outcome: scored` carry numbers -- omitting either would
        # silently under-validate the very files S4 claims to validate.
        "$defs",
        "$ref",
        "oneOf",
        "allOf",
        "if",
        "then",
    }
)
_IGNORED = frozenset({"$schema", "$id", "title", "description"})


class UnsupportedSchema(Exception):
    """The schema uses a keyword this walker does not interpret."""


class Defect(str):
    """A validation defect, tagged with its class."""

    __slots__ = ()


@dataclass(frozen=True)
class Finding:
    kind: str  # "unknown" | "missing" | "invalid"
    path: str
    message: str


def _subschemas(schema: Mapping[str, Any]) -> Iterator[Mapping[str, Any]]:
    """Every nested schema position.

    Recursing only into `properties`/`items` -- as the first version did --
    makes a totality claim vacuous for the result schema, whose structure lives
    almost entirely inside `$defs`.
    """
    for key in ("properties", "$defs"):
        for sub in schema.get(key, {}).values():
            if isinstance(sub, dict):
                yield sub
    for key in ("items", "if", "then", "additionalProperties"):
        sub = schema.get(key)
        if isinstance(sub, dict):
            yield sub
    for key in ("oneOf", "allOf"):
        for sub in schema.get(key, []) or []:
            if isinstance(sub, dict):
                yield sub


def assert_supported(schema: Mapping[str, Any]) -> None:
    """Raise unless every keyword in `schema` is interpreted or ignored."""
    unknown = set(schema) - _INTERPRETED - _IGNORED
    if unknown:
        raise UnsupportedSchema(f"uninterpreted schema keywords: {sorted(unknown)}")
    for sub in _subschemas(schema):
        assert_supported(sub)


def declared_paths(schema: Mapping[str, Any], prefix: str = "") -> Iterator[str]:
    """Every dotted path the schema declares. Arrays are yielded at the array
    node and never per element, which is also how consumption is marked."""
    for name, sub in schema.get("properties", {}).items():
        path = f"{prefix}{name}"
        yield path
        if sub.get("type") == "object" or "properties" in sub:
            yield from declared_paths(sub, f"{path}.")


def _resolve(ref: str, root: Mapping[str, Any]) -> Mapping[str, Any]:
    """Resolve a fragment-only `#/$defs/<name>` reference."""
    if not ref.startswith("#/"):
        raise UnsupportedSchema(f"only fragment refs are supported, got {ref!r}")
    node: Any = root
    for part in ref[2:].split("/"):
        node = node[part]
    if not isinstance(node, dict):
        raise UnsupportedSchema(f"{ref} does not name a schema")
    return node


def _type_ok(value: Any, expected: str) -> bool:
    if expected == "null":
        return value is None
    if expected == "object":
        return isinstance(value, dict)
    if expected == "array":
        return isinstance(value, list)
    if expected == "string":
        return isinstance(value, str)
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "integer":
        # PyYAML yields Python bool, and isinstance(True, int) is True, so an
        # unguarded check would let `rerank_depth: true` resolve as 1.
        return isinstance(value, int) and not isinstance(value, bool)
    if expected == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    return True


def validate(
    value: Any,
    schema: Mapping[str, Any],
    path: str = "",
    root: Mapping[str, Any] | None = None,
    _seen: frozenset[str] = frozenset(),
) -> list[Finding]:
    """Collect every defect. Never first-failure: a config author needs all of
    them in one pass."""
    findings: list[Finding] = []
    here = path or "<root>"
    root = schema if root is None else root

    ref = schema.get("$ref")
    if isinstance(ref, str):
        if ref in _seen:  # cycle guard
            return findings
        return validate(value, _resolve(ref, root), path, root, _seen | {ref})

    for sub in schema.get("allOf", []) or []:
        findings.extend(validate(value, sub, path, root, _seen))

    if "if" in schema and "then" in schema:
        # Conditional requirement: the `then` applies only when `if` matches.
        if not validate(value, schema["if"], path, root, _seen):
            findings.extend(validate(value, schema["then"], path, root, _seen))

    branches = schema.get("oneOf")
    if branches:
        # One branch MUST fail, so naively unioning would emit guaranteed-false
        # defects. Report nothing if any branch is clean, else the smallest set.
        results = [validate(value, branch, path, root, _seen) for branch in branches]
        if all(results):
            findings.extend(min(results, key=len))

    if "const" in schema and value != schema["const"]:
        findings.append(Finding("invalid", here, f"must be {schema['const']!r}"))
        return findings
    if "enum" in schema and value not in schema["enum"]:
        findings.append(
            Finding("invalid", here, f"must be one of {sorted(map(str, schema['enum']))}")
        )
        return findings

    expected = schema.get("type")
    if expected is not None:
        # A list-valued `type` is a UNION. Treating it as a scalar made
        # `["string", "null"]` fall through and accept anything at all -- and
        # that shape appears 20 times across the two sidecar schemas.
        options = expected if isinstance(expected, list) else [expected]
        if not any(_type_ok(value, option) for option in options):
            findings.append(
                Finding("invalid", here, f"must be {expected}, got {type(value).__name__}")
            )
            return findings

    if isinstance(value, str) and "pattern" in schema:
        if not re.fullmatch(schema["pattern"], value):
            findings.append(Finding("invalid", here, f"must match {schema['pattern']}"))

    if isinstance(value, (int, float)) and not isinstance(value, bool):
        # NB minimum/maximum cannot catch NaN: nan < lo and nan > hi are both
        # False. The non-finite rule lives in the resolver, outside the walker.
        if "minimum" in schema and value < schema["minimum"]:
            findings.append(Finding("invalid", here, f"must be >= {schema['minimum']}"))
        if "maximum" in schema and value > schema["maximum"]:
            findings.append(Finding("invalid", here, f"must be <= {schema['maximum']}"))

    if isinstance(value, list):
        if "minItems" in schema and len(value) < schema["minItems"]:
            findings.append(Finding("invalid", here, f"needs >= {schema['minItems']} items"))
        if schema.get("uniqueItems") and len(value) != len({repr(v) for v in value}):
            findings.append(Finding("invalid", here, "items must be unique"))
        item_schema = schema.get("items")
        if isinstance(item_schema, dict):
            for index, item in enumerate(value):
                findings.extend(validate(item, item_schema, f"{path}[{index}]", root, _seen))

    if isinstance(value, dict):
        properties = schema.get("properties", {})
        for name in schema.get("required", []):
            if name not in value:
                findings.append(
                    Finding("missing", f"{path}{name}" if path else name, "required key absent")
                )
        additional = schema.get("additionalProperties")
        if additional is False:
            for name in value:
                if name not in properties:
                    findings.append(
                        Finding(
                            "unknown",
                            f"{path}{name}" if path else name,
                            "not defined by the schema",
                        )
                    )
        elif isinstance(additional, dict):
            # A SCHEMA, not a flag. Without this every per-K aggregate -- the
            # highest-value part of the sidecar -- goes unvalidated.
            for name, item in value.items():
                if name not in properties:
                    findings.extend(validate(item, additional, f"{path}{name}.", root, _seen))
        for name, sub in properties.items():
            if name in value:
                findings.extend(validate(value[name], sub, f"{path}{name}.", root, _seen))

    return findings


__all__ = ["Finding", "UnsupportedSchema", "assert_supported", "declared_paths", "validate"]
