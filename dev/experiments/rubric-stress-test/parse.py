#!/usr/bin/env python3
"""Reusable transcript-parsing module for the rubric-stress-test detectors.

HARD RULE: nothing here (or in any caller) may Read/cat a whole transcript into
an LLM context. This module streams JSONL line-by-line and yields *normalized*
records. Callers consume only aggregates + short snippets.

Normalized record (dict):
  file          absolute path of the transcript
  line_no       1-indexed line number within the file
  ts            timestamp string (or "")
  type          top-level line type (user|assistant|system|attachment|...)
  role          message.role if present else ""
  is_sidechain  bool (True => subagent turn)
  is_subfile    bool (True => file is a subagent/agent-* file, by PATH)
  is_tool_result  bool (user line carrying tool_result block(s))
  is_hitl       bool (user line, NOT a tool_result => candidate real HITL turn)
  uuid          this line's uuid ("" if absent)
  parent_uuid   parentUuid ("" if absent)
  parent_type   type of the parent line, resolved within-file ("" if unknown)
  text          flattened human-readable text (assistant text blocks joined,
                user string/text joined, toolUseResult text extracted)
  tool_names    list of tool_use names on an assistant line (may be empty)
  tool_input_text  concatenated stringified tool_use inputs (for command/verdict scans)
"""
import json, os, hashlib


def parent_session_id(path):
    """Stable per-SESSION grouping key so every file of one session (parent
    transcript + all subagents + workflow journals) shares a fold. Deriving:
      - <proj>/<uuid>/subagents/agent-*.jsonl -> "<proj>/<uuid>"  (segment before /subagents/)
      - <proj>/<uuid>.jsonl                   -> "<proj>/<uuid>"  (the file's own uuid)
      - tmp-task-outputs/<prefix>__<id>.output-> "task/<basename>" (singleton; these
        volatile /tmp captures carry no recoverable parent session id, and every
        file has a unique prefix, so each is its own group and cannot straddle).
    """
    path = path.rstrip("/")
    if "/subagents/" in path:
        pre = path.split("/subagents/")[0]
        parts = pre.split("/")
        return f"{parts[-2]}/{parts[-1]}" if len(parts) >= 2 else pre
    if path.endswith(".output"):
        return "task/" + os.path.basename(path)
    parts = path.split("/")
    leaf = parts[-1]
    uid = leaf[:-6] if leaf.endswith(".jsonl") else leaf
    return f"{parts[-2]}/{uid}" if len(parts) >= 2 else uid


def candidate_fingerprint(cand):
    """Stable identity fingerprint for label-pinning (F4). Keyed on the parts that
    identify a candidate turn: parent session + file + line + matched signal.
    Confidence tier is EXCLUDED so a confidence re-tiering does not orphan a label.
    Snippet is EXCLUDED too (N1): a pure ±window/regex tweak that leaves the SAME
    (session,file,line,signal) turn a valid detection must NOT orphan a still-valid
    label — including snippet in the key made this round orphan 43% of labels on
    incidental window shifts, breaking like-for-like pre/post precision comparison.
    A genuine relabel need is now signalled by matched_signal or line_no changing
    (the detection moved), not by cosmetic window text."""
    ps = cand.get("parent_session") or parent_session_id(cand.get("file", ""))
    payload = "|".join([ps, cand.get("file", ""), str(cand.get("line_no", "")),
                        cand.get("matched_signal", "")])
    return hashlib.sha1(payload.encode("utf-8", "replace")).hexdigest()


def _blocks_text(content):
    """content may be str | list[block]. Return (joined_text, tool_names, tool_input_text, has_tool_result)."""
    if content is None:
        return "", [], "", False
    if isinstance(content, str):
        return content, [], "", False
    texts, tool_names, tool_inputs = [], [], []
    has_tr = False
    if isinstance(content, list):
        for b in content:
            if not isinstance(b, dict):
                if isinstance(b, str):
                    texts.append(b)
                continue
            bt = b.get("type")
            if bt == "text":
                t = b.get("text")
                if isinstance(t, str):
                    texts.append(t)
            elif bt == "tool_use":
                nm = b.get("name")
                if nm:
                    tool_names.append(nm)
                inp = b.get("input")
                if inp is not None:
                    try:
                        tool_inputs.append(json.dumps(inp, ensure_ascii=False))
                    except Exception:
                        tool_inputs.append(str(inp))
            elif bt == "tool_result":
                has_tr = True
                # tool_result content can itself be str | list[blocks]
                rc = b.get("content")
                if isinstance(rc, str):
                    texts.append(rc)
                elif isinstance(rc, list):
                    for rb in rc:
                        if isinstance(rb, dict) and rb.get("type") == "text":
                            t = rb.get("text")
                            if isinstance(t, str):
                                texts.append(t)
    return "\n".join(texts), tool_names, "\n".join(tool_inputs), has_tr


def _tool_use_result_text(obj):
    """Extract short text from a top-level toolUseResult field (scout output)."""
    tur = obj.get("toolUseResult")
    if tur is None:
        return ""
    if isinstance(tur, str):
        return tur
    if isinstance(tur, dict):
        # common shapes: {stdout,stderr}, {content:[...]}, {output:...}, {text:...}
        for k in ("stdout", "text", "output", "result"):
            v = tur.get(k)
            if isinstance(v, str) and v:
                return v
        c = tur.get("content")
        if isinstance(c, str):
            return c
        if isinstance(c, list):
            out = []
            for b in c:
                if isinstance(b, dict) and b.get("type") == "text":
                    t = b.get("text")
                    if isinstance(t, str):
                        out.append(t)
                elif isinstance(b, str):
                    out.append(b)
            return "\n".join(out)
    return ""


def iter_file(path):
    """Yield normalized records for one transcript file. Two-pass within a file
    so parent_type can be resolved (uuid->type map). Memory is bounded by the
    number of LINES in one file (ids only), never by content size."""
    base = os.path.basename(path)
    is_subfile = base.startswith("agent-") or "/subagents/" in path

    # Pass 1: uuid -> type (cheap; parse minimally)
    uuid_type = {}
    try:
        with open(path, "r", errors="replace") as fh:
            for line in fh:
                line = line.strip()
                if not line or line[0] != "{":
                    continue
                try:
                    o = json.loads(line)
                except Exception:
                    continue
                u = o.get("uuid")
                if u:
                    uuid_type[u] = o.get("type", "")
    except (FileNotFoundError, IsADirectoryError):
        return

    # Pass 2: emit normalized records
    with open(path, "r", errors="replace") as fh:
        for i, line in enumerate(fh, 1):
            line = line.strip()
            if not line or line[0] != "{":
                continue
            try:
                o = json.loads(line)
            except Exception:
                continue
            typ = o.get("type", "")
            msg = o.get("message") or {}
            role = msg.get("role", "") if isinstance(msg, dict) else ""
            content = msg.get("content") if isinstance(msg, dict) else None
            text, tool_names, tool_input_text, has_tr = _blocks_text(content)
            tur_text = _tool_use_result_text(o)
            if tur_text:
                # keep toolUseResult text available but bounded downstream
                text = (text + "\n" + tur_text) if text else tur_text
                has_tr = has_tr or True if typ == "user" else has_tr
            is_tool_result = (typ == "user") and (has_tr or bool(o.get("toolUseResult")))
            is_hitl = (typ == "user") and not is_tool_result
            pu = o.get("parentUuid") or ""
            yield {
                "file": path,
                "line_no": i,
                "ts": o.get("timestamp", ""),
                "type": typ,
                "role": role,
                "is_sidechain": bool(o.get("isSidechain")),
                "is_subfile": is_subfile,
                "is_tool_result": is_tool_result,
                "is_hitl": is_hitl,
                "uuid": o.get("uuid", ""),
                "parent_uuid": pu,
                "parent_type": uuid_type.get(pu, ""),
                "text": text,
                "tool_names": tool_names,
                "tool_input_text": tool_input_text,
            }


def iter_split(split_file):
    """Yield normalized records across every file listed in the split."""
    with open(split_file) as fh:
        paths = [l.strip() for l in fh if l.strip()]
    for p in paths:
        yield from iter_file(p)


def load_paths(split_file):
    with open(split_file) as fh:
        return [l.strip() for l in fh if l.strip()]
