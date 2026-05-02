# Preliminary Solution Design (PSD): {{Title}}

| Field          | Value                                           |
| -------------- | ----------------------------------------------- |
| Author(s)      | {{names}}                                       |
| Date           | {{YYYY-MM-DD}}                                  |
| Status         | Draft / Under Review / Approved / Superseded    |
| Related Needs  | {{links to User Needs entries}}                 |
| Related Inputs | {{links to Technical Input / constraints docs}} |
| Supersedes     | {{prior PSD, if any}}                           |

_Front-matter metadata. One row per field. Keep links, not prose._

## 1. Purpose

_One paragraph. State the problem the PSD solves and the decision it forces. Do not restate the full need — link to it._

## 2. Scope

_What this PSD covers and, just as important, what it does not. Bullet in-scope and out-of-scope items. Call out any deferred sub-problems with a pointer to where they will be handled._

## 3. Inputs

### 3.1 User Needs (Desires / Wants / Needs)

_Bullet the stakeholder needs this PSD must satisfy. Tag each as **Need** (must), **Want** (should), or **Desire** (nice). Cite the source need ID so traceability holds._

### 3.2 Constraints and Limitations (Technical Input)

_Bullet hard constraints (regulatory, interface, performance budget, existing-system compatibility, cost, schedule, team skill, etc.). Mark each **Hard** or **Soft**. A trade-off can bend a Soft constraint; a Hard constraint is non-negotiable._

### 3.3 Assumptions

_What you are taking as given without proof. Each assumption is a future risk if it turns out false — note the impact._

## 4. Evaluation Criteria

_The yardsticks used to compare options. Derive directly from §3. Each criterion gets: a short name, what it measures, and a weight or priority (e.g., 1–5, or Must/High/Med/Low). Keep the list short — 5 to 8 criteria is usually enough._

| #   | Criterion | Measures | Weight | Source |
| --- | --------- | -------- | ------ | ------ |
|     |           |          |        |        |

## 5. Candidate Solutions

_Describe 2–4 candidates at the same level of detail. Each one gets the same sub-structure so they can be compared fairly. If you only have one candidate, you have not done a trade study yet — generate at least one foil._

### 5.1 Option A — {{name}}

_One-paragraph description. Include a sketch, block diagram, or interface outline if useful. State the key design choices that distinguish this option._

**Pros.** _Bullet the advantages, tied to criteria in §4._

**Cons.** _Bullet the drawbacks, tied to criteria in §4._

**Risks / unknowns.** _What would have to be true for this to work; what could go wrong._

### 5.2 Option B — {{name}}

_Same structure as Option A._

### 5.3 Option C — {{name}} _(optional)_

_Same structure._

## 6. Trade-Off Analysis

_The comparison itself. Score each option against each criterion. Use the same scale across the row. Show the math; do not just present a winner._

| Criterion (weight) | Option A | Option B | Option C |
| ------------------ | -------- | -------- | -------- |
|                    |          |          |          |
| **Weighted total** |          |          |          |

_Below the table: 1–2 paragraphs walking through the dominant trade-offs. Call out where options tie, where one clearly wins, and where the score is sensitive to weight changes._

## 7. Recommended Solution

_State the selected option in one sentence. Then 2–4 paragraphs justifying the pick against §4 and §6. Be explicit about which criteria you are optimizing for and which you are accepting as second-best. If the recommendation depends on an assumption from §3.3, say so._

### 7.1 High-Level Description

_Block diagram, interface list, or short narrative of how the chosen solution works end-to-end. Just enough for a reader to picture it; full detail belongs in the downstream Detailed Design._

### 7.2 How It Meets the Needs

_Map each Need/Want/Desire from §3.1 to where the recommended solution addresses it. A small table works well._

| Need ID | How addressed |
| ------- | ------------- |
|         |               |

## 8. Alternatives Considered

_One short paragraph per non-recommended option summarising why it lost. This is the record future readers will check when they ask "did you consider X?" — keep enough detail that the answer is obviously yes._

## 9. Open Questions and Risks

_Bullet what is still unresolved and what could invalidate the recommendation. For each, note who owns the answer and by when._

## 10. Next Steps

_What happens after this PSD is approved: detailed design, prototype, spike, follow-on PSD for a deferred sub-problem, etc. Bullet list with owners._

## 11. References

_Links to the source needs, constraints documents, prior art, standards, and any external research cited. Inline citations elsewhere in the doc should resolve here._
