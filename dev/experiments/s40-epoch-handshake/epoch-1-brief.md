# Epoch 1 brief — world

**From:** Steward, after independently verifying `epoch-0-receipt.json` and
`hello.txt` (`68 65 6c 6c 6f 0a`).

**To:** fresh role-simulated orchestrator for the handshake experiment

## Goal

Facilitate one implementer that creates `world.txt` containing exactly `world`
and `hello-world.txt` containing exactly `hello world`.

## Inputs

- Verified predecessor receipt: `epoch-0-receipt.json`
- Verified predecessor artifact: `hello.txt`

## Constraints

- Work only in this experiment directory.
- Do not alter `hello.txt` or any Epoch 0 receipt.
- The implementer must tell the orchestrator when both artifacts are complete.
- The orchestrator must verify the exact content, write `epoch-1-receipt.json`,
  and send a compact receipt message to the Steward.

## Stop condition

Stop after the receipt is sent. The Steward independently verifies the final
`hello world` output.
