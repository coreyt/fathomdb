#!/usr/bin/env python3
"""Deterministic agentic-failure detectors, families A-E.

Each detector consumes the *list of normalized records for ONE file* (so it can
use within-file structure: nearest-prior-assistant, later-reversal-in-session)
and yields candidate rows:
  {detector, family, rubric_ref, file, line_no, ts, session, matched_signal,
   snippet(<=300c), confidence_heuristic}

No LLM is invoked. Pure stdlib + regex. Snippets are truncated to 300 chars.

Family map:
  A  hitl-bounce             HITL rejects/redirects the agent's proposed action  (D1/D4/D7)
  B  unwitnessed-premise     load-bearing topology claim with no cited witness   (premise-substitution)
  C  review-catch            reviewer (codex/code-review/ReportFindings) BLOCK/CONCERN verdict (D5)
  D  self-correction         assistant admits its OWN prior error                (D3)
  E  halt-scout-falsify      scout proves a premise false -> HALT / SCOPE CORRECTED / was WRONG (save mechanism)
"""
import re, os

SNIP = 300


def _snip(s, n=SNIP):
    s = " ".join(s.split())
    return s[:n]


def _session(path):
    b = os.path.basename(path)
    return b[:-6] if b.endswith(".jsonl") else b


# ===========================================================================
# Shared pre-filter: synthetic / injected user-line content that is NOT a
# human HITL turn but structurally looks like one.  Anchored to leading content.
# ===========================================================================
SYNTHETIC = re.compile(
    r"<command-name>|<command-message>|<local-command|<system-reminder>|"
    r"^\s*Stop hook feedback:|coordinator sent a message|The coordinator sent|"
    r"A session-scoped Stop hook is now active|This session is being continued|"
    r"Caveat: The messages below|^\s*Review target:|^\s*Goal set:|"
    r"<local-command-stdout>|\[Request interrupted|"
    r"Base directory for this skill:|<task-notification>|\[SYSTEM NOTIFICATION|"
    r"NOT USER INPUT|<task-id>|<output-file>|"
    r"tool_use ids were found|API Error", re.I)


def is_synthetic(text):
    head = text[:400]
    return bool(SYNTHETIC.search(head))


# ===========================================================================
# FAMILY A -- HITL bounce
# ===========================================================================
A_STRONG = [
    ("no_lead",     re.compile(r"^\s*no[,.](?!\s+(?:problem|worries|need|longer|rush|thanks|prob))", re.I)),
    ("thats_wrong", re.compile(r"\bthat'?s\s+(?:not right|wrong|incorrect|not what|not the)\b", re.I)),
    ("this_wrong",  re.compile(r"\b(?:this is|you'?re)\s+(?:not right|wrong|incorrect|not correct)\b", re.I)),
    ("not_what_i",  re.compile(r"\bnot what i (?:asked|meant|wanted|said)\b", re.I)),
    ("you_failed",  re.compile(r"\byou (?:missed|forgot|didn'?t|failed to|overlooked|skipped|misunderstood|misread|are wrong|were wrong)\b", re.I)),
    ("you_got_wrong", re.compile(r"\byou\s+(?:got|had)\b[^.\n]{0,25}\bwrong\b", re.I)),
    ("reconsider",  re.compile(r"\b(?:reconsider|rethink|think again)\b", re.I)),
    ("step_back",   re.compile(r"\bstep back\b", re.I)),
    ("wrong_axis",  re.compile(r"\bwrong (?:way|approach|file|direction|about)\b", re.I)),
    ("why_did_you", re.compile(r"\bwhy (?:did|are|would) you\b", re.I)),
]

A_SOFT = [
    ("actually_lead", re.compile(r"^\s*actually\b", re.I)),
    ("actually_subj", re.compile(r"\bactually,?\s+(?:i|you|we|the|it|let)\b", re.I)),
    ("reread",      re.compile(r"\bre-?read\b", re.I)),
    ("recheck",     re.compile(r"\bre-?check\b", re.I)),
    ("do_x_instead", re.compile(r"\bdo\s+(?:a|the|an)\b[^.\n]{0,40}\binstead\b", re.I)),
    ("instead",     re.compile(r"\binstead\b", re.I)),
]
INSTEAD_OF = re.compile(r"\binstead of\b", re.I)

A_REVERSAL = [
    ("revert",    re.compile(r"\b(?:revert|undo|roll ?back|back (?:that|it) out)\b", re.I)),
    ("redo",      re.compile(r"\bre-?do (?:it|that|this)\b", re.I)),
    ("throwaway", re.compile(r"\bthrow (?:that|it) (?:away|out)\b", re.I)),
    ("do_instead", re.compile(r"\bdo (?:a|the) [^.\n]{0,30} instead\b", re.I)),
]

A_SCOPE = [
    ("which_intended", re.compile(r"\bwhich (?:way|one) is (?:intended|correct|right)\b", re.I)),
    ("determine_which", re.compile(r"\bdetermine which\b", re.I)),
    ("scope_corrected", re.compile(r"\bSCOPE CORRECTED\b")),
    ("wrong_about", re.compile(r"\bwrong about (?:the )?(?:topology|scope|assumption)\b", re.I)),
    ("is_duplicate", re.compile(r"\bis a duplicate\b", re.I)),
    ("no_live_consumers", re.compile(r"\bno live consumers\b", re.I)),
    ("already_superseded", re.compile(r"\balready superseded\b", re.I)),
    ("n_sites", re.compile(r"~\s?\d{1,4}\s+(?:call\s?)?sites?\b", re.I)),
]
CORRECTION_CO = re.compile(r"\b(?:wrong|not|actually|instead|should|isn'?t|aren'?t|no,)\b", re.I)


def _nearest_prior_assistant(records, idx):
    """Walk back from idx; return the nearest assistant record (skipping
    tool_result user lines). None if none precedes (structural gate)."""
    for j in range(idx - 1, -1, -1):
        r = records[j]
        if r["type"] == "assistant":
            return r
        # a real HITL user turn before us means we're not directly replying to a proposal-chain
        if r["type"] == "user" and r["is_hitl"]:
            return None
    return None


def _first_user_index(records):
    for j, r in enumerate(records):
        if r["type"] == "user" and r["is_hitl"]:
            return j
    return None


MUTATING_TOOL = {"Edit", "Write", "NotebookEdit", "MultiEdit"}
GIT_COMMIT = re.compile(r"git\s+commit|git\s+push|maturin|cargo\s+publish", re.I)


def _prior_had_mutation(records, idx):
    pa = _nearest_prior_assistant(records, idx)
    if pa is None:
        return False
    if any(t in MUTATING_TOOL for t in pa["tool_names"]):
        return True
    if "Bash" in pa["tool_names"] and GIT_COMMIT.search(pa["tool_input_text"]):
        return True
    return False


# F5: one HITL turn must yield ONE candidate. When several A sub-blocks fire on
# the same (file,line_no) turn, keep the single highest-precedence detector so a
# turn is never counted as multiple independent TPs/FPs.
A_PRECEDENCE = {
    "hitl-strong-rejection": 0,
    "hitl-reversal-rework": 1,
    "hitl-scope-topology-correction": 2,
    "hitl-soft-redirect": 3,
    "agent-to-agent-bounce": 4,
}


def _dedupe_hitl(cands):
    best = {}
    for c in cands:
        key = (c["file"], c["line_no"])
        rank = A_PRECEDENCE.get(c["detector"], 99)
        if key not in best or rank < A_PRECEDENCE.get(best[key]["detector"], 99):
            best[key] = c
    # preserve original emission order, one row per turn
    seen = set(); out = []
    for c in cands:
        key = (c["file"], c["line_no"])
        if key in seen:
            continue
        seen.add(key)
        out.append(best[key])
    return out


def detect_hitl(records):
    out = []
    first_u = _first_user_index(records)
    if first_u is None:
        return out
    for idx, r in enumerate(records):
        if r["type"] != "user" or not r["is_hitl"]:
            continue
        text = r["text"]
        if not text or is_synthetic(text):
            continue
        head = text[:500]
        prior_asst = _nearest_prior_assistant(records, idx)
        structural_ok = (idx > first_u) and (prior_asst is not None)
        # subagent coordinator relay classification
        coord = r["is_subfile"] and bool(re.search(r"coordinator sent a message|The coordinator sent", text[:600]))
        # HITL family is MAIN-session only (spec). In subagent files, ONLY emit
        # the agent-to-agent sibling signal for coordinator relays.
        if r["is_subfile"] and not coord:
            continue

        def emit(det, rubric, sig, conf):
            out.append({
                "detector": det, "family": "A-hitl-bounce", "rubric_ref": rubric,
                "file": r["file"], "line_no": r["line_no"], "ts": r["ts"],
                "session": _session(r["file"]), "matched_signal": sig,
                "snippet": _snip(text), "confidence_heuristic": conf,
                "structural_ok": structural_ok, "is_subfile": r["is_subfile"],
            })

        # ---- strong rejection ----
        for name, pat in A_STRONG:
            if pat.search(head):
                conf = "high" if structural_ok else "med"
                if coord:
                    emit("agent-to-agent-bounce", "D4-orchestrator-relay", name, "info")
                else:
                    emit("hitl-strong-rejection", "D1/D7", name, conf)
                break
        # ---- soft redirect ----
        soft_hit = None
        for name, pat in A_SOFT:
            if pat.search(head):
                # disambiguate bare 'instead' comparative
                if name == "instead" and INSTEAD_OF.search(head) and not re.search(r"\binstead\b(?!\s+of)", head, re.I):
                    continue
                soft_hit = name
                break
        # soft redirect: require structural context (replies to a proposal).
        # Non-structural soft hits are dominated by standing resume-prompts
        # ("Re-check … and resume only if cleared") -> drop.
        if soft_hit and not coord and structural_ok:
            emit("hitl-soft-redirect", "D4/D7", soft_hit, "med")
        # ---- reversal / rework ----
        for name, pat in A_REVERSAL:
            if pat.search(head):
                conf = "high" if _prior_had_mutation(records, idx) else ("med" if structural_ok else "low")
                emit("hitl-reversal-rework", "D7", name, conf)
                break
        # ---- scope / topology correction (needs co-occurring correction cue) ----
        for name, pat in A_SCOPE:
            m = pat.search(text)
            if m:
                # 'scope_corrected'/'wrong_about' are self-evident corrections.
                # The rest ('determine_which','which_intended','is_duplicate',
                # 'n_sites', 'no_live_consumers','already_superseded') are too
                # generic (they fire on spawn-prompts / skill docs) -> require a
                # co-occurring correction cue AND structural context.
                auto = name in ("scope_corrected", "wrong_about")
                if auto or (CORRECTION_CO.search(text) and structural_ok):
                    conf = "high" if (auto or structural_ok) else "med"
                    emit("hitl-scope-topology-correction", "D1/D4", name, conf)
                break
    return _dedupe_hitl(out)


# ===========================================================================
# FAMILY B -- unwitnessed premise / premise-substitution
# ===========================================================================
B_PREMISE = {
    "no_consumers": re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+|real\s+|active\s+)?consumers?\b", re.I),
    "no_callers":   re.compile(r"\bno\s+(?:live\s+|other\s+|remaining\s+)?(?:callers?|call\s?sites?|references?|usages?)\b", re.I),
    "already_superseded": re.compile(r"\b(?:already|now)\s+supersed(?:ed|es)\b", re.I),
    "is_a_duplicate": re.compile(r"\bis\s+(?:a\s+)?duplicate\b|\bare\s+duplicates?\b|\bis\s+redundant\b", re.I),
    "owned_by_now": re.compile(r"\bowned\s+by\s+\w+\s+now\b|\bnow\s+owned\s+by\b|\bhandled\s+by\s+\w+\s+now\b", re.I),
    "n_sites": re.compile(r"[~≈]\s?\d{1,4}\s+(?:call\s?)?sites?\b|\b\d{1,4}\s+call\s?sites?\b", re.I),
    "nothing_uses": re.compile(r"\bnothing\s+(?:else\s+)?(?:uses|references|calls|depends\s+on|reads)\b", re.I),
    "safe_to_delete": re.compile(r"\bsafe\s+to\s+(?:delete|remove|drop)\b", re.I),
    "not_used_anywhere": re.compile(r"\bnot\s+used\s+anywhere\b|\bdead\s+code\b|\bnever\s+(?:called|used|referenced)\b", re.I),
}
B_WITNESS = re.compile(
    r"\b(?:grep|rg|ripgrep|search(?:ed|ing)?|confirm(?:ed)?|verif(?:y|ied)|checked)\b"
    r"|[\w./-]+\.(?:rs|py|ts|js|md|toml):\d+|```|`[\w./:-]+:\d+`", re.I)
B_CODE_SUBJ = re.compile(
    r"[\w./-]+\.(?:rs|py|ts|js)\b|\w+::\w+|\b\w+\(\)|"
    r"\b(?:function|fn|method|class|struct|enum|table|column|field|module|crate|"
    r"endpoint|route|handler|migration|schema|trait|struct|interface|component)\b", re.I)
B_ACTION = re.compile(
    r"\b(?:delete|deletes|deleting|deleted|remove|removes|removing|removed|drop|dropping|"
    r"finish|finishing|consolidat|dedup|de-?duplicat|prune|purge|kill|"
    r"skip|skipping|rip\s?out|tear\s?out|collapse|out\s+of\s+scope|no\s+need\s+to)\b", re.I)
B_UNCERTAIN = re.compile(r"\b(?:I\s+think|likely|probably|might|maybe|appears?\s+to|seems?\s+to|assume|not\s+sure|unsure)\b", re.I)
# F7-B: a claim ABOUT hardware ("K620 was never used for compute") is not a
# code-deadness premise — exclude when the subject is a device/compute resource.
B_HARDWARE = re.compile(
    r"\b(?:K620|RTX|3090|GPU|GPUs|CUDA|VRAM|device|devices|compute|CPU|CPUs|core|cores|"
    r"card|cards|hardware|memory\s+bandwidth|display)\b", re.I)
# F7-B: retrospective / RCA prose that QUOTES a past bad premise is analysis, not a
# live load-bearing claim — exclude.
B_RCA_META = re.compile(
    r"\bRCA\b|\bretro(?:spective)?\b|\bpost-?mortem\b|\bin\s+hindsight\b|"
    r"\bthe\s+mistake\s+was\b|\broot\s+cause\b|\blesson\b|\bnever\s+grepped\b|"
    r"\bwe\s+(?:should\s+have|failed\s+to)\b|"
    # N2: retrospective-narrative phrasing that recounts a PAST wrong premise
    # (the 30-N ratify->reverse story) rather than asserting a live claim.
    r"\bgot\s+(?:\w+\s+){0,2}things?\s+wrong\b|\bhad\s+to\s+be\b[^.\n]{0,40}\breplaced\b|"
    r"\bHITL-ratified\b|\borchestrator\s+scouted\b", re.I)
# N2: this suite's OWN rubric/design-doc prose DESCRIBES the premise-substitution
# criteria ("RC-4 (unwitnessed plan premises)", "is not a witness", "criterion C8")
# and thereby quotes the very phrases the detector keys on — a self-referential
# meta-hit about the detectors themselves, not a live load-bearing topology claim.
# Drop when the window is discussing the rubric/detector design.
# B-RUBRIC-META-OVERBROAD fix: keep ONLY genuinely suite-specific phrases. The
# bare tokens 'ratification', 'detector(s)', 'rubric' were removed — 'ratified'/
# 'ratification' is pervasive governance vocabulary in this corpus (RATIFIED/
# HITL-ratified throughout) and 'detector'/'rubric' occur in ordinary prose, so
# matching them bare risked silently dropping a genuine "no consumers -> delete"
# premise merely because governance words sat within the ±200-char window.
B_RUBRIC_META = re.compile(
    r"\bRC-\d\b|\bunwitnessed\s+plan\s+premises?\b|\bis\s+not\s+a\s+witness\b|"
    r"\bpremise-?witness\b|\bnew\s+criteri(?:on|a)\b|"
    r"\bC\d\b\s+(?:fails|gates|criteri)", re.I)
# F7-B: an ENUMERATED file/callsite list in the window IS the witness (a change
# summary that names the sites it touched) — treat as grounded.
B_PATHLIST = re.compile(r"[\w./-]+\.(?:rs|py|ts|js|toml|md)\b")


def detect_unwitnessed_premise(records):
    out = []
    for r in records:
        if r["type"] != "assistant":
            continue
        text = r["text"]
        if not text:
            continue
        # match premise in the assistant's own prose; scan a bounded window
        for name, pat in B_PREMISE.items():
            m = pat.search(text)
            if not m:
                continue
            # window around the claim
            s, e = max(0, m.start() - 200), min(len(text), m.end() + 200)
            win = text[s:e]
            # F7-B: an enumerated file/callsite list is itself a witness (the
            # claimant named the sites), so count >=2 path tokens as grounded.
            enumerated = len(B_PATHLIST.findall(win)) >= 2
            witnessed = bool(B_WITNESS.search(win)) or enumerated
            code_subj = bool(B_CODE_SUBJ.search(win))
            action = bool(B_ACTION.search(win))
            hedged = bool(B_UNCERTAIN.search(win))
            if witnessed:
                continue  # grounded -> not this family
            # F7-B: hardware-usage statement, not code deadness -> drop
            if B_HARDWARE.search(win):
                continue
            # F7-B: retrospective/RCA meta-quote of a past premise -> drop
            if B_RCA_META.search(win):
                continue
            # N2: self-referential rubric/detector-design prose -> drop
            if B_RUBRIC_META.search(win):
                continue
            if not code_subj:
                continue  # pure doc/ledger-supersession vocab -> drop
            # confidence: load-bearing (action-adjacent) + unhedged = high
            if action and not hedged:
                conf = "high"
            elif action or not hedged:
                conf = "med"
            else:
                conf = "low"
            out.append({
                "detector": "unwitnessed-premise", "family": "B-unwitnessed-premise",
                "rubric_ref": "premise-substitution", "file": r["file"],
                "line_no": r["line_no"], "ts": r["ts"], "session": _session(r["file"]),
                "matched_signal": name + ("+action" if action else ""),
                "snippet": _snip(win), "confidence_heuristic": conf,
            })
            break  # one premise hit per assistant turn is enough
    return out


# ===========================================================================
# FAMILY C -- review-catch (reviewer flags a defect)
# ===========================================================================
C_ANCHOR = re.compile(
    r"codex|§9|section 9|code[- ]review|reviewer|review round|ReportFindings|"
    r"\bverdict\b|round[- ]?\d|design review|security[- ]review", re.I)
C_VERDICT = {
    "BLOCK": re.compile(r"\bBLOCK(?:ER|ING|S|ED)?\b"),
    "CONCERN": re.compile(r"\bCONCERN(?:S)?\b"),
    "must_fix": re.compile(r"\bmust[- ]fix\b", re.I),
    "P0P1": re.compile(r"\bP[012]\b"),
    "finding_id": re.compile(r"\bF-?\d{1,3}\b|\bCR-\d+\b|\bR-[A-Z0-9]{1,6}-\d+\b"),
    "CONFIRMED": re.compile(r"\bCONFIRMED\b|\bPLAUSIBLE\b"),
}
REVIEW_TOOLS = re.compile(r"codex|code-?review|ReportFindings|security[- ]review", re.I)
# gate-policy narration (e.g. "BLOCK -> HITL", "CONCERN -> fix-N", "gate on the
# verdict") is NOT a reviewer CATCH of a real defect; suppress to lift precision.
C_POLICY = re.compile(r"BLOCK\s*[→\-/]|CONCERN\s*[→\-/]|→\s*(?:HITL|fix-?N|fix-?\d)|"
                      r"\bgate on\b|\bon its verdict\b|verdict\s*\(", re.I)


def detect_review_catch(records):
    out = []
    for r in records:
        text = r["text"]
        tin = r["tool_input_text"]
        # A review tool_use on THIS (assistant) line. NB: this is used only for
        # anchoring + the echo exemption. It does NOT open a trustworthy "strict"
        # tier — see the C1 note below and formalization.md Known-Gap #3.
        is_review_tool = any(REVIEW_TOOLS.search(t or "") for t in r["tool_names"]) or bool(REVIEW_TOOLS.search(tin))
        blob = text
        if not blob and not is_review_tool:
            continue
        # need a review context anchor unless it's an actual reviewer tool call
        anchored = is_review_tool or bool(C_ANCHOR.search(blob))
        if not anchored:
            continue
        for name, pat in C_VERDICT.items():
            m = pat.search(blob)
            if not m:
                continue
            # require the verdict token to be reasonably near an anchor (or reviewer tool)
            if not is_review_tool:
                a = C_ANCHOR.search(blob)
                if a and abs(a.start() - m.start()) > 400:
                    continue
            # a CATCH = a defect flagged (BLOCK/CONCERN/must-fix/P0/P1/finding); skip lone CONFIRMED
            if name == "CONFIRMED":
                continue
            s, e = max(0, m.start() - 120), min(len(blob), m.start() + 180)
            win = blob[s:e]
            policy = bool(C_POLICY.search(win))
            # F6/C2: code-file / git-log ECHO of a finding-id or "codex §9" provenance
            # mention is NOT a review catch — it is a NUMBERED SOURCE BLOCK the agent is
            # quoting (line-numbered `NNN #|//|*` comment lines, path:line, git
            # short-hash). C2: broadened to catch numbered source-comment blocks that
            # cite a finding-id (e.g. `101 # ... (R-ONNX-2)`) WITHOUT an adjacent
            # codex/§9 token — those slipped through and fired as spurious catches.
            numbered_src = len(re.findall(r"(?:^|\s)\d{2,4}\s+(?:#|//|\*)\s", win)) >= 2
            echo = bool(numbered_src or
                        re.search(r"(?:^|\s)\d{1,4}\s+\S.*\b(?:codex|§9)", win) or
                        re.search(r"\.(?:rs|py|ts|js):\d", win) or
                        re.search(r"\b[0-9a-f]{7,8}\s+\w+\(", win))
            if echo and not is_review_tool:
                continue
            # C1 (RESOLVED via finding option (b)): the previous "strict" channel
            # (channel=="strict" => high) was DEAD CODE — the verdict token lands on
            # the *user tool_result* line whereas is_review_tool is only ever set from
            # the *assistant* tool_use line, so the two never coincided (0/1127). A
            # tool_use_id linkage back to the originating review tool WAS prototyped;
            # it makes the result line reachable, but this corpus's review mechanism
            # is codex-via-Bash emitting free-form prose in which BLOCK/CONCERN/P0/
            # finding-id tokens appear in PASS summaries, spec text (`decide_083 BLOCK
            # on eu7<0.90`), commit lines and code echoes INDISTINGUISHABLY from real
            # catches (~50% FP even under a tight review-invocation + catch-verdict
            # filter). So there is no deterministic "trustworthy strict" tier: ALL
            # C-review-catch is narration-grade. channel is retained as a constant
            # label for downstream compatibility.
            channel = "narration"
            if name in ("BLOCK", "must_fix"):
                conf = "med"          # strong verdict token but only narrated
            else:
                conf = "low" if policy else "med"
            out.append({
                "detector": "review-catch", "family": "C-review-catch",
                "rubric_ref": "D5-reviewer-defect", "file": r["file"],
                "line_no": r["line_no"], "ts": r["ts"], "session": _session(r["file"]),
                "channel": channel,
                "matched_signal": name + ("+tool" if is_review_tool else ("+policy" if policy else "+anchor")),
                "snippet": _snip(win), "confidence_heuristic": conf,
            })
            break
    return out


# ===========================================================================
# FAMILY D -- self-correction (assistant admits its OWN error)
# ===========================================================================
D_PATS = {
    "misunderstood": re.compile(r"\bi\s+(?:mis-?understood|misunderstood)\b", re.I),
    "i_was_wrong": re.compile(r"\bi\s+was\s+wrong\b", re.I),
    "that_was_incorrect": re.compile(r"\b(?:that|this|my earlier)\s+(?:was|is)\s+(?:incorrect|wrong)\b", re.I),
    "misread": re.compile(r"\bi\s+(?:mis-?read|misread)\b", re.I),
    "let_me_correct": re.compile(r"\blet\s+me\s+correct\b", re.I),
    "actually_correct": re.compile(r"\bactually(?:,)?\s+the\s+correct\b", re.I),
    "falsely_assumed": re.compile(r"\bi\s+(?:falsely|wrongly|incorrectly)\s+assumed\b", re.I),
    "closer_inspection": re.compile(r"\bon\s+closer\s+(?:inspection|look|reading)\b", re.I),
    "correcting_my": re.compile(r"\bcorrecting\s+my\b|\bi\s+need\s+to\s+correct\b", re.I),
    "i_apologize": re.compile(r"\bi\s+apologi[sz]e\b|\bmy\s+mistake\b", re.I),
    "you_are_right": re.compile(r"\byou'?re\s+(?:right|correct)\b|\bgood\s+catch\b", re.I),
}
# F7-D: "let me correct" / "correcting my" also fire on ordinary edit intent
# ("let me correct and expand the memory note"). Require an explicit error term
# in the surrounding window for these two to count as a self-error admission.
D_NEEDS_ERROR = {"let_me_correct", "correcting_my"}
D_ERROR_TERM = re.compile(
    r"\b(?:wrong|incorrect|mistake|misread|misunderstood|misstated|conflat\w+|"
    r"error|erroneous|stale|opposite|contradict\w+|inconsistent|duplicat\w+|"
    r"wasn'?t\s+right|not\s+correct|got\s+it\s+wrong)\b", re.I)


def detect_self_correction(records):
    out = []
    for r in records:
        if r["type"] != "assistant":
            continue
        text = r["text"]
        if not text:
            continue
        for name, pat in D_PATS.items():
            m = pat.search(text)
            if not m:
                continue
            s, e = max(0, m.start() - 100), min(len(text), m.start() + 200)
            # F7-D: gate edit-intent phrases on a co-occurring error term.
            if name in D_NEEDS_ERROR and not D_ERROR_TERM.search(text[s:e]):
                continue
            # "you're right" is high-signal (concedes a HITL catch); most others med
            conf = "high" if name in ("i_was_wrong", "you_are_right", "falsely_assumed", "misunderstood") else "med"
            out.append({
                "detector": "self-correction", "family": "D-self-correction",
                "rubric_ref": "D3-agent-self-error", "file": r["file"],
                "line_no": r["line_no"], "ts": r["ts"], "session": _session(r["file"]),
                "matched_signal": name, "snippet": _snip(text[s:e]),
                "confidence_heuristic": conf,
            })
            break
    return out


# ===========================================================================
# FAMILY E -- halt-scout-falsify  (premise proved false -> HALT / SCOPE CORRECTED / was WRONG)
# This is the RCA save-mechanism: the highest-value ratification->reversal pair.
# ===========================================================================
E_HALT = re.compile(r"\bHALT(?:ED|ING)?\b|\bSTOPP?(?:ED|ING)?\b(?=[^a-z])")
E_REVERSAL = re.compile(
    r"\bSCOPE\s+CORRECT(?:ED|ION)\b"
    r"|\b(?:I\s+was|you\s+were|this\s+was|that\s+was|we\s+were|i\s+am)\s+WRONG\b"
    r"|\bi\s+was\s+wrong\s+about\b"
    r"|\bstand\s+corrected\b|\bretract(?:ed|ing|ion)?\b"
    r"|\bthere\s+(?:are|is)\s+(?:still\s+)?(?:live\s+)?(?:consumers?|callers?|references?)\b"
    # NB: dropped the bare "X is still used" clause — it fired on forward work
    # ("check if datetime is still used"); reversals of a no-consumers premise
    # are already covered by "there are still consumers" / "not a duplicate".
    r"|\bnot\s+(?:a\s+)?duplicate\b|\bwasn'?t\s+(?:a\s+)?duplicate\b"
    r"|\bwasn'?t\s+superseded\b|\bstill\s+in\s+use\b", re.I)
E_PREMISE_FALSE = re.compile(
    r"\b(?:premise|assumption|claim)\b[^.\n]{0,60}\b(?:is|was|proved|turned out|being)\b[^.\n]{0,30}\b(?:false|wrong|incorrect|invalid|untrue)\b", re.I)
E_SCOUT = re.compile(r"\bscout(?:ing|ed)?\b|read-?only\s+(?:scout|check|pass)|before\s+(?:i\s+)?(?:delete|remove|change)", re.I)
# F7-E: adversarial-review agents DEBATE a finding's severity rather than reverse
# a premise ("overstates ... into a defect", "only the cost side is half-wrong",
# "is NOT actually wrong or in tension"). These are rebuttals, not self-reversals.
E_REBUTTAL = re.compile(
    r"\bover-?stat\w+\b|\bover-?reach\w*\b|\bover-?claim\w*\b|\bexaggerat\w+\b|"
    r"\btoo\s+strong\b|\bhalf-?wrong\b|\breconcilable\b|\binto\s+a\s+defect\b|"
    r"\bnot\s+actually\s+wrong\b|\bnot\s+(?:a\s+)?(?:real\s+)?(?:defect|bug|issue)\b|"
    r"\bin\s+tension\b|\bstands?\b(?!\s+corrected)|\bdisagree\b|\brebut\w*\b", re.I)
# F7-E: reversal is credible when first-person ("I/we was wrong", "my assumption
# … wrong", "my mistake") or an explicit prior-premise-now-false clause.
E_FIRST_PERSON = re.compile(
    r"\b(?:I\s+was|we\s+were|i\s+am)\s+wrong\b|\bmy\s+(?:assumption|premise|claim|"
    r"path|mistake|earlier)\b|\bi\s+(?:stand\s+corrected|retract)\b|\bmy\s+mistake\b", re.I)


def detect_halt_scout(records):
    out = []
    for r in records:
        if r["type"] != "assistant":
            continue
        text = r["text"]
        if not text:
            continue
        mr = E_REVERSAL.search(text)
        mpf = E_PREMISE_FALSE.search(text)
        mh = E_HALT.search(text)
        if not (mr or mpf):
            continue  # require an actual reversal/falsification, not a bare HALT
        anchor0 = mr or mpf
        # negation guard (widened to 30c): adversarial review agents DEFEND a
        # claim ("is NOT actually wrong", "not that the claim is false").
        # NB: match only real negations — a bare `n'?t\b` also matches the "nt"
        # in words like "markdownli(nt)" and wrongly drops genuine reversals.
        pre = text[max(0, anchor0.start() - 30):anchor0.start()].lower()
        if re.search(r"\bnot\b|\bno\b|\b(?:is|are|was|were|does|do|did|has|have|had|"
                     r"will|would|could|should|ca|wo|ai)n'?t\b", pre):
            continue
        # F7-E: rebuttal/severity-debate guard — an adversarial reviewer arguing a
        # finding is OVERSTATED ("overstates … into a defect", "only the cost side
        # is half-wrong", "not actually wrong or in tension") is NOT a self-reversal.
        gs, ge = max(0, anchor0.start() - 160), min(len(text), anchor0.start() + 160)
        gwin = text[gs:ge]
        if E_REBUTTAL.search(gwin):
            continue
        # F7-E: keep only credible reversals — first-person self-reversal, an
        # explicit premise-now-false clause, or a concrete premise-falsification
        # (still consumers / not a duplicate / still in use). Bare third-person
        # "X was wrong" without any of these is dropped.
        credible = (
            bool(mpf)
            or bool(E_FIRST_PERSON.search(text))
            or bool(re.search(
                r"\bSCOPE\s+CORRECT|there\s+(?:are|is)\s+(?:still\s+)?(?:live\s+)?"
                r"(?:consumers?|callers?|references?)|not\s+(?:a\s+)?duplicate|"
                r"wasn'?t\s+(?:a\s+)?duplicate|wasn'?t\s+superseded|still\s+in\s+use|"
                r"stand\s+corrected|retract", text, re.I)))
        if not credible:
            continue
        # anchor snippet on the strongest hit
        anchor = mr or mpf
        s, e = max(0, anchor.start() - 120), min(len(text), anchor.start() + 200)
        scout = bool(E_SCOUT.search(text)) or bool(mh)
        if mr and re.search(r"\bSCOPE\s+CORRECT|WRONG\b", text):
            conf = "high"
        elif mpf and mh:
            conf = "high"
        else:
            conf = "med"
        sig = []
        if mr:
            sig.append("reversal")
        if mpf:
            sig.append("premise_false")
        if mh:
            sig.append("halt")
        if scout:
            sig.append("scout")
        out.append({
            "detector": "halt-scout-falsify", "family": "E-halt-scout-falsify",
            "rubric_ref": "RCA-save-mechanism", "file": r["file"],
            "line_no": r["line_no"], "ts": r["ts"], "session": _session(r["file"]),
            "matched_signal": "+".join(sig), "snippet": _snip(text[s:e]),
            "confidence_heuristic": conf,
        })
    return out


ALL_DETECTORS = [
    detect_hitl,
    detect_unwitnessed_premise,
    detect_review_catch,
    detect_self_correction,
    detect_halt_scout,
]


def detect_file(records):
    out = []
    for fn in ALL_DETECTORS:
        try:
            out.extend(fn(records))
        except Exception as ex:
            out.append({"detector": fn.__name__, "family": "ERROR", "rubric_ref": "",
                        "file": records[0]["file"] if records else "", "line_no": 0,
                        "ts": "", "session": "", "matched_signal": str(ex)[:100],
                        "snippet": "", "confidence_heuristic": "error"})
    return out
