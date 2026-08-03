#!/usr/bin/env bash
# Select the checkout-owned Python virtualenv for shell-suite subprocesses.

# Usage: use_checkout_venv_python_path <checkout-root>
#
# `scripts/bootstrap.sh` installs Python developer dependencies into the
# checkout-local `.venv`. Shell suites deliberately invoke `python3` directly,
# so make that interpreter visible before the test harness registers any suite.
# A missing or incomplete venv leaves PATH untouched, preserving the existing
# system-Python fallback without mutating or rebinding any virtualenv.
use_checkout_venv_python_path() {
  local checkout_root="$1"
  local venv_bin="$checkout_root/.venv/bin"

  if [ -x "$venv_bin/python" ]; then
    case ":${PATH:-}:" in
      *":$venv_bin:"*) ;;
      *) PATH="$venv_bin${PATH:+:$PATH}" ;;
    esac
    export PATH
  fi
}
