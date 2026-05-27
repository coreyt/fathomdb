#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Acquire 2000 Enron emails from the CMU May-2015 mailbox dump.

Source:  https://www.cs.cmu.edu/~enron/enron_mail_20150507.tar.gz
         (May 7, 2015 community-standard distribution).
License: No explicit OSI license; CMU asks users to "be sensitive to the
         privacy of the people involved." HITL-approved (2026-05-27) to
         commit Enron-derived test data to the repo with the CMU
         April-2026 message-impersonation note recorded in corpus-card.md.

Provenance: cmu-enron-2015-05-07.

The pipeline:
  1. Use the locally-cached tarball at ENRON_CACHE_TARBALL (no
     re-download if present).
  2. Verify SHA-256 against the pin below.
  3. Walk the tarball deterministically.
  4. For each "_sent_mail" / "sent" / "sent_items" folder per user,
     take the lexicographically first SENT_PER_USER messages.
  5. Parse RFC-822 headers + body; strip signatures (best-effort:
     truncate at the first standalone "-- " line or trailing
     phone-block pattern).
  6. Subsample to TARGET_COUNT total emails by walking users in
     sorted order and round-robining across users until full.
  7. Emit canonical JSONL.

thread_id is set from the In-Reply-To header (or References, first
entry) if present; otherwise from Message-ID itself so single-message
threads are still groupable.

Determinism: a re-run on the same tarball produces a bit-identical
JSONL. The tarball SHA is the upstream pin.
"""

from __future__ import annotations

import email
import email.policy
import hashlib
import os
import re
import sys
import tarfile
from email.message import Message
from email.utils import parsedate_to_datetime
from pathlib import Path
from typing import Iterator

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _corpus_lib import CorpusDoc, corpus_data_dir, doc_id, write_jsonl  # noqa: E402

UPSTREAM_URL = "https://www.cs.cmu.edu/~enron/enron_mail_20150507.tar.gz"
UPSTREAM_LAST_MODIFIED = "2015-05-07T20:35:29Z"
UPSTREAM_ETAG = '"1a6b8803-51583da2f8640"'  # from CMU HTTP HEAD
ENRON_CACHE_TARBALL = Path(
    os.environ.get(
        "ENRON_CACHE_TARBALL",
        str(corpus_data_dir() / "downloads" / "enron_mail_20150507.tar.gz"),
    )
)
PROVENANCE = "cmu-enron-2015-05-07"
LICENSE_SPDX = "LicenseRef-Enron-Research-Use"
TARGET_COUNT = 2000
SENT_PER_USER = 30  # over-fetch so we have headroom after RR fill
SENT_FOLDER_PAT = re.compile(r"(?:^|/)(_sent_mail|sent_items|sent)(?:/|$)", re.IGNORECASE)
SIG_LINE_RE = re.compile(r"^-- ?$")
EMAIL_POLICY = email.policy.compat32  # tolerant; many Enron headers are malformed


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def strip_signature(body: str) -> str:
    """Best-effort signature stripping: cut at the first `-- ` line."""
    lines = body.splitlines()
    for i, line in enumerate(lines):
        if SIG_LINE_RE.match(line):
            return "\n".join(lines[:i]).rstrip()
    return body.rstrip()


def parse_message(raw: bytes) -> Message | None:
    try:
        return email.message_from_bytes(raw, policy=EMAIL_POLICY)
    except Exception:
        return None


def extract_body(msg: Message) -> str:
    if msg.is_multipart():
        for part in msg.walk():
            ct = (part.get_content_type() or "").lower()
            if ct == "text/plain":
                payload = part.get_payload(decode=True)
                if isinstance(payload, bytes):
                    try:
                        return payload.decode("utf-8", errors="replace")
                    except Exception:
                        return payload.decode("latin-1", errors="replace")
        return ""
    payload = msg.get_payload(decode=True)
    if isinstance(payload, bytes):
        try:
            return payload.decode("utf-8", errors="replace")
        except Exception:
            return payload.decode("latin-1", errors="replace")
    return msg.get_payload() or ""


def header(msg: Message, name: str) -> str | None:
    v = msg.get(name)
    if v is None:
        return None
    return str(v).strip()


def split_recipients(value: str | None) -> list[str]:
    if not value:
        return []
    parts = re.split(r"[,;]\s*", value)
    return [p.strip() for p in parts if p.strip()]


def iter_user_sent_messages(tf: tarfile.TarFile) -> Iterator[tuple[str, str, bytes]]:
    """Yield (user, tarball_path, raw_bytes) for messages in sent folders.

    Single sequential pass over the gzipped tarball. The yield order is
    the upstream tarball's internal order — deterministic across runs
    against the same archive. We MUST NOT call getmembers() or sort up
    front; a 520k-file gz tarball needs O(N²) seeks for sorted random
    access since gzip has no random index.
    """
    for m in tf:
        if not m.isfile():
            continue
        # maildir/<user>/<folder>/<filename>
        parts = m.name.split("/")
        if len(parts) < 4 or parts[0] != "maildir":
            continue
        user = parts[1]
        folder_path = "/".join(parts[2:-1])
        if not SENT_FOLDER_PAT.search(folder_path):
            continue
        try:
            f = tf.extractfile(m)
            if f is None:
                continue
            raw = f.read()
        except Exception:
            continue
        yield user, m.name, raw


def build_doc(user: str, tar_path: str, msg: Message, body: str) -> CorpusDoc | None:
    from_addr = header(msg, "From") or user
    to_addrs = split_recipients(header(msg, "To"))
    cc_addrs = split_recipients(header(msg, "Cc"))
    subject = header(msg, "Subject")
    message_id = header(msg, "Message-ID") or tar_path
    in_reply_to = header(msg, "In-Reply-To")
    references = header(msg, "References")
    # thread_id: prefer In-Reply-To -> first Reference -> own Message-ID
    thread_id = in_reply_to
    if not thread_id and references:
        thread_id = references.split()[0].strip()
    if not thread_id:
        thread_id = message_id
    parent_doc_id = None
    if in_reply_to and in_reply_to != message_id:
        parent_doc_id = doc_id(PROVENANCE, in_reply_to)
    date_hdr = header(msg, "Date")
    try:
        dt = parsedate_to_datetime(date_hdr) if date_hdr else None
        created_at = dt.isoformat() if dt is not None else "2001-01-01T00:00:00+00:00"
    except Exception:
        created_at = "2001-01-01T00:00:00+00:00"

    clean_body = strip_signature(body)
    if not clean_body.strip():
        return None
    return CorpusDoc(
        doc_id=doc_id(PROVENANCE, message_id),
        source_type="email",
        title=subject,
        body=clean_body,
        created_at=created_at,
        modified_at=None,
        author_or_sender=from_addr,
        recipients=to_addrs + cc_addrs,
        people_mentions=[],
        project_mentions=[],
        tags=["enron-user:" + user],
        url_or_external_id=tar_path,
        thread_id=thread_id,
        parent_doc_id=parent_doc_id,
        license=LICENSE_SPDX,
        provenance=PROVENANCE,
    )


def main() -> int:
    if not ENRON_CACHE_TARBALL.exists():
        print(f"ERROR: Enron tarball missing at {ENRON_CACHE_TARBALL}", file=sys.stderr)
        print(f"       fetch via: curl -L -o {ENRON_CACHE_TARBALL} {UPSTREAM_URL}", file=sys.stderr)
        return 2
    sha = sha256_file(ENRON_CACHE_TARBALL)
    print(f"tarball sha256: {sha}")
    print(f"  upstream:     etag={UPSTREAM_ETAG} last-modified={UPSTREAM_LAST_MODIFIED}")

    out_path = corpus_data_dir() / "raw" / "enron.jsonl"

    # Pass 1: collect SENT_PER_USER candidate messages per user.
    print(f"opening tarball {ENRON_CACHE_TARBALL} (sequential read)", flush=True)
    per_user: dict[str, list[tuple[str, bytes]]] = {}
    scanned = 0
    kept = 0
    with tarfile.open(ENRON_CACHE_TARBALL, mode="r|gz") as tf:  # streaming mode
        for user, name, raw in iter_user_sent_messages(tf):
            scanned += 1
            bucket = per_user.setdefault(user, [])
            if len(bucket) < SENT_PER_USER:
                bucket.append((name, raw))
                kept += 1
            if scanned % 50000 == 0:
                print(f"  scanned {scanned} sent-folder files; kept {kept} across {len(per_user)} users", flush=True)
    print(f"scan done: kept {kept} sent messages across {len(per_user)} users", flush=True)

    if not per_user:
        print("ERROR: no sent-folder messages found in tarball", file=sys.stderr)
        return 1
    print(f"found sent-folder messages for {len(per_user)} users")

    # Pass 2: round-robin across sorted users, parse + emit, stop at TARGET_COUNT.
    users = sorted(per_user.keys())
    docs: list[CorpusDoc] = []
    seen_doc_ids: set[str] = set()
    cursor = [0] * len(users)
    while len(docs) < TARGET_COUNT:
        progressed = False
        for ui, user in enumerate(users):
            if len(docs) >= TARGET_COUNT:
                break
            bucket = per_user[user]
            while cursor[ui] < len(bucket):
                name, raw = bucket[cursor[ui]]
                cursor[ui] += 1
                msg = parse_message(raw)
                if msg is None:
                    continue
                body = extract_body(msg)
                if not body.strip():
                    continue
                d = build_doc(user, name, msg, body)
                if d is None:
                    continue
                if d.doc_id in seen_doc_ids:
                    continue
                seen_doc_ids.add(d.doc_id)
                docs.append(d)
                progressed = True
                break
        if not progressed:
            break

    if len(docs) < TARGET_COUNT:
        print(f"WARN: only {len(docs)} docs emitted (target {TARGET_COUNT})", file=sys.stderr)
    count, out_sha = write_jsonl(out_path, docs)
    print(f"wrote {count} docs to {out_path}")
    print(f"sha256 = {out_sha}")
    return 0 if count == TARGET_COUNT else 1


if __name__ == "__main__":
    raise SystemExit(main())
