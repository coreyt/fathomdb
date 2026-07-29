"""`sbom-survey` — a CycloneDX 1.6 dependency survey over FathomDB's tracked manifests.

Spec of record: `dev/design/0.8.20-slice-31-sbom-survey-tool.md` (0.8.20 Slice 31).
This package is the Slice 32 implementation of that spec.

The tool is **informational and NOT CI-gating** (`plan-0.8.20.md` §3a, HITL
2026-07-29, steward `seq-153`). It never applies a dependency bump and never
edits a manifest or a lockfile; its only write path is its own gitignored
output directory.
"""

from __future__ import annotations

__all__ = ["TIER_VOCABULARY", "__version__"]

__version__ = "0.1.0"

#: The HITL-ruled tier vocabulary, in the ruled order — `shipped` first because
#: it is the tier that outranks the others in Slice 33's triage (design §5.9).
#: `fixture` is an EXCLUSION REASON, never a fourth tier (§5.2): putting
#: deliberately fake packages in the component list would hand a vulnerability
#: feed real-looking phantoms.
TIER_VOCABULARY: tuple[str, str, str] = ("shipped", "dev-tooling", "eval-only")
