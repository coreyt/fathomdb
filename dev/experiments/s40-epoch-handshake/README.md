# Slice 40 epoch handshake experiment

This is a no-release-state, no-source-change facilitation test for the epoch
protocol in `dev/plans/runs/0.8.20-slice-40-epoch-orchestration.md`.

The experiment has two sequential epochs:

1. A Steward commissions an Epoch-0 orchestrator with `epoch-0-brief.md`.
   That orchestrator commissions one implementer to create `hello.txt`, then
   returns `epoch-0-receipt.json` to the Steward.
2. The Steward verifies that receipt, commissions a **fresh** Epoch-1
   orchestrator with `epoch-1-brief.md`, and that orchestrator commissions one
   implementer to create `world.txt` and `hello-world.txt`.

The expected final text is `hello world`. The receipt files record the message
path, artifact hashes, verification result, agent identity, and next action.
Neither epoch may edit anything outside this directory.
