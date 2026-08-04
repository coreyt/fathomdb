#!/usr/bin/env python3
"""Check the small set of current public facts against their machine sources."""

import json
import os
import re
import sys
from pathlib import Path


PUBLIC_DOCS = (
    Path("README.md"),
    Path("docs/index.md"),
    Path("docs/getting-started/index.md"),
    Path("docs/install/python.md"),
    Path("docs/install/typescript.md"),
    Path("docs/install/rust.md"),
    Path("docs/compatibility/index.md"),
)
PLATFORM_BOUNDARY_DOCS = PUBLIC_DOCS[:-1] + (Path("docs/compatibility/index.md"),)
NUMBER_WORDS = {
    "zero": 0,
    "one": 1,
    "two": 2,
    "three": 3,
    "four": 4,
    "five": 5,
    "six": 6,
    "seven": 7,
    "eight": 8,
    "nine": 9,
    "ten": 10,
    "eleven": 11,
    "twelve": 12,
}


def repo_root() -> Path:
    override = os.environ.get("REPO_ROOT")
    if override:
        return Path(override).resolve()
    return Path(__file__).resolve().parent.parent


def fail(message: str) -> None:
    print(f"FAIL public-doc-truth: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(root: Path, relative: Path) -> dict:
    try:
        return json.loads((root / relative).read_text())
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read {relative}: {exc}")


def workspace_member_count(cargo_toml: str) -> int:
    match = re.search(r"(?ms)^members\s*=\s*\[(.*?)^\]", cargo_toml)
    if not match:
        fail("Cargo.toml has no workspace members list")
    return len(re.findall(r'^\s*"[^"]+"\s*,?\s*$', match.group(1), re.MULTILINE))


def mentions_published_version(text: str, version: str) -> bool:
    return bool(re.search(rf"(?is)\bv?{re.escape(version)}\b.{{0,100}}\bpublished\b", text))


def has_platform_boundary(text: str) -> bool:
    return "Linux x86_64" in text or "x86_64-unknown-linux-gnu" in text


def main() -> None:
    root = repo_root()
    state = load_json(root, Path("dev/plans/release-state-0.8.20.json"))
    version = state.get("release")
    published = state.get("published")
    if not isinstance(version, str) or not isinstance(published, dict):
        fail("release-state-0.8.20.json must declare release and published")
    if published.get("tag") != f"v{version}":
        fail("release-state published tag must match its release")
    if published.get("npm_dist_tag") != "next":
        fail("release-state must declare npm's next dist-tag")

    manifest = load_json(root, Path("dev/platform-capabilities.json"))
    published_triples = [
        platform.get("triple")
        for platform in manifest.get("platforms", [])
        if platform.get("status") == "published"
    ]
    if published_triples != ["linux-x64-gnu"]:
        fail(f"manifest must declare only linux-x64-gnu as published, got {published_triples}")

    docs: dict[Path, str] = {}
    for relative in PUBLIC_DOCS:
        try:
            docs[relative] = (root / relative).read_text()
        except OSError as exc:
            fail(f"cannot read public document {relative}: {exc}")

    unpublished = re.compile(
        rf"(?is)\bv?{re.escape(version)}\b.{{0,120}}\b(?:is\s+not|has\s+not|not\s+yet)\s+published\b"
    )
    for relative, text in docs.items():
        if unpublished.search(text.replace("**", "").replace("`", "")):
            fail(f"{relative} says published {version} is unpublished")

    for relative in (Path("README.md"), Path("docs/index.md"), Path("docs/getting-started/index.md")):
        if not mentions_published_version(docs[relative], version):
            fail(f"{relative} lacks a current published {version} statement")

    for relative in PLATFORM_BOUNDARY_DOCS:
        if not has_platform_boundary(docs[relative]):
            fail(f"{relative} lacks the linux-x64 published-platform boundary")

    arm64_positive = re.compile(
        r"(?is)\b(?:linux\s+)?(?:aarch64|arm64)(?:-unknown-linux-gnu)?\b"
        r".{0,80}\b(?:is|are|currently|now)\s+(?:published|available|supported)\b"
    )
    for relative, text in docs.items():
        if arm64_positive.search(text):
            fail(f"{relative} advertises aarch64/arm64 as a published native artifact")

    ts_install = docs[Path("docs/install/typescript.md")]
    if "npm install fathomdb@next" not in ts_install:
        fail("docs/install/typescript.md lacks `npm install fathomdb@next`")

    expected_count = workspace_member_count((root / "Cargo.toml").read_text())
    readme = docs[Path("README.md")]
    count_match = re.search(
        r"(?im)^[-*]\s+([A-Za-z]+|\d+)\s+Rust workspace members\b", readme
    )
    if not count_match:
        fail("README.md lacks a Rust workspace member count")
    claimed = count_match.group(1).lower()
    claimed_count = NUMBER_WORDS.get(claimed, int(claimed) if claimed.isdigit() else None)
    if claimed_count != expected_count:
        fail(f"README.md claims {claimed_count} Rust workspace members; Cargo.toml has {expected_count}")

    print(
        f"ok    public-doc-truth: {version} published; "
        f"{published_triples[0]} is the sole published native artifact"
    )


if __name__ == "__main__":
    main()
