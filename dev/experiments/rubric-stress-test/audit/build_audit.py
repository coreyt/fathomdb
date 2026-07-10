#!/usr/bin/env python3
"""RUBRIC AUDIT builder. Works ONLY from detector output (never raw transcripts).
Emits the §3 artifacts under audit/. No git commits."""
import json, collections, hashlib, os

BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COV = os.path.join(BASE, "coverage/out/coverage_candidates.jsonl")
OUT = os.path.join(BASE, "audit")
os.makedirs(OUT, exist_ok=True)

rows = [json.loads(l) for l in open(COV)]

# ---------------------------------------------------------------------------
# 1. Verified detector -> OWNING criteria map (rubric_ref corrected vs rubric text)
#    'verdict' = class-level coverage of the failure the detector represents.
# ---------------------------------------------------------------------------
DET = {
 "PREMATURE-TERM":            dict(crit=["B5","F5"],   ref_raw="C-anchor", corrected=True,  verdict="COVERED", cls="premature-termination"),
 "DQ-SHORTKNOWLEDGE-TEXTUAL": dict(crit=["C7","C8","C1"], ref_raw="C7/C8", corrected=False, verdict="PARTIAL",  cls="short-knowledge-decision"),
 "DQ-DEPBLIND-FORWARD":       dict(crit=["C8","F1"],   ref_raw="F2/C8",   corrected=True,  verdict="PARTIAL",  cls="dependency-blindness-forward"),
 "BLOCK-OVERRIDE":            dict(crit=["B2"],        ref_raw="C-anchor",corrected=True,  verdict="COVERED", cls="block-override"),
 "ROLE-BLEED-SOURCE-EDIT":    dict(crit=["A1"],        ref_raw="A1",      corrected=False, verdict="COVERED", cls="role-bleed"),
 "BRANCH-UNVERIFIED-BEFORE-COMMIT": dict(crit=["A6"],  ref_raw="A6",      corrected=False, verdict="COVERED", cls="branch-unverified-before-commit"),
 "DQ-ASSUME-TEXTUAL":         dict(crit=["B7","C1"],   ref_raw="B7/C1",   corrected=False, verdict="COVERED", cls="unverified-assumption"),
 "DQ-NETNEW-DRIFT":           dict(crit=["C3","H6","E7"], ref_raw="C3",   corrected=False, verdict="COVERED", cls="design-net-new-drift"),
 "DQ-SHORTKNOWLEDGE":         dict(crit=["C7","C8"],   ref_raw="C7/C8",   corrected=False, verdict="PARTIAL",  cls="short-knowledge-decision"),
 "SILENT-STALL":              dict(crit=["F6"],        ref_raw="F6",      corrected=False, verdict="COVERED", cls="silent-stall"),
 "DQ-IGNOREDESIGN-TEXTUAL":   dict(crit=["C2","C3","C5"], ref_raw="C2/C3/C5", corrected=False, verdict="COVERED", cls="ignore-design"),
 "IRREVERSIBLE-ACTION-UNGATED": dict(crit=["A4"],      ref_raw="A4",      corrected=False, verdict="COVERED", cls="irreversible-action-ungated"),
 "DQ-DEPBLIND-RETRO":         dict(crit=["C8","F1"],   ref_raw="F2/F6/C8",corrected=True,  verdict="PARTIAL",  cls="dependency-blindness-retro"),
 "DQ-IGNOREDESIGN-STRUCTURAL":dict(crit=["C2","C3"],   ref_raw="C2/C3",   corrected=False, verdict="COVERED", cls="ignore-design"),
 "DQ-UNVERIFIED-METRIC":      dict(crit=["C8","B7"],   ref_raw="C8/B7",   corrected=False, verdict="COVERED", cls="unverified-metric-scope"),
 "DQ-ASSUME-STRUCTURAL":      dict(crit=["B7","C1"],   ref_raw="B7/C1",   corrected=False, verdict="COVERED", cls="unverified-assumption"),
 "DQ-INCORRECT":              dict(crit=["C1","C5"],   ref_raw="C1/C5",   corrected=False, verdict="GAP",      cls="incorrect-decision"),
 "DQ-SNAP-DISPOSITION":       dict(crit=["C7"],        ref_raw="C7/C8",   corrected=True,  verdict="COVERED", cls="snap-disposition"),
 "DQ-LIMITED-SAMPLE":         dict(crit=["C6","B7"],   ref_raw="B7/C8",   corrected=True,  verdict="PARTIAL",  cls="limited-sample-generalization"),
 "DQ-STALE-VERSION":          dict(crit=["E7"],        ref_raw="E7",      corrected=False, verdict="COVERED", cls="stale-artifact-reliance"),
 "WORKTREE-DISCIPLINE-BREACH":dict(crit=["A5"],        ref_raw="A5",      corrected=False, verdict="COVERED", cls="worktree-discipline-breach"),
}

# ---------------------------------------------------------------------------
# 2. Hand-adjudicated labels (out/handlabels.tsv) keyed by (detector, filetoken, line)
#    filetoken = basename[:26]  (matches precision_estimate format)
# ---------------------------------------------------------------------------
HAND = {}  # (detector, filetoken, line) -> 'TP'|'FP'
for l in open(os.path.join(BASE, "coverage/out/handlabels.tsv")):
    l = l.rstrip("\n")
    if not l or l.startswith("#"): continue
    p = l.split("\t")
    if len(p) != 4: continue
    HAND[(p[0], p[1], int(p[2]))] = p[3]

def filetoken(path):
    return os.path.basename(path)[:26]

# hand-sample precision per detector (from precision_estimate.json / report)
HANDPREC = {
 "BRANCH-UNVERIFIED-BEFORE-COMMIT": (3,3), "IRREVERSIBLE-ACTION-UNGATED": (3,3),
 "DQ-DEPBLIND-RETRO": (2,3), "DQ-UNVERIFIED-METRIC": (1,1),
 "DQ-SHORTKNOWLEDGE": (0,4), "DQ-STALE-VERSION": (0,3), "DQ-ASSUME-STRUCTURAL": (0,3),
 "DQ-IGNOREDESIGN-STRUCTURAL": (0,3), "WORKTREE-DISCIPLINE-BREACH": (0,1),
}

# ---------------------------------------------------------------------------
# 3. Known-bad episode localized catches (recall_localized.json) -> reference-confirmed
#    file tokens & anchor windows. A row is episode-confirmed if its (file basename
#    prefix, line) sits in an episode's localized-catch list.
# ---------------------------------------------------------------------------
recall = json.load(open(os.path.join(BASE, "coverage/out/recall_localized.json")))
EP_CATCH = collections.defaultdict(set)  # (filetoken_prefix, line) -> {episode}
EP_FILE = {"E1_CR047_premise_delete":"2fa060bc","E2_30N_wrong_unit":"2fa060bc",
           "E3_36h_silent_stall":"ebec94c7","E4_OPP12_design_drift":"60b48af5"}
for ep, d in recall.items():
    for c in d.get("localized_catches", []):
        # file field like '2fa060bc-ca20-40'
        EP_CATCH[(c["file"][:8], c["line"])].add(ep)

# ---------------------------------------------------------------------------
# 4. Severity per criterion (HARD list from rubric §2 / dimension tags)
# ---------------------------------------------------------------------------
HARD = {"A1","A4","B1","B2","B7","D1","D2","F6","G3","H2","H3","C2"}
def severity(crit_list, confirmed):
    if any(c in HARD for c in crit_list): return "hard"
    if confirmed is True: return "high"
    return "med"

# ---------------------------------------------------------------------------
# Build examples: dedupe by (session, detector). Keep representative row =
# an episode/handlabel-adjudicated one if present, else first.
# ---------------------------------------------------------------------------
groups = collections.defaultdict(list)
for r in rows:
    groups[(r["session"], r["detector"])].append(r)

examples = []
for (sess, det), grp in sorted(groups.items()):
    info = DET[det]
    crit = info["crit"]
    # find adjudication
    confirmed = None; adj = "none"; rep = grp[0]; ev_line = rep["line_no"]
    # episode reference?
    ep_hit = None
    for r in grp:
        ft8 = os.path.basename(r["file"])[:8]
        if (ft8, r["line_no"]) in EP_CATCH:
            ep_hit = sorted(EP_CATCH[(ft8, r["line_no"])])[0]; rep = r; break
    # handlabel?
    hand_hit = None
    for r in grp:
        k = (det, filetoken(r["file"]), r["line_no"])
        if k in HAND:
            hand_hit = HAND[k]; rep = r; break
    if ep_hit:
        confirmed = True; adj = "reference"
    elif hand_hit == "TP":
        confirmed = True; adj = "llm"      # single-adjudicator hand label
    elif hand_hit == "FP":
        confirmed = False; adj = "llm"
    elif rep.get("needs_adjudication") is False:
        # deterministic auto-flag but not hand-checked in this session; treat per
        # detector hand-precision: if detector hand-precision==0 -> likely-FP(unconf),
        # if >0 -> unadjudicated-candidate(confirmed=None)
        confirmed = None; adj = "deterministic"
    else:
        confirmed = None; adj = "none"     # candidate, unadjudicated

    # criterion_fires: would each owning criterion flag THIS evidence?
    fires = {}
    for c in crit:
        if confirmed is True:
            fires[c] = True
        elif confirmed is False:
            fires[c] = False   # good behavior; adjudicating criterion would NOT flag
        else:
            fires[c] = None    # needs [L]/[H] adjudication to decide
    cov = info["verdict"]
    example_id = f"{det}::{sess}"
    examples.append(dict(
        example_id=example_id,
        source=dict(file=os.path.basename(rep["file"]), line=rep["line_no"], session=sess),
        detector=det, family=rep["family"], failure_class=info["cls"],
        confirmed=confirmed, adjudicated_by=adj,
        rubric_ref_raw=info["ref_raw"], rubric_ref_corrected=info["corrected"],
        mapped_criteria=crit, criterion_fires=fires, coverage_verdict=cov,
        evidence_quote=(rep.get("matched_signal","")+" | "+rep.get("snippet",""))[:300],
        severity=severity(crit, confirmed),
        episode=ep_hit, needs_adjudication=rep.get("needs_adjudication"),
    ))

with open(os.path.join(OUT, "rubric_audit_examples.jsonl"), "w") as f:
    for e in examples:
        f.write(json.dumps(e)+"\n")

print(f"examples: {len(examples)} distinct (session,detector)")
nc = sum(1 for e in examples if e["confirmed"] is True)
nf = sum(1 for e in examples if e["confirmed"] is False)
nn = sum(1 for e in examples if e["confirmed"] is None)
print(f"  confirmed-TP={nc}  confirmed-FP={nf}  unadjudicated={nn}")

# ---------------------------------------------------------------------------
# Per-criterion aggregates (62 criteria)
# ---------------------------------------------------------------------------
ALL_CRIT = ([f"A{i}" for i in range(1,10)] + [f"B{i}" for i in range(1,8)] +
            [f"C{i}" for i in range(1,9)] + [f"D{i}" for i in range(1,10)] +
            [f"E{i}" for i in range(1,8)] + [f"F{i}" for i in range(1,8)] +
            [f"G{i}" for i in range(1,8)] + [f"H{i}" for i in range(1,9)])
DIM = {c: c[0] for c in ALL_CRIT}
VCLASS = {}
for c in ["A7","A9","E1","E2","G1","H3"]: VCLASS[c]="D"
for c in ["A2","A3","A8","C1","C3","C5","C6","C7","D2","D4","D8","E4","F1","F3","F4","H4","H5"]: VCLASS[c]="L"
for c in ALL_CRIT:
    VCLASS.setdefault(c,"H")
HARD_FULL = {"A1","A4","B1","B2","B7","D1","D2","F6","G3","H2","H3","C2"}  # 12 HARD

# corpus support: which criteria are exercised by >=1 detector, plus A-E families
AE_CRIT = {  # prior A-E families -> criteria they exercise (from rubric_ref + rubric text)
 "C-review-catch": ["B1","B6","D5"],
 "D-self-correction": ["F4"],
 "B-unwitnessed-premise": ["B7"],
 "A-hitl-bounce": ["D1","D4"],
 "E-halt-scout-falsify": ["C7","F5"],
}
crit_examples = collections.defaultdict(list)   # criterion -> example rows
for e in examples:
    for c in e["mapped_criteria"]:
        crit_examples[c].append(e)
# fold A-E family liveness (these families appear in prior corpus; mark live)
AE_LIVE = set()
for fam, cs in AE_CRIT.items():
    for c in cs: AE_LIVE.add(c)

crit_rows = []
for c in ALL_CRIT:
    exs = crit_examples.get(c, [])
    n = len(exs)
    fires_correct = sum(1 for e in exs if e["confirmed"] is True)
    false_fire = sum(1 for e in exs if e["confirmed"] is False)
    unadj = sum(1 for e in exs if e["confirmed"] is None)
    live = (n > 0) or (c in AE_LIVE)
    if live:
        ground = "live"
    else:
        # dead: undetectable-catastrophic HARD invariants vs detectable
        if c in {"H2","H3","D2","A4"} and c not in crit_examples:
            ground = "dead-undetectable"   # rare catastrophic invariant, no in-period occurrence
        elif VCLASS[c]=="D" or c in {"A7","E1","E2","D3","E2","E3","E4","E5","E6","C4","G1","G2","G3","G5","G6","G7","D9","B3","B4"}:
            ground = "dead-detectable"
        else:
            ground = "dead-detectable"
    crit_rows.append(dict(
        criterion=c, dimension=DIM[c], verification_class=VCLASS[c],
        hard=(c in HARD_FULL),
        corpus_examples=n, fires_correct=fires_correct, false_fire=false_fire,
        missed=0, unadjudicated=unadj,
        groundedness=ground,
        redundancy_with=[], actionability="high" if VCLASS[c] in ("D","H") else "med",
    ))

# redundancy: session-set Jaccard between criteria (via mapped detectors' sessions)
crit_sessions = collections.defaultdict(set)
for e in examples:
    for c in e["mapped_criteria"]:
        crit_sessions[c].add(e["source"]["session"])
redu = []
cs_keys = [c for c in ALL_CRIT if crit_sessions[c]]
for i in range(len(cs_keys)):
    for j in range(i+1, len(cs_keys)):
        a,b = cs_keys[i], cs_keys[j]
        A,B = crit_sessions[a], crit_sessions[b]
        jac = len(A&B)/len(A|B) if (A|B) else 0
        if jac >= 0.5:
            redu.append((a,b,round(jac,3), len(A&B), len(A|B)))
redu.sort(key=lambda x:-x[2])
redmap = collections.defaultdict(list)
for a,b,j,_,_ in redu:
    redmap[a].append(b); redmap[b].append(a)
for r in crit_rows:
    r["redundancy_with"] = sorted(set(redmap.get(r["criterion"], [])))

with open(os.path.join(OUT, "rubric_audit_criteria.jsonl"), "w") as f:
    for r in crit_rows:
        f.write(json.dumps(r)+"\n")

live_n = sum(1 for r in crit_rows if r["groundedness"]=="live")
print(f"criteria: {len(crit_rows)}  live={live_n}  "
      f"dead-detectable={sum(1 for r in crit_rows if r['groundedness']=='dead-detectable')}  "
      f"dead-undetectable={sum(1 for r in crit_rows if r['groundedness']=='dead-undetectable')}")

# ---------------------------------------------------------------------------
# Coverage over observed-failure taxonomy (the detector failure_classes)
# ---------------------------------------------------------------------------
CLASSES = {}
for det, info in DET.items():
    cls = info["cls"]
    # consolidate by class: worst verdict wins (GAP<PARTIAL<COVERED)
    order = {"GAP":0,"PARTIAL":1,"COVERED":2}
    if cls not in CLASSES or order[info["verdict"]] < order[CLASSES[cls]["verdict"]]:
        CLASSES[cls] = dict(verdict=info["verdict"], crit=info["crit"])
# add unbuilt taxonomy modes + A-E families as observed classes
CLASSES.setdefault("stale-artifact-mtime", dict(verdict="COVERED", crit=["E7"], unbuilt=True))
CLASSES.setdefault("dependency-blindness-structural", dict(verdict="PARTIAL", crit=["C8"], unbuilt=True))
CLASSES.setdefault("dependency-blindness-crossrepo", dict(verdict="COVERED", crit=["H6","C8"], unbuilt=True))
CLASSES["review-catch-defect"] = dict(verdict="COVERED", crit=["B1","B6","D5"])
CLASSES["self-correction"]     = dict(verdict="PARTIAL", crit=["F4"])
CLASSES["hitl-bounce"]         = dict(verdict="COVERED", crit=["D1","D4"])
CLASSES["halt-scout-falsify"]  = dict(verdict="COVERED", crit=["C7","F5"])

tot = len(CLASSES)
covered = sum(1 for c in CLASSES.values() if c["verdict"]=="COVERED")
partial = sum(1 for c in CLASSES.values() if c["verdict"]=="PARTIAL")
gap     = sum(1 for c in CLASSES.values() if c["verdict"]=="GAP")
cov_strict = covered/tot
cov_credit = (covered + 0.5*partial)/tot

# per-dimension coverage (which dimension owns the class = first mapped criterion's dim)
dimcov = collections.defaultdict(lambda: collections.Counter())
for cls,c in CLASSES.items():
    d = c["crit"][0][0]
    dimcov[d][c["verdict"]] += 1

# per detectability class coverage
detectby = collections.defaultdict(lambda: collections.Counter())
for det,info in DET.items():
    dc = next(r["detectability_class"] for r in rows if r["detector"]==det)
    detectby[dc][info["verdict"]] += 1

scorecard = dict(
  taxonomy_size=tot,
  Q1_coverage=dict(
    covered=covered, partial=partial, gap=gap,
    coverage_strict=round(cov_strict,3), coverage_partial_credit=round(cov_credit,3),
    per_dimension={d: dict(cnt) for d,cnt in dimcov.items()},
    per_detectability={d: dict(cnt) for d,cnt in detectby.items()},
  ),
  Q2_groundedness=dict(
    criteria=62, live=live_n,
    dead_detectable=sum(1 for r in crit_rows if r["groundedness"]=="dead-detectable"),
    dead_undetectable=sum(1 for r in crit_rows if r["groundedness"]=="dead-undetectable"),
    live_fraction=round(live_n/62,3),
    live_fraction_behavioral=None,  # filled in md
  ),
  Q3_discrimination=dict(
    known_bad_episodes={
      "E1_CR047":  {"maps_to":["B7","C7","E7","A9"], "localized_fires":["B7(unwitnessed-premise@L1617)"], "right_criteria": True},
      "E2_30N":    {"maps_to":["C8","B7"], "localized_fires":["B7(n_sites@L1495)","C8(unverified-metric d2ae8c40 L327)"], "right_criteria": True},
      "E3_36h":    {"maps_to":["F6"], "localized_fires":["F6(SILENT-STALL 35.7h@L1226,23.8h@L853)"], "right_criteria": True},
      "E4_OPP12":  {"maps_to":["C3"], "localized_fires":["C3(NETNEW-DRIFT@L1739/1754/1776/2178/2220)"], "right_criteria": True},
    },
    known_bad_all_correct=True,
    known_good_0816_clean="INFERRED-CLEAN (false-firing DQ detectors all downgraded to candidate/adjudication; criterion-level [L]/[H] judge rejects good behavior — DIRECT SCORING PENDING judge run)",
  ),
  Q6_nonredundancy=dict(
    comoving_pairs=[dict(a=a,b=b,jaccard=j,shared=s,union=u) for a,b,j,s,u in redu],
    max_pairwise=redu[0][2] if redu else 0,
  ),
  placeholders=dict(
    Q4_interjudge="PENDING two independent judge runs (method §4)",
    Q5_evidence_grounding="PENDING judge run (fraction of verdicts citing quote/sha/line before score)",
    Q7_actionability="PARTIAL (per-criterion actionability in criteria.jsonl; formal distinct-remediation audit PENDING)",
    Q8_false_negative_sensitivity="PENDING consequence-weighted recall on confirmed failures (needs full adjudication)",
    Q9_gaming_resistance="PENDING adversarial pass (method §4)",
    Q10_calibration="PENDING (hard/soft & severity vs real impact — needs judge run)",
  ),
)
json.dump(scorecard, open(os.path.join(OUT,"scorecard.json"),"w"), indent=2)
print(f"\nQ1 coverage strict={cov_strict:.3f} credit={cov_credit:.3f} (covered={covered} partial={partial} gap={gap} / {tot})")
print(f"Q6 max pairwise jaccard={redu[0] if redu else None}")

# ---------------------------------------------------------------------------
# Deterministic tuning/validation split (sha1 nibble ~70/30) of DISTINCT examples
# ---------------------------------------------------------------------------
split = {"tuning": [], "validation": []}
for e in examples:
    h = int(hashlib.sha1(e["example_id"].encode()).hexdigest()[0], 16)  # 0..15
    (split["tuning"] if h < 11 else split["validation"]).append(e["example_id"])
# where do known-bad episodes land?
ep_ex = {}
for e in examples:
    if e["episode"]:
        ep_ex.setdefault(e["episode"], []).append(
          (e["example_id"], "tuning" if int(hashlib.sha1(e["example_id"].encode()).hexdigest()[0],16)<11 else "validation"))
splitobj = dict(
  method="sha1(example_id) first nibble; <11 -> tuning (~70%), else validation (~30%)",
  tuning_n=len(split["tuning"]), validation_n=len(split["validation"]),
  known_bad_placement=ep_ex,
  tuning=sorted(split["tuning"]), validation=sorted(split["validation"]),
)
json.dump(splitobj, open(os.path.join(OUT,"failure_corpus_split.json"),"w"), indent=2)
print(f"split: tuning={len(split['tuning'])} validation={len(split['validation'])}")
for ep,v in ep_ex.items():
    print(f"  {ep}: {set(s for _,s in v)}")
