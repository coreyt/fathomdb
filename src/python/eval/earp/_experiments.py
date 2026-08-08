"""Resolution shim for the repo-root `experiments/` package.

`experiments/` is a package at the REPO ROOT; `eval/` lives under `src/python`,
and pytest's `pythonpath = ["."]` adds only `src/python`. The one pre-existing
importer inserts the repo root by hand, and the repo's test script happens to
run from the root with `python -m`, so `import experiments._lib` resolves today
by accident of cwd rather than by design.

This module makes the dependency explicit and cwd-independent. It is also the
boundary decision itself, recorded in one place: off-wheel `eval/` depends on
repo-root `experiments/`.
"""

from __future__ import annotations

import sys
from pathlib import Path

#: eval/earp/_experiments.py -> eval/earp -> eval -> src/python -> src -> repo
REPO_ROOT = Path(__file__).resolve().parents[4]

if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from experiments import _lib as lib  # noqa: E402 -- must follow the path insert

__all__ = ["REPO_ROOT", "lib"]
