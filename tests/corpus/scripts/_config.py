"""Shared typed-config helper for corpus acquire scripts.

An acquire run is a CONFIG, not a code edit: a typed dataclass with sensible
defaults, ``--config file.yaml`` + dotted ``--override key=val`` one-offs, every
field consumed-or-loudly-rejected (unknown-key guard), and YAML/JSON load with a
bare-JSON fallback. Mirrors memex's ``eval/*/config.py`` convention, kept
self-contained here so every acquire script shares one entrypoint.

The helper is generic over a flat config dataclass: the script owns the dataclass
(fields + an optional ``validate()`` method); this module owns loading, overrides,
the unknown-key guard, and CLI wiring. ``acquire_wec_eng.py`` is the exemplar.

Usage:
    @dataclass
    class WecEngConfig:
        split: str = "train"
        sample_size: int = 3000
        seed: int = 20260702
        def validate(self) -> None: ...

    parser = argparse.ArgumentParser()
    add_config_cli(parser)
    args = parser.parse_args()
    cfg = resolve_config(WecEngConfig, args, WecEngConfig())
"""

from __future__ import annotations

import json
from collections.abc import Iterable
from dataclasses import asdict, fields, is_dataclass
from pathlib import Path
from typing import Any, TypeVar

try:
    import yaml

    _HAVE_YAML = True
except ImportError:  # pragma: no cover - exercised only where PyYAML is absent
    yaml = None
    _HAVE_YAML = False

T = TypeVar("T")


def config_from_dict(cls: type[T], data: dict[str, Any]) -> T:
    """Build a typed config of ``cls`` from a plain mapping, rejecting any
    unknown key (a typo fails fast instead of being silently dropped)."""
    if not (is_dataclass(cls) and isinstance(cls, type)):
        raise TypeError(f"{cls!r} is not a config dataclass")
    if not isinstance(data, dict):
        raise ValueError(f"config must be a mapping, got {type(data).__name__}")
    known = {f.name for f in fields(cls)}
    unknown = set(data) - known
    if unknown:
        raise ValueError(f"unknown config keys: {sorted(unknown)}")
    return cls(**data)


def _parse(text: str) -> Any:
    if not text.strip():
        return {}
    if _HAVE_YAML:
        return yaml.safe_load(text)
    return json.loads(text)


def load_config(cls: type[T], path: str | Path) -> T:
    """Load a YAML/JSON config file into a typed ``cls`` (bare-JSON fallback when
    PyYAML is absent; JSON is a valid YAML subset either way)."""
    p = Path(path)
    data = _parse(p.read_text(encoding="utf-8"))
    if data is None:
        data = {}
    if not isinstance(data, dict):
        raise ValueError(
            f"config {p} must parse to a mapping, got {type(data).__name__}"
        )
    return config_from_dict(cls, data)


def _coerce(raw: str) -> Any:
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return raw


def _set_dotted(data: dict[str, Any], dotted: str, value: Any) -> None:
    parts = dotted.split(".")
    cur = data
    for part in parts[:-1]:
        nxt = cur.get(part)
        if not isinstance(nxt, dict):
            nxt = {}
            cur[part] = nxt
        cur = nxt
    cur[parts[-1]] = value


def apply_overrides(cls: type[T], cfg: T, overrides: Iterable[str]) -> T:
    """Return a new config with ``key=val`` dotted-key overrides applied (values
    are JSON-coerced); the input config is not mutated."""
    data = asdict(cfg)
    for item in overrides:
        if "=" not in item:
            raise ValueError(f"override must be 'key=val', got {item!r}")
        key, _, raw = item.partition("=")
        _set_dotted(data, key.strip(), _coerce(raw.strip()))
    return config_from_dict(cls, data)


def add_config_cli(parser: Any) -> None:
    """Register the shared ``--config`` / ``--override`` entrypoint flags."""
    parser.add_argument(
        "--config", default=None, help="path to a YAML/JSON acquire config"
    )
    parser.add_argument(
        "--override",
        action="append",
        default=[],
        metavar="KEY=VAL",
        help="dotted-key override, repeatable (e.g. --override sample_size=500)",
    )


def resolve_config(cls: type[T], args: Any, default: T) -> T:
    """Resolve the effective config: load ``--config`` (or the baked ``default``),
    then apply ``--override`` mutations, then ``validate()`` if the config
    provides it."""
    cfg = load_config(cls, args.config) if args.config else default
    if getattr(args, "override", None):
        cfg = apply_overrides(cls, cfg, args.override)
    validate = getattr(cfg, "validate", None)
    if callable(validate):
        validate()
    return cfg
