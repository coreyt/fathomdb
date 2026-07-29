"""`python -m sbom_survey` — the entry point the acceptance suite drives."""

from __future__ import annotations

import sys

from .cli import main

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
