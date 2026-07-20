#!/usr/bin/env python3
"""Run the REAL shipped ELPS harness (`src/python/eval/elps_live_harness.py`) in stub mode.

TEST-ONLY shim for 0.8.20 Slice 5 fix-4. The engine spawns an extractor as an
argv command, so this wrapper exists to (a) force `ELPS_STUB_MODE=1` BEFORE the
harness module is imported — the harness snapshots that variable at import time
— and (b) load the harness from an absolute path supplied by the caller, so the
test does not depend on cwd or on `src/python` being installed.

Usage:  elps_stub_shim.py <absolute-path-to-elps_live_harness.py>

Driving the real harness (rather than a hand-written stub) is the point: it is
what makes the multi-document ingest test a witness for the SHIPPED extractor
contract instead of for a fixture we control.
"""
import importlib.util
import os
import sys

# Must be set before importing the harness: `_STUB_MODE` is read at module import.
os.environ["ELPS_STUB_MODE"] = "1"

if len(sys.argv) < 2:
    print("usage: elps_stub_shim.py <path-to-elps_live_harness.py>", file=sys.stderr)
    sys.exit(2)

harness_path = sys.argv[1]
if not os.path.isfile(harness_path):
    print(f"[elps_stub_shim] no harness at {harness_path}", file=sys.stderr)
    sys.exit(2)

spec = importlib.util.spec_from_file_location("elps_live_harness_under_test", harness_path)
if spec is None or spec.loader is None:
    print(f"[elps_stub_shim] cannot load {harness_path}", file=sys.stderr)
    sys.exit(2)

module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.main()
