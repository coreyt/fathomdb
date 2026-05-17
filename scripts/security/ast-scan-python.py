#!/usr/bin/env python3
"""AC-050a Python shim scanner shim. Forwards to ast_scan.py."""
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
os.execvp(
    sys.executable,
    [sys.executable, os.path.join(SCRIPT_DIR, "ast_scan.py"), "--language", "python", *sys.argv[1:]],
)
