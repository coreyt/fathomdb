#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire bahmutov/daily-logs as a `note` corpus.

Source:  github.com/bahmutov/daily-logs, MIT license (verified via
         package.json on pinned revision).
Pinned:  commit SHA 521476da90da3c3f095e458c2b92e8bf379819b7 (2020-09-01).

The repo contains 18 monthly Markdown files (e.g. 2019/03-March-2019.md)
where each "## <Day> YYYY-MM-DD" or "## The Weekend" heading delimits a
daily log. We:

  - Fetch each monthly file at the pinned revision,
  - Split on H2 headings,
  - Emit one CorpusDoc per daily section (the body is the bullet list
    under that heading), preserving @-tags as corpus `tags`,
  - Take the first 300 entries in chronological order (after sorting
    months) to hit the Version-B target.

"redacted" markers stay verbatim — they already obscure the private
info per the upstream README.
"""

from __future__ import annotations

import re
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import CorpusDoc, corpus_data_dir, doc_id, write_jsonl  # noqa: E402

UPSTREAM_REPO = "bahmutov/daily-logs"
UPSTREAM_SHA = "521476da90da3c3f095e458c2b92e8bf379819b7"
PROVENANCE = f"github:{UPSTREAM_REPO}@{UPSTREAM_SHA}"
LICENSE_SPDX = "MIT"
TARGET_COUNT = 300
AUTHOR = "Gleb Bahmutov"

MONTH_FILES = [
    ("2019", "03-March-2019.md"),
    ("2019", "04-April-2019.md"),
    ("2019", "05-May-2019.md"),
    ("2019", "06-June-2019.md"),
    ("2019", "07-July-2019.md"),
    ("2019", "08-August-2019.md"),
    ("2019", "09-September-2019.md"),
    ("2019", "10-October-2019.md"),
    ("2019", "11-November-2019.md"),
    ("2019", "12-December-2019.md"),
    ("2020", "01-January-2020.md"),
    ("2020", "02-February-2020.md"),
    ("2020", "03-March-2020.md"),
    ("2020", "04-April-2020.md"),
    ("2020", "05-May-2020.md"),
    ("2020", "06-June-2020.md"),
    ("2020", "07-July-2020.md"),
    ("2020", "08-August-2020.md"),
]

DATE_RE = re.compile(r"(\d{4})-(\d{2})-(\d{2})")
TAG_RE = re.compile(r"@([a-z][a-z0-9_-]*)")


def fetch_month(year: str, name: str) -> tuple[str, str]:
    url = f"https://raw.githubusercontent.com/{UPSTREAM_REPO}/{UPSTREAM_SHA}/{year}/{name}"
    with urllib.request.urlopen(url) as resp:
        return url, resp.read().decode("utf-8")


def parse_month(year: str, month_name: str, text: str) -> list[tuple[str, str, str]]:
    """Return list of (heading, body, iso_date) for each H2 section."""
    lines = text.splitlines()
    sections: list[tuple[str, list[str]]] = []
    current: tuple[str, list[str]] | None = None
    for line in lines:
        if line.startswith("## "):
            if current is not None:
                sections.append(current)
            current = (line[3:].strip(), [])
        elif current is not None:
            current[1].append(line)
    if current is not None:
        sections.append(current)

    out: list[tuple[str, str, str]] = []
    # Track running date so weekend / unheaded sections inherit a plausible date.
    last_date: str | None = None
    for heading, body_lines in sections:
        body = "\n".join(body_lines).strip()
        if not body:
            continue
        m = DATE_RE.search(heading)
        if m:
            iso = f"{m.group(1)}-{m.group(2)}-{m.group(3)}"
        elif last_date is not None:
            # Heading is like "The Weekend" or just a day name — anchor at the
            # next day after the last dated section.
            try:
                anchor = datetime.fromisoformat(last_date).date()
                iso = anchor.isoformat()
            except ValueError:
                iso = last_date
        else:
            # No date yet — anchor at month start.
            month_idx = int(month_name.split("-")[0])
            iso = f"{year}-{month_idx:02d}-01"
        last_date = iso
        out.append((heading, body, iso))
    return out


def build_doc(heading: str, body: str, iso_date: str, native_id: str) -> CorpusDoc:
    tags = sorted({"tag:" + t for t in TAG_RE.findall(body)})
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, native_id),
        source_type="note",
        title=heading,
        body=body,
        created_at=datetime.fromisoformat(iso_date).replace(tzinfo=timezone.utc).isoformat(),
        modified_at=None,
        author_or_sender=AUTHOR,
        recipients=[],
        people_mentions=[],
        project_mentions=[],
        tags=tags + ["daily-log"],
        url_or_external_id=f"bahmutov:{native_id}",
        thread_id=None,
        parent_doc_id=None,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


def main() -> int:
    out_path = corpus_data_dir() / "raw" / "bahmutov_dailylogs.jsonl"
    docs: list[CorpusDoc] = []

    for year, fname in MONTH_FILES:
        print(f"fetching {year}/{fname}")
        url, text = fetch_month(year, fname)
        sections = parse_month(year, fname, text)
        for idx, (heading, body, iso) in enumerate(sections):
            native_id = f"{year}/{fname}#{idx:02d}"
            docs.append(build_doc(heading, body, iso, native_id))
        if len(docs) >= TARGET_COUNT:
            break

    docs = docs[:TARGET_COUNT]
    count, sha = write_jsonl(out_path, docs)
    print(f"wrote {count} docs to {out_path}")
    print(f"sha256 = {sha}")
    return 0 if count == TARGET_COUNT else 1


if __name__ == "__main__":
    raise SystemExit(main())
