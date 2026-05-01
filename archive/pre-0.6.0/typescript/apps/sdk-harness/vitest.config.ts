import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    testTimeout: 400_000,
    // singleFork keeps all tests in one worker process, which avoids the
    // vitest IPC onTaskUpdate timeout that fires when a long-running
    // synchronous test prevents the worker from processing RPC callbacks.
    pool: "forks",
    poolOptions: {
      forks: {
        singleFork: true,
      },
    },
  },
});
