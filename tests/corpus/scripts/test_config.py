"""Unit tests for the shared typed-config helper (_config.py).

Pure, no network. Covers the acquire-script config contract: typed dataclass
defaults, YAML/JSON load with a bare-JSON fallback, dotted-key --override
one-offs, the unknown-key guard (consumed-or-loudly-rejected), and the
--config/--override CLI wiring + resolve_config validation hook.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _config import (  # noqa: E402
    add_config_cli,
    apply_overrides,
    config_from_dict,
    load_config,
    resolve_config,
)


@dataclass
class SampleConfig:
    split: str = "train"
    sample_size: int = 3000
    seed: int = 20260702

    def validate(self) -> None:
        if self.split not in ("train", "dev", "test"):
            raise ValueError(f"bad split {self.split!r}")
        if not isinstance(self.sample_size, int) or self.sample_size <= 0:
            raise ValueError(f"bad sample_size {self.sample_size!r}")


def test_config_from_dict_builds_typed_config():
    cfg = config_from_dict(SampleConfig, {"split": "dev", "sample_size": 10})
    assert cfg.split == "dev"
    assert cfg.sample_size == 10
    assert cfg.seed == 20260702


def test_config_from_dict_rejects_unknown_key():
    with pytest.raises(ValueError, match="unknown config keys"):
        config_from_dict(SampleConfig, {"bogus": 1})


def test_config_from_dict_rejects_non_mapping():
    with pytest.raises(ValueError, match="mapping"):
        config_from_dict(SampleConfig, ["not", "a", "dict"])


def test_load_config_reads_yaml(tmp_path):
    p = tmp_path / "c.yaml"
    p.write_text("split: test\nsample_size: 5\nseed: 42\n", encoding="utf-8")
    cfg = load_config(SampleConfig, p)
    assert cfg.split == "test"
    assert cfg.sample_size == 5
    assert cfg.seed == 42


def test_load_config_reads_bare_json(tmp_path):
    p = tmp_path / "c.json"
    p.write_text(json.dumps({"split": "dev", "sample_size": 7}), encoding="utf-8")
    cfg = load_config(SampleConfig, p)
    assert cfg.split == "dev"
    assert cfg.sample_size == 7


def test_load_config_empty_file_is_all_defaults(tmp_path):
    p = tmp_path / "empty.yaml"
    p.write_text("", encoding="utf-8")
    cfg = load_config(SampleConfig, p)
    assert cfg == SampleConfig()


def test_apply_overrides_does_not_mutate_input():
    base = SampleConfig()
    out = apply_overrides(SampleConfig, base, ["sample_size=99"])
    assert out.sample_size == 99
    assert base.sample_size == 3000


def test_apply_overrides_coerces_values():
    out = apply_overrides(SampleConfig, SampleConfig(), ["seed=1", "split=dev"])
    assert out.seed == 1
    assert isinstance(out.seed, int)
    assert out.split == "dev"


def test_apply_overrides_rejects_malformed_item():
    with pytest.raises(ValueError, match="override"):
        apply_overrides(SampleConfig, SampleConfig(), ["no-equals-sign"])


def test_resolve_config_applies_overrides_and_validates():
    parser = argparse.ArgumentParser()
    add_config_cli(parser)
    args = parser.parse_args(["--override", "split=dev", "--override", "sample_size=25"])
    cfg = resolve_config(SampleConfig, args, SampleConfig())
    assert cfg.split == "dev"
    assert cfg.sample_size == 25


def test_resolve_config_default_when_no_flags():
    parser = argparse.ArgumentParser()
    add_config_cli(parser)
    args = parser.parse_args([])
    cfg = resolve_config(SampleConfig, args, SampleConfig())
    assert cfg == SampleConfig()


def test_resolve_config_validation_rejects_bad_value():
    parser = argparse.ArgumentParser()
    add_config_cli(parser)
    args = parser.parse_args(["--override", "sample_size=-1"])
    with pytest.raises(ValueError, match="sample_size"):
        resolve_config(SampleConfig, args, SampleConfig())


def test_resolve_config_loads_from_config_file(tmp_path):
    p = tmp_path / "c.yaml"
    p.write_text("split: test\n", encoding="utf-8")
    parser = argparse.ArgumentParser()
    add_config_cli(parser)
    args = parser.parse_args(["--config", str(p)])
    cfg = resolve_config(SampleConfig, args, SampleConfig())
    assert cfg.split == "test"
