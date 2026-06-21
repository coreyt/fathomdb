#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire LOCOMO (Long-term Conversational Memory) for FathomDB 0.8.3.

Source:  snap-research/locomo on GitHub — ``data/locomo10.json``.
         https://github.com/snap-research/locomo
Paper:   Maharana et al. 2024, "Evaluating Very Long-Term Conversational Memory
         of LLM Agents", ACL 2024 (arXiv:2402.17753).
License: **CC-BY-NC-4.0** — NON-COMMERCIAL. EVAL-ONLY. The payload is written
         under data/corpus-data/ (gitignored) and is NEVER committed, NEVER
         shipped in the library. This is the same EVAL-ONLY footprint posture as
         the priced answerer. License text saved alongside as
         ``locomo10.LICENSE.txt``.

Role:    Second real agentic-memory source for the 0.8.3 Mem0-parity corpus. The
         Slice-5 re-pin showed multi_session/temporal underpowered on
         LongMemEval alone (the real-question pool is exhausted at ~500 total);
         LOCOMO adds 281 multi-hop + 321 temporal + 841 single-hop REAL questions
         on the underpowered classes (see eval/locomo_loader.py for the
         category->class map and the paired-power-proxy adequacy result).

Dataset: 10 conversations, 1,986 QA total; categories
         {1 multi-hop, 2 temporal, 3 open-domain, 4 single-hop, 5 adversarial}.
         Images are NOT released (URLs/captions only) — not used here (text-only).
"""

from __future__ import annotations

import hashlib
import json
import sys
import urllib.request
from pathlib import Path

_RAW = "https://raw.githubusercontent.com/snap-research/locomo/main"
_OUT = Path("data/corpus-data/raw/locomo10.json")
_LICENSE_OUT = Path("data/corpus-data/raw/locomo10.LICENSE.txt")
_EXPECT_CONVERSATIONS = 10
_EXPECT_QA = 1986


def _download(url: str, dest: Path) -> bytes:
    dest.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(url, timeout=120) as resp:  # noqa: S310 (pinned raw.githubusercontent host)
        data = resp.read()
    dest.write_bytes(data)
    return data


def main() -> int:
    print(f"[acquire_locomo] downloading {_RAW}/data/locomo10.json", file=sys.stderr)
    data = _download(f"{_RAW}/data/locomo10.json", _OUT)
    _download(f"{_RAW}/LICENSE.txt", _LICENSE_OUT)

    convs = json.loads(data)
    n_conv = len(convs)
    n_qa = sum(len(c.get("qa", [])) for c in convs)
    sha = hashlib.sha256(data).hexdigest()
    print(
        f"[acquire_locomo] {n_conv} conversations, {n_qa} QA, "
        f"sha256={sha[:16]}… ({_OUT})",
        file=sys.stderr,
    )
    if n_conv != _EXPECT_CONVERSATIONS or n_qa != _EXPECT_QA:
        print(
            f"[acquire_locomo] WARNING: expected {_EXPECT_CONVERSATIONS} conv / "
            f"{_EXPECT_QA} QA, got {n_conv}/{n_qa} — upstream may have changed",
            file=sys.stderr,
        )
    print("[acquire_locomo] OK — EVAL-ONLY, gitignored, do not commit the payload", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
