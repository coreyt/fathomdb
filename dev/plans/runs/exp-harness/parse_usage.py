#!/usr/bin/env python3
"""Parse a subagent .output JSONL transcript into real billed-token segments.

Each prompt turn (spawn prompt + each SendMessage follow-up) becomes one segment.
Emits per-segment and total: input / cache_creation / cache_read / output tokens,
cache_hit_ratio, and an Opus-rate $ estimate (rates parameterized).

Usage: parse_usage.py <file.output> [--label NAME] [--json]
"""
import json
import argparse

# Opus 4.x public rates, $/M tokens. Ratios are the robust signal if model differs.
RATES = {"input": 15.0, "output": 75.0, "cache_write": 18.75, "cache_read": 1.50}

def cost(u):
    return (u["input"]*RATES["input"] + u["output"]*RATES["output"]
            + u["cache_creation"]*RATES["cache_write"]
            + u["cache_read"]*RATES["cache_read"]) / 1_000_000

def is_segment_boundary(rec, msg):
    """A new prompt turn starts a segment: the initial spawn prompt (non-meta user),
    or a SendMessage delivery (isMeta user 'coordinator sent a message'). Tool
    results (role user, content array with tool_result) are NOT boundaries."""
    if rec.get("type") != "user" or msg.get("role") != "user":
        return False
    content = msg.get("content")
    if isinstance(content, list):
        if any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content):
            return False
    if rec.get("isMeta"):
        txt = content if isinstance(content, str) else ""
        return "sent a message" in txt
    return True

def snippet(msg, n=60):
    c = msg.get("content")
    if isinstance(c, str):
        t = c
    elif isinstance(c, list):
        t = " ".join(b.get("text","") for b in c if isinstance(b, dict) and b.get("type")=="text")
    else:
        t = ""
    t = " ".join(t.split())
    return t[:n]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--label", default=None)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    segments = []  # list of dicts
    cur = None
    def new_seg(label):
        return {"label": label, "input":0, "cache_creation":0, "cache_read":0,
                "output":0, "assistant_turns":0, "first_ts":None, "last_ts":None}

    with open(args.file) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue
            msg = rec.get("message") or {}
            ts = rec.get("timestamp")
            if is_segment_boundary(rec, msg):
                # close previous, open new
                if cur is not None:
                    segments.append(cur)
                lbl = snippet(msg) or f"seg{len(segments)}"
                lbl = lbl.replace("The coordinator sent a message while you were working:", "[MSG]").strip()
                cur = new_seg(lbl[:33] or f"seg{len(segments)}")
                cur["first_ts"] = ts
            if rec.get("type") == "assistant":
                u = (msg.get("usage") or {})
                if cur is None:
                    cur = new_seg("(pre-prompt)")
                cur["input"] += u.get("input_tokens", 0) or 0
                cur["cache_creation"] += u.get("cache_creation_input_tokens", 0) or 0
                cur["cache_read"] += u.get("cache_read_input_tokens", 0) or 0
                cur["output"] += u.get("output_tokens", 0) or 0
                cur["assistant_turns"] += 1
                cur["last_ts"] = ts
    if cur is not None:
        segments.append(cur)

    # derive
    for s in segments:
        tip = s["input"] + s["cache_creation"] + s["cache_read"]
        s["total_input_processed"] = tip
        s["cache_hit_ratio"] = round(s["cache_read"]/tip, 4) if tip else 0.0
        s["est_cost_usd"] = round(cost(s), 6)

    tot = new_seg("TOTAL")
    for s in segments:
        for k in ("input","cache_creation","cache_read","output","assistant_turns"):
            tot[k] += s[k]
    tip = tot["input"]+tot["cache_creation"]+tot["cache_read"]
    tot["total_input_processed"] = tip
    tot["cache_hit_ratio"] = round(tot["cache_read"]/tip,4) if tip else 0.0
    tot["est_cost_usd"] = round(cost(tot), 6)

    if args.json:
        print(json.dumps({"label":args.label,"file":args.file,"segments":segments,"total":tot}, indent=2))
        return

    name = args.label or args.file.split("/")[-1]
    print(f"== {name} ==")
    hdr = f"{'seg':<34}{'turns':>6}{'input':>9}{'cwrite':>9}{'cread':>9}{'output':>8}{'hit%':>7}{'$':>10}"
    print(hdr)
    for i, s in enumerate(segments):
        print(f"{(str(i)+' '+s['label'])[:33]:<34}{s['assistant_turns']:>6}{s['input']:>9}"
              f"{s['cache_creation']:>9}{s['cache_read']:>9}{s['output']:>8}"
              f"{s['cache_hit_ratio']*100:>6.1f}{s['est_cost_usd']:>10.5f}")
    print("-"*92)
    s = tot
    print(f"{'TOTAL':<34}{s['assistant_turns']:>6}{s['input']:>9}{s['cache_creation']:>9}"
          f"{s['cache_read']:>9}{s['output']:>8}{s['cache_hit_ratio']*100:>6.1f}{s['est_cost_usd']:>10.5f}")

if __name__ == "__main__":
    main()
