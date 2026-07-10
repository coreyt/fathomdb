#!/usr/bin/env python3
"""Q-IRR (inter-rater), C6 grounding, and Q-DISC specificity from the two
independent judge runs. Detector-output only. No git commits."""
import json, os, collections
BASE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
A = {json.loads(l)["example_id"]: json.loads(l) for l in open(os.path.join(BASE,"audit/judge_A.jsonl"))}
B = {json.loads(l)["example_id"]: json.loads(l) for l in open(os.path.join(BASE,"audit/judge_B.jsonl"))}
pack = [json.loads(l) for l in open(os.path.join(BASE,"audit/adjudication_pack.jsonl"))]
role = {p["example_id"]: p["role"] for p in pack}
det  = {p["example_id"]: p["detector"] for p in pack}

ids = [p["example_id"] for p in pack]
assert set(ids) <= set(A) and set(ids) <= set(B), "missing judge rows"

def kappa(labels_a, labels_b):
    n = len(labels_a)
    cats = sorted(set(labels_a) | set(labels_b))
    po = sum(1 for x,y in zip(labels_a,labels_b) if x==y)/n
    pa = collections.Counter(labels_a); pb = collections.Counter(labels_b)
    pe = sum((pa[c]/n)*(pb[c]/n) for c in cats)
    k = (po-pe)/(1-pe) if pe != 1 else 1.0
    return round(po,3), round(k,3), pe

# 3-way verdict agreement
va = [A[i]["verdict"] for i in ids]; vb = [B[i]["verdict"] for i in ids]
po3, k3, pe3 = kappa(va, vb)
# binary: REAL_FAILURE vs NOT
ba = ["R" if A[i]["verdict"]=="REAL_FAILURE" else "N" for i in ids]
bb = ["R" if B[i]["verdict"]=="REAL_FAILURE" else "N" for i in ids]
po2, k2, pe2 = kappa(ba, bb)

print("=== Q-IRR (two independent judges, N=27) ===")
print(f"3-way verdict:  observed agreement={po3}  Cohen κ={k3}")
print(f"binary (REAL vs not): observed agreement={po2}  Cohen κ={k2}")
print("  (note: base-rate is extreme — 26/27 GOOD — so κ deflates even at high agreement;")
print("   report BOTH observed agreement and κ, with the base-rate caveat.)")
disagree = [i for i in ids if A[i]["verdict"]!=B[i]["verdict"]]
print(f"disagreements: {len(disagree)}")
for i in disagree:
    print(f"  {i[:60]}  A={A[i]['verdict']}/{A[i]['severity']}  B={B[i]['verdict']}/{B[i]['severity']}")

# C6 grounding
print("\n=== C6 grounding (DQ-LIMITED-SAMPLE, 9) ===")
c6 = [i for i in ids if det[i]=="DQ-LIMITED-SAMPLE"]
ra = sum(1 for i in c6 if A[i]["verdict"]=="REAL_FAILURE")
rb = sum(1 for i in c6 if B[i]["verdict"]=="REAL_FAILURE")
consensus_tp = [i for i in c6 if A[i]["verdict"]=="REAL_FAILURE" and B[i]["verdict"]=="REAL_FAILURE"]
print(f"judge A REAL_FAILURE: {ra}/9   judge B REAL_FAILURE: {rb}/9   consensus TP: {len(consensus_tp)}")
print(f"  -> C6 confirmed-TP in corpus = {len(consensus_tp)} (either-judge = {ra or rb})")

# Q-DISC specificity (good-behavior candidates correctly rejected)
print("\n=== Q-DISC specificity (good-behavior group) ===")
good = [i for i in ids if role[i]=="specificity"]
sa = sum(1 for i in good if A[i]["verdict"]=="GOOD_BEHAVIOR")
sb = sum(1 for i in good if B[i]["verdict"]=="GOOD_BEHAVIOR")
print(f"correctly rejected as GOOD: A={sa}/{len(good)} ({round(100*sa/len(good),1)}%)  B={sb}/{len(good)} ({round(100*sb/len(good),1)}%)")

# positive controls
print("\n=== positive controls (known-bad episode rows) ===")
ctrl = [i for i in ids if role[i]=="positive-control"]
for i in ctrl:
    print(f"  {i[:60]}  A={A[i]['verdict']}/{A[i]['severity']}  B={B[i]['verdict']}/{B[i]['severity']}")

out = dict(
  n=27, kappa_3way=k3, obs_agreement_3way=po3, kappa_binary=k2, obs_agreement_binary=po2,
  disagreements=len(disagree),
  c6_grounding=dict(judgeA=ra, judgeB=rb, consensus_tp=len(consensus_tp)),
  specificity=dict(judgeA=f"{sa}/{len(good)}", judgeB=f"{sb}/{len(good)}"),
)
json.dump(out, open(os.path.join(BASE,"audit/irr_result.json"),"w"), indent=2)
print("\nwrote irr_result.json")
