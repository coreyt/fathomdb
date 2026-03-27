from typing import Any


def main(argv: list[str] | None = None) -> int:
    from .app import main as app_main

    return app_main(argv)


def run_harness(*args: Any, **kwargs: Any):
    from .app import run_harness as app_run_harness

    return app_run_harness(*args, **kwargs)


__all__ = ["main", "run_harness"]
