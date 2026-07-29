"""Artifact writers — `sbom.cdx.json`, `staleness.json`, `staleness.md` (design §5.6, §5.8).

These files, not the in-process objects, are what Slice 33 opens, so the
consumer contract is fixed HERE as well: `staleness.json`'s envelope is exactly
`{generated, source, summary, rows}`, every row carries the Slice-33 field set,
and the rows are in the ruled `(tier, ecosystem, name, locked_version)` order.

Everything is written deterministically — sorted keys, a trailing newline, no
wall-clock stamp — so a recurring re-run diffs to nothing when nothing changed
(REQ-13).
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:  # pragma: no cover - typing only
    from .survey import Survey

__all__ = ["ARTIFACTS", "staleness_document", "staleness_markdown", "write_reports"]

ARTIFACTS = ("sbom.cdx.json", "staleness.json", "staleness.md")


def staleness_document(survey: Survey) -> dict:
    """The `staleness.json` envelope, exactly `{generated, source, summary, rows}`."""
    return {
        "generated": survey.timestamp,
        "source": survey.source,
        "summary": survey.summary(),
        "rows": [row.as_dict() for row in survey.staleness()],
    }


def staleness_markdown(survey: Survey) -> str:
    """A paste-able fragment for Slice 33's findings doc.

    The UNKNOWN count is in the header on purpose: an offline run must not be
    mistakable for a clean run at a glance (§5.4).
    """
    rows = survey.staleness()
    summary = survey.summary()
    lines = [
        "# Dependency staleness — `sbom-survey`",
        "",
        f"**Generated:** {survey.timestamp} · **Source:** {survey.source} ·"
        f" **Components:** {summary['components']}",
        "",
        f"**current:** {summary['current']} · **outdated:** {summary['outdated']} ·"
        f" **ahead:** {summary['ahead']} · **unknown:** {summary['unknown']}"
        f" of {summary['components']}",
        "",
    ]
    if survey.source == "offline":
        lines += [
            "> This was an **offline** run: no registry was consulted, so every row"
            " is `unknown`. An unknown latest is never reported as `current`.",
            "",
        ]
    lines += [
        "| ecosystem | name | tier | depth | locked | latest | status | edit sites |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for row in rows:
        sites = ", ".join(f"`{site}`" for site in row.edit_sites) or "—"
        lines.append(
            f"| {row.ecosystem} | `{row.name}` | {row.tier} | {row.depth} |"
            f" {row.locked_version or '—'} | {row.latest_version or '—'} |"
            f" {row.status} | {sites} |"
        )
    lines += [
        "",
        "## Excluded manifests",
        "",
    ]
    if survey.excluded:
        for excluded in survey.excluded:
            lines.append(f"- `{excluded.path}` — {excluded.reason}")
    else:
        lines.append("- none")
    lines.append("")
    return "\n".join(lines)


def _write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8", newline="\n")


def write_reports(survey: Survey, out_dir: Path | str) -> list[Path]:
    """Write all three artifacts into `out_dir`, creating it if needed."""
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)

    sbom_path = out / "sbom.cdx.json"
    _write(sbom_path, survey.to_cyclonedx_json())

    staleness_path = out / "staleness.json"
    _write(
        staleness_path,
        json.dumps(staleness_document(survey), indent=2, sort_keys=True) + "\n",
    )

    markdown_path = out / "staleness.md"
    _write(markdown_path, staleness_markdown(survey))

    return [sbom_path, staleness_path, markdown_path]
