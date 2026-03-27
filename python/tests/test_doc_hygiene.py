from __future__ import annotations

import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "check-doc-hygiene.py"


def write(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")


def run_script(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def test_doc_hygiene_passes_when_tracker_and_checklist_are_aligned(tmp_path: Path) -> None:
    todo = tmp_path / "todo.md"
    checklist = tmp_path / "checklist.md"

    write(
        todo,
        "# TODO\n- [x] done item\n",
    )
    write(
        checklist,
        """# Checklist
## Current Readiness Matrix
| Area | Status | Evidence | Required To Close |
|---|---|---|---|
| Example area | `done` | ok | ok |

## Mandatory Blockers Before A Production Claim
None.

## Strongly Recommended Before Wider Production Use
None.

## Current Overall Assessment
Current assessment: **production-ready within the documented support contract**.
""",
    )

    result = run_script("--todo", str(todo), "--checklist", str(checklist))

    assert result.returncode == 0, result.stderr


def test_doc_hygiene_fails_when_tracker_has_unchecked_boxes(tmp_path: Path) -> None:
    todo = tmp_path / "todo.md"
    checklist = tmp_path / "checklist.md"

    write(todo, "# TODO\n- [ ] pending item\n")
    write(
        checklist,
        """# Checklist
## Current Readiness Matrix
| Area | Status | Evidence | Required To Close |
|---|---|---|---|
| Example area | `done` | ok | ok |

## Mandatory Blockers Before A Production Claim
None.

## Strongly Recommended Before Wider Production Use
None.

## Current Overall Assessment
Current assessment: **production-ready within the documented support contract**.
""",
    )

    result = run_script("--todo", str(todo), "--checklist", str(checklist))

    assert result.returncode != 0
    assert "unchecked tracker items" in result.stderr
