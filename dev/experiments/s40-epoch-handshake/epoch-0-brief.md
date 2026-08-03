# Epoch 0 brief — hello

**From:** Steward

**To:** fresh role-simulated orchestrator for the handshake experiment

## Goal

Facilitate one implementer that creates `hello.txt` containing exactly `hello`.

## Constraints

- Work only in this experiment directory.
- Do not create `world.txt`, `hello-world.txt`, a Git commit, or any release
  artifact.
- The implementer must tell the orchestrator when the artifact is complete.
- The orchestrator must verify the exact content, write `epoch-0-receipt.json`,
  and send a compact receipt message to the Steward.

## Stop condition

Stop after the receipt is sent. The Steward alone verifies it and commissions
the fresh Epoch 1 orchestrator.
