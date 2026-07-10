#!/usr/bin/env python3
"""Decision-Quality (DQ-*) candidate-extractor + corpus probe.

PRIMARY FOCUS: broad decision-quality failures that are OFTEN NOT verbalized as
caught — distinct from the A-E "verbalized-catch" families. For each DQ mode we
implement a deterministic candidate-extractor (textual and/or structural) and
report: hit count, distinct sessions, and up to N truncated snippets (<=300 chars).

HARD RULES honored: streams via parse.py (never reads a whole transcript into an
LLM context); emits only aggregates + short snippets. No git writes; no mutation
of staged data or prior out/.

Structural detectors use turn-threading within a file:
  - an assistant "turn" = one assistant line (may carry text + tool_use blocks)
  - a "turn window" = the assistant line plus a small look-back/look-ahead over
    sibling assistant/tool lines in the same file, bounded by K lines.
"""
import sys, os, re, json, collections
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import parse  # noqa: E402  (reuse the sealed parser)

MANIFEST = "/home/coreyt/transcript-data/manifest.tsv"
SNIP_MAX = 300
PER_MODE_SNIPS = 6

def clip(s, n=SNIP_MAX):
    s = re.sub(r"\s+", " ", s or "").strip()
    return s[:n]

# ---- context-gathering vs action vs verification tool classes ----
READ_TOOLS = {"Read", "Grep", "Glob", "WebFetch", "WebSearch", "NotebookRead"}
VERIFY_TOOLS = READ_TOOLS | {"Bash"}  # Bash counts as verify only if grep/test/cat-ish
ACTION_TOOLS = {"Edit", "Write", "MultiEdit", "NotebookEdit"}
COMMIT_RE = re.compile(r"git\s+commit|git\s+push|cargo\s+publish|maturin|npm\s+publish", re.I)
VERIFY_BASH_RE = re.compile(r"\b(grep|rg|cat|less|head|tail|find|ls|test|pytest|cargo\s+(test|check|clippy)|git\s+(log|blame|show|diff))\b", re.I)

# =====================================================================
# TEXTUAL SIGNAL BANKS (deterministic-textual candidate-extractors)
# =====================================================================
# Each maps mode -> {signal_name: compiled regex}. Kept narrow but honest.

DQ_STALE = {
    "stale_word":     re.compile(r"\b(?:is|now|are|was|were|these are|this is)\s+(?:out[\s-]?of[\s-]?date|stale|outdated|no longer (?:accurate|current|valid|true))\b", re.I),
    "stale_bare":     re.compile(r"\b(?:STALE|out-of-date|out of date)\b"),
    "predates":       re.compile(r"\bpre-?dates?\b", re.I),
    "superseded_doc": re.compile(r"\b(?:doc|plan|design|ADR|spec|note)\s+(?:is\s+)?(?:now\s+)?supersed(?:ed|es)\b", re.I),
    "last_validated": re.compile(r"\blast\s+(?:validated|verified|updated|checked)\b", re.I),
    "as_of_old":      re.compile(r"\bas of (?:20\d\d|the (?:old|prior|previous))\b", re.I),
    "since_then":     re.compile(r"\b(?:has|have)\s+(?:since\s+)?(?:changed|moved|been (?:renamed|removed|refactored))\s+since\b", re.I),
    "reflects_old":   re.compile(r"\b(?:reflects?|describes?|assumes?)\s+(?:the\s+)?(?:old|prior|previous|earlier|pre-\w+)\b", re.I),
}

DQ_ASSUME = {
    "i_assume":     re.compile(r"\bI(?:'ll| will)?\s+assum(?:e|ing)\b", re.I),
    "assuming":     re.compile(r"\bassuming\b", re.I),
    "presumably":   re.compile(r"\bpresumably\b", re.I),
    "should_be":    re.compile(r"\b(?:it|this|that|which)\s+should\s+(?:be|already|have|work|exist)\b", re.I),
    "must_be":      re.compile(r"\b(?:it|this|that)\s+must\s+(?:be|have|already)\b", re.I),
    "probably_alr": re.compile(r"\b(?:probably|likely|presumably)\s+(?:already|exists?|works?|fine|correct|there)\b", re.I),
    "i_expect":     re.compile(r"\bI(?:'d)?\s+expect\b", re.I),
    "i_believe":    re.compile(r"\bI\s+believe\s+(?:it|this|that|the)\b", re.I),
    "take_it_that": re.compile(r"\b(?:I'll take it that|take it as given|assume it's)\b", re.I),
    "presume":      re.compile(r"\bpresum(?:e|ing)\b", re.I),
    "no_need_check":re.compile(r"\bno need to (?:check|verify|confirm|look)\b", re.I),
}

DQ_SHORT_DECISION = {  # decision keywords (used structurally too)
    "decide":   re.compile(r"\b(?:I(?:'ll| will)?\s+)?(?:decide|deciding|the decision is|let's go with|going with|let's do|we'll do)\b", re.I),
    "ratify":   re.compile(r"\bratif(?:y|ies|ied)\b", re.I),
    "approve":  re.compile(r"\b(?:approv(?:e|ed|ing)|sign(?:ed)?[\s-]off|LGTM|green-?light)\b", re.I),
    "land":     re.compile(r"\b(?:let's\s+)?(?:land|merge|ship|commit)\s+(?:it|this|that|the)\b", re.I),
    "delete":   re.compile(r"\b(?:DELETE|remove|drop|rip out|tear out)\s+(?:it|this|that|the|these|all)\b", re.I),
    "choose":   re.compile(r"\b(?:I(?:'ll| will)?\s+)?(?:choose|pick|select|opt for)\b", re.I),
}

DQ_DEPBLIND = {  # retrospective dependency-surprise markers
    "depends_on":    re.compile(r"\b(?:actually\s+)?depends on\b|\bis a dependency of\b", re.I),
    "blocked_by":    re.compile(r"\bblocked by\b|\bis blocking\b", re.I),
    "didnt_account": re.compile(r"\bdidn'?t (?:account for|realize|consider|know (?:that|about))\b", re.I),
    "missed_that":   re.compile(r"\bmissed that\b|\boverlooked that\b|\bdidn'?t catch that\b", re.I),
    "broke_because": re.compile(r"\bbroke because\b|\bbreaks because\b|\bfailed because\b|\bregressed because\b", re.I),
    "forgot_that":   re.compile(r"\bforgot (?:that|to account|about)\b", re.I),
    "requires_y":    re.compile(r"\b(?:X\s+)?requires\s+\w+\b.{0,40}\b(?:first|before|too|as well)\b", re.I),
    "downstream":    re.compile(r"\b(?:downstream|dependent)\s+(?:consumers?|callers?|crates?|modules?)\b", re.I),
    "cross_crate":   re.compile(r"\bcross-crate\b|\bcross-cutting\b|\bripple(?:s|d)? (?:through|into)\b", re.I),
}

DQ_IGNOREDESIGN = {
    "contradicts":   re.compile(r"\bcontradicts?\s+(?:the\s+)?(?:design|architecture|ADR|spec|contract|plan)\b", re.I),
    "against_arch":  re.compile(r"\bagainst\s+(?:the\s+)?(?:design|architecture|ADR|spec|contract)\b", re.I),
    "design_says":   re.compile(r"\b(?:the\s+)?(?:design|ADR|spec|architecture)\s+says\b.{0,60}\bbut\b", re.I),
    "not_per_adr":   re.compile(r"\bnot per (?:the )?(?:ADR|design|spec)\b|\bviolates (?:the )?(?:ADR|design|spec|contract|invariant)\b", re.I),
    "diverges":      re.compile(r"\bdiverges? from (?:the )?(?:design|architecture|plan|spec|contract)\b", re.I),
    "ignores_design":re.compile(r"\bignor(?:e|es|ing)\s+(?:the\s+)?(?:design|architecture|ADR|contract)\b", re.I),
    "off_design":    re.compile(r"\boff[\s-]?(?:the[\s-])?design\b|\bnot (?:in|part of) the design\b", re.I),
}

# OPP-12-style pre-audit design-drift markers (acid-test episode 4)
DQ_NETNEW = {
    "net_new":       re.compile(r"\bnet[\s-]?new\b", re.I),
    "exists_vs":     re.compile(r"\bexists?\s+vs\.?\s+net[\s-]?new\b|\bexists-vs-net-new\b", re.I),
    "contradicts_shipped": re.compile(r"\bcontradict(?:s|ing)?\s+(?:shipped|signed|ratified|the shipped)\b", re.I),
    "already_shipped_mech": re.compile(r"\b(?:already|is)\s+(?:shipped|implemented|signed|ratified)\b.{0,40}\b(?:mechanism|not|already)\b", re.I),
    "reading_as_shipped": re.compile(r"\breads? as (?:near-?)?(?:shipped|done|near-done|already built)\b", re.I),
}

DQ_INCORRECT = {  # only knowable via later reversal / admitted-wrong; overlaps D but decision-scoped
    "was_wrong_decision": re.compile(r"\b(?:the|that|my)\s+(?:decision|call|choice|plan|approach)\s+(?:was|is)\s+(?:wrong|incorrect|a mistake|flawed)\b", re.I),
    "wrong_call":   re.compile(r"\bwrong call\b|\bbad call\b|\bmisjudged\b|\bmiscalled\b", re.I),
    "overturned":   re.compile(r"\bOVERTURNED?\b|\breversed the decision\b|\brolling back the decision\b", re.I),
    "shouldnt_have":re.compile(r"\bshould(?:n'?t| not) have (?:decided|approved|landed|deleted|merged|committed|shipped)\b", re.I),
}

TEXT_BANKS = {
    "DQ-STALE": DQ_STALE,
    "DQ-ASSUME": DQ_ASSUME,
    "DQ-DEPBLIND": DQ_DEPBLIND,
    "DQ-IGNOREDESIGN": DQ_IGNOREDESIGN,
    "DQ-NETNEW(OPP12)": DQ_NETNEW,
    "DQ-INCORRECT": DQ_INCORRECT,
}

# guardrails to suppress obvious meta/rubric self-reference (from prior FP lessons)
META_RE = re.compile(r"\b(?:rubric|detector|criterion|criteria|regex|signal|false positive|FP mechanism|taxonomy|failure mode)\b", re.I)

def session_of(path):
    return parse.parent_session_id(path)

def scan():
    paths = []
    with open(MANIFEST) as fh:
        for line in fh:
            p = line.split("\t", 1)[0].strip()
            if p:
                paths.append(p)

    text_hits = collections.defaultdict(lambda: collections.Counter())      # mode -> signal -> count
    text_sess = collections.defaultdict(lambda: collections.defaultdict(set))
    text_snips = collections.defaultdict(list)                              # mode -> [snippet dicts]
    meta_suppressed = collections.Counter()

    # structural accumulators
    assume_before_action = []   # DQ-ASSUME structural
    short_decision = []         # DQ-SHORTKNOWLEDGE structural
    silent_stall = []           # background-agent stall (episode 3)
    struct_sess = collections.defaultdict(set)

    n_files = 0
    for p in paths:
        n_files += 1
        sess = session_of(p)
        recs = list(parse.iter_file(p))
        # ---- textual pass (assistant lines only; retrospective markers can be any assistant) ----
        for r in recs:
            if r["type"] != "assistant":
                continue
            txt = r["text"] or ""
            if not txt:
                continue
            is_meta = bool(META_RE.search(txt))
            for mode, bank in TEXT_BANKS.items():
                for sig, pat in bank.items():
                    if pat.search(txt):
                        if is_meta and mode in ("DQ-STALE", "DQ-DEPBLIND"):
                            meta_suppressed[mode] += 1
                            continue
                        text_hits[mode][sig] += 1
                        text_sess[mode][sig].add(sess)
                        if len(text_snips[mode]) < PER_MODE_SNIPS:
                            m = pat.search(txt)
                            s = max(0, m.start() - 80)
                            text_snips[mode].append({
                                "sess": sess, "file": os.path.basename(p),
                                "line": r["line_no"], "sig": sig,
                                "snip": clip(txt[s:s + SNIP_MAX])})

        # ---- structural pass: assume-before-action & short-knowledge decision ----
        # Build ordered assistant turns with their tool classes.
        struct_scan(recs, p, sess, assume_before_action, short_decision, struct_sess)

        # ---- structural pass: silent background-agent stall ----
        stall_scan(recs, p, sess, silent_stall)

    return {
        "n_files": n_files,
        "text_hits": text_hits, "text_sess": text_sess, "text_snips": text_snips,
        "meta_suppressed": meta_suppressed,
        "assume_before_action": assume_before_action,
        "short_decision": short_decision,
        "silent_stall": silent_stall,
        "struct_sess": struct_sess,
    }

ASSUME_LOAD = re.compile(r"\b(I(?:'ll| will)?\s+assum|assuming|presumably|it should (?:be|already)|must (?:be|have)|probably already|likely already|I believe (?:it|this|the)|no need to (?:check|verify))", re.I)
DECISION_KW = re.compile(r"\b(?:let's (?:go with|do|land|ship|merge)|going with|I'll (?:delete|remove|land|merge|ship|approve|go with)|DELETE\b|ratif|approv|sign-?off|the decision is|we'll (?:do|go|land))", re.I)

def tool_classes(r):
    """Return (has_read, has_verify, has_action, has_commit) for one assistant line."""
    names = r.get("tool_names") or []
    tin = r.get("tool_input_text") or ""
    has_read = any(n in READ_TOOLS for n in names)
    has_action = any(n in ACTION_TOOLS for n in names)
    has_commit = bool(COMMIT_RE.search(tin))
    has_verify = has_read
    if "Bash" in names and VERIFY_BASH_RE.search(tin):
        has_verify = True
    return has_read, has_verify, has_action, has_commit

def struct_scan(recs, path, sess, assume_out, short_out, struct_sess):
    # ordered assistant lines only
    a = [r for r in recs if r["type"] == "assistant"]
    n = len(a)
    for i, r in enumerate(a):
        txt = r["text"] or ""
        _, has_verify_here, has_action_here, has_commit_here = tool_classes(r)

        # DQ-ASSUME structural: load-bearing assumption phrase, then an ACTION in
        # this or the next <=2 assistant turns, with NO verify tool_use in the
        # window [i-1 .. action]. (assumption drives an unverified action)
        if ASSUME_LOAD.search(txt):
            # look ahead up to 3 assistant turns for an action/commit
            verified = has_verify_here
            action_at = None
            for j in range(i, min(n, i + 4)):
                _, hv, ha, hc = tool_classes(a[j])
                if hv and j > i:  # a verify AFTER the assumption counts as checking
                    verified = True
                if (ha or hc) and j >= i:
                    action_at = j
                    break
            # also look one turn back for a prior verify
            if i > 0:
                _, hvb, _, _ = tool_classes(a[i - 1])
                verified = verified or hvb
            if action_at is not None and not verified:
                m = ASSUME_LOAD.search(txt)
                s = max(0, m.start() - 60)
                if len(assume_out) < 40:
                    assume_out.append({
                        "sess": sess, "file": os.path.basename(path),
                        "line": r["line_no"], "action_turn_gap": action_at - i,
                        "snip": clip(txt[s:s + SNIP_MAX])})
                struct_sess["DQ-ASSUME-struct"].add(sess)

        # DQ-SHORTKNOWLEDGE structural: a decision keyword with FEW context-gathering
        # tool_uses in the preceding window (look back up to 6 assistant turns).
        if DECISION_KW.search(txt):
            look = a[max(0, i - 6):i]
            ctx_reads = sum(1 for x in look if tool_classes(x)[0]
                            or ("Bash" in (x.get("tool_names") or []) and VERIFY_BASH_RE.search(x.get("tool_input_text") or "")))
            if ctx_reads <= 1:  # a big call on <=1 prior context-gathering action
                m = DECISION_KW.search(txt)
                s = max(0, m.start() - 60)
                if len(short_out) < 40:
                    short_out.append({
                        "sess": sess, "file": os.path.basename(path),
                        "line": r["line_no"], "prior_ctx_reads": ctx_reads,
                        "snip": clip(txt[s:s + SNIP_MAX])})
                struct_sess["DQ-SHORT-struct"].add(sess)

SPAWN_RE = re.compile(r"\b(?:commission(?:ed|ing)?|spawn(?:ed|ing)?|launch(?:ed|ing)?|dispatch(?:ed)?|kicked off|delegat(?:e|ed|ing)|background (?:agent|orchestrator|task)|run_in_background|orchestrator)\b", re.I)

def parse_ts(ts):
    if not ts:
        return None
    try:
        from datetime import datetime
        return datetime.fromisoformat(ts.replace("Z", "+00:00"))
    except Exception:
        return None

def stall_scan(recs, path, sess, out):
    """Episode-3 SILENT stall: a spawn/commission event followed by a large
    wall-clock gap to the NEXT line with no intervening progress. Structural,
    no textual catch required."""
    prev = None
    for r in recs:
        ts = parse_ts(r["ts"])
        if prev is not None and ts is not None:
            pts, ptext, pline = prev
            if pts is not None:
                gap_h = (ts - pts).total_seconds() / 3600.0
                if gap_h >= 6.0 and SPAWN_RE.search(ptext or ""):
                    if len(out) < 40:
                        out.append({
                            "sess": sess, "file": os.path.basename(path),
                            "line": pline, "gap_hours": round(gap_h, 1),
                            "snip": clip(ptext)})
        if ts is not None:
            prev = (ts, r["text"], r["line_no"])
    return out

def main():
    R = scan()
    out = {"n_files": R["n_files"], "modes": {}}
    for mode in TEXT_BANKS:
        hits = R["text_hits"][mode]
        sess = R["text_sess"][mode]
        total = sum(hits.values())
        allsess = set()
        for s in sess.values():
            allsess |= s
        out["modes"][mode] = {
            "total_hits": total,
            "distinct_sessions": len(allsess),
            "by_signal": {k: {"hits": v, "sessions": len(sess[k])} for k, v in hits.most_common()},
            "snippets": R["text_snips"][mode],
        }
    out["structural"] = {
        "DQ-ASSUME(assume-before-unverified-action)": {
            "candidates": len(R["assume_before_action"]),
            "distinct_sessions": len(R["struct_sess"]["DQ-ASSUME-struct"]),
            "snippets": R["assume_before_action"][:PER_MODE_SNIPS],
        },
        "DQ-SHORTKNOWLEDGE(decision-on-thin-context)": {
            "candidates": len(R["short_decision"]),
            "distinct_sessions": len(R["struct_sess"]["DQ-SHORT-struct"]),
            "snippets": R["short_decision"][:PER_MODE_SNIPS],
        },
        "DQ-SILENTSTALL(spawn-then-wallclock-gap)": {
            "candidates": len(R["silent_stall"]),
            "snippets": R["silent_stall"][:PER_MODE_SNIPS],
        },
    }
    out["meta_suppressed"] = dict(R["meta_suppressed"])
    print(json.dumps(out, indent=1))

if __name__ == "__main__":
    main()
