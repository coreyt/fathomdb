#!/usr/bin/env python3
"""Resolve the one tracked, active FathomDB release.

The release state is deliberately derived from tracked state/board pairs, never
from a worktree scan: linked worktrees and untracked scratch documents must not
change the release of record.  On success stdout is a tab-separated
``release, board, state`` tuple, suitable for small shell consumers. When every
tracked release is published, success has empty stdout: the schedule is valid
but has no active release to commission.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
STATE_RE = re.compile(r"dev/plans/release-state-([0-9][0-9.]*)\.json$")
BOARD_RE = re.compile(r"dev/plans/runs/STATUS-([0-9][0-9.]*)\.md$")
CLOSED_MARKER = "closed — historical record"


def tracked(pattern: str) -> list[str]:
    result = subprocess.run(
        ["git", "--no-optional-locks", "ls-files", "-z", "--", pattern],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", "replace").strip())
    return sorted(p.decode("utf-8") for p in result.stdout.split(b"\0") if p)


def closed(path: str) -> bool:
    with open(path, encoding="utf-8") as fh:
        return CLOSED_MARKER in "".join(fh.readline() for _ in range(15)).casefold()


def main() -> int:
    try:
        states = tracked(":(glob)dev/plans/release-state-*.json")
        boards = tracked(":(glob)dev/plans/runs/STATUS-*.md")
    except RuntimeError as exc:
        print(f"FAIL release-current: cannot list tracked files: {exc}", file=sys.stderr)
        return 2

    errors: list[str] = []
    state_by_release: dict[str, tuple[str, bool]] = {}
    for state in states:
        match = STATE_RE.fullmatch(state)
        if not match:
            continue
        release = match.group(1)
        try:
            with open(state, encoding="utf-8") as fh:
                data = json.load(fh)
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{state} is not parseable JSON: {exc}")
            continue
        expected_board = f"dev/plans/runs/STATUS-{release}.md"
        if data.get("release") != release or data.get("board") != expected_board:
            errors.append(
                f"inconsistent release link in {state}: filename requires release "
                f"{release!r} and board {expected_board!r}"
            )
            continue
        # A published state is historical even if its retained board has not
        # yet acquired the newer CLOSED banner convention.
        state_by_release[release] = (state, isinstance(data.get("published"), dict))

    board_by_release: dict[str, str] = {}
    for board in boards:
        match = BOARD_RE.fullmatch(board)
        if match:
            board_by_release[match.group(1)] = board

    for release, (state, _) in state_by_release.items():
        expected = f"dev/plans/runs/STATUS-{release}.md"
        if board_by_release.get(release) != expected:
            errors.append(f"inconsistent release link: {state} requires tracked board {expected}")
    for release, board in board_by_release.items():
        # Historical boards predate the release-state writer. They remain valid
        # retained evidence; only a live board must participate in the current
        # release contract.
        if release not in state_by_release and not closed(board):
            errors.append(f"inconsistent release link: tracked board {board} has no valid state file")

    if errors:
        for error in errors:
            print(f"FAIL release-current: {error}", file=sys.stderr)
        return 1

    live = [
        (release, board_by_release[release], state_by_release[release][0])
        for release in sorted(board_by_release)
        if not closed(board_by_release[release]) and not state_by_release[release][1]
    ]
    if not live and state_by_release and all(published for _, published in state_by_release.values()):
        return 0
    if len(live) != 1:
        names = ", ".join(release for release, _, _ in live) or "none"
        print(
            "FAIL release-current: expected exactly one tracked live release; found " + names,
            file=sys.stderr,
        )
        return 1
    print("\t".join(live[0]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
