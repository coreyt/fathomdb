#!/usr/bin/env python3
"""Semantic-neutrality guard for markdown reformatting, using a REAL CommonMark parser.

WHY THIS EXISTS
---------------
A markdown auto-fixer (markdownlint `--fix`, prettier `--write`, etc.) is supposed to change
only *formatting*, never *meaning*. In practice they corrupt fragile constructs: prettier's
non-configurable `*`->`_` reflow breaks multi-line / nested / adjacent-to-`code` emphasis
(broken spans, snake_case `_` loss, word-joins that change tokenization); markdownlint `--fix`
mis-reads `#`-prefixed prose as headings (MD018) and wraps schemeless hosts into broken
autolinks (MD034). See `dev/tools/md-fix-corruption-ledger.md`.

This guard parses the markdown with markdown-it-py (CommonMark + GFM tables) and extracts the
*tokenizer-stable VISIBLE TEXT* an LLM/reader actually sees — emphasis markers are structure,
not text, so a clean `*x*`->`_x_` change is neutral, but a word-join, a broken emphasis span,
a changed code-span body, a changed link target, or a changed fenced-code body all show up.
A regex cannot do this reliably (it stripped `_` and masked real snake_case corruption); a real
parser can. Do NOT replace this with hand-rolled regex.

USAGE
-----
  md_neutrality_guard.py diff OLD.md NEW.md      # exit 0 if meaning-neutral, 3 if changed
  md_neutrality_guard.py diff --old-stdin NEW.md # OLD read from stdin (for "vs pre-fix" checks)
  md_neutrality_guard.py fingerprint FILE.md     # print the JSON fingerprint

Exit codes: 0 neutral · 3 meaning changed · 2 usage/parse error.
"""
import json
import re
import sys

try:
    from markdown_it import MarkdownIt
except ImportError:  # pragma: no cover
    sys.stderr.write("md_neutrality_guard: markdown-it-py not installed (pip install markdown-it-py)\n")
    sys.exit(2)

MD = MarkdownIt("commonmark").enable("table").enable("strikethrough")
HEADING_PUNCT = re.compile(r"[ \t.,:;!?]+$")
WS = re.compile(r"\s+")


def _walk_inline(children, text_out, code_out, link_out):
    link_stack = []
    for t in children:
        ty = t.type
        if ty == "text":
            text_out.append(t.content)
        elif ty == "code_inline":
            code_out.append("`" + WS.sub(" ", t.content).strip() + "`")
            text_out.append("\x00code\x00")
        elif ty in ("softbreak", "hardbreak"):
            text_out.append(" ")
        elif ty == "link_open":
            link_stack.append((dict(t.attrs).get("href", ""), len(text_out)))
        elif ty == "link_close":
            href, start = link_stack.pop() if link_stack else ("", len(text_out))
            vis = "".join(text_out[start:]).strip()
            if href and href.strip() != vis:
                link_out.append(href.strip())
        elif ty == "image":
            link_out.append(dict(t.attrs).get("src", ""))
            if t.children:
                _walk_inline(t.children, text_out, code_out, link_out)
        elif ty == "html_inline":
            pass
        else:
            if t.children:
                _walk_inline(t.children, text_out, code_out, link_out)


def fingerprint(text):
    """Tokenizer-stable meaning fingerprint: visible words + inline-code + link targets + fences."""
    tokens = MD.parse(text)
    text_parts, code_parts, link_parts, fences, htmls = [], [], [], [], []
    pending_heading = False
    for tok in tokens:
        if tok.type == "heading_open":
            pending_heading = True
        elif tok.type in ("fence", "code_block"):
            body = "\n".join(x.rstrip() for x in tok.content.split("\n"))
            fences.append(body.strip("\n"))
            text_parts.append("\n")
        elif tok.type == "html_block":
            # block-level raw HTML (MD033 is enabled — admonitions, <details>, HTML tables):
            # its reader-visible content is meaning. Capture it (ws-collapsed) so tampering
            # inside a <div>…</div> is not silently neutral.
            htmls.append(WS.sub(" ", tok.content).strip())
            text_parts.append("\n")
        elif tok.type == "inline":
            seg_text, seg_code, seg_link = [], [], []
            _walk_inline(tok.children or [], seg_text, seg_code, seg_link)
            seg = "".join(seg_text)
            if pending_heading:
                seg = HEADING_PUNCT.sub("", seg)  # MD026 trailing heading punctuation = neutral
                pending_heading = False
            text_parts.append(seg)
            text_parts.append("\n")
            code_parts.extend(seg_code)
            link_parts.extend(seg_link)
        elif tok.type.endswith("_open") or tok.type.endswith("_close"):
            text_parts.append("\n")
    blob = WS.sub(" ", "".join(text_parts)).strip()
    return {
        "words": [w for w in blob.split(" ") if w],
        "codes": code_parts,
        "links": [x for x in link_parts if x],
        "fences": fences,
        "html": htmls,
    }


DIMS = ("words", "codes", "links", "fences", "html")


def diff(old_text, new_text):
    """Return list of changed dimensions ([] == neutral)."""
    fo, fn = fingerprint(old_text), fingerprint(new_text)
    return [k for k in DIMS if fo.get(k) != fn.get(k)]


def _report(name, old_text, new_text):
    import difflib

    fo, fn = fingerprint(old_text), fingerprint(new_text)
    changed = [k for k in DIMS if fo.get(k) != fn.get(k)]
    if not changed:
        return True
    sys.stderr.write("MEANING CHANGED: %s  [%s]\n" % (name, ",".join(changed)))
    for k in changed:
        sm = difflib.SequenceMatcher(a=fo[k], b=fn[k], autojunk=False)
        for tag, i1, i2, j1, j2 in sm.get_opcodes():
            if tag == "equal":
                continue
            sys.stderr.write("  [%s] %r -> %r\n" % (k, fo[k][i1:i2], fn[k][j1:j2]))
    return False


def main():
    a = sys.argv[1:]
    if a and a[0] == "fingerprint" and len(a) == 2:
        print(json.dumps(fingerprint(open(a[1], encoding="utf-8").read()), ensure_ascii=False))
        return
    if a and a[0] == "diff":
        rest = a[1:]
        if rest and rest[0] == "--old-stdin":
            old = sys.stdin.read()
            new = open(rest[1], encoding="utf-8").read()
            name = rest[1]
        elif len(rest) == 2:
            old = open(rest[0], encoding="utf-8").read()
            new = open(rest[1], encoding="utf-8").read()
            name = rest[1]
        else:
            sys.stderr.write(__doc__)
            sys.exit(2)
        sys.exit(0 if _report(name, old, new) else 3)
    sys.stderr.write(__doc__)
    sys.exit(2)


if __name__ == "__main__":
    main()
