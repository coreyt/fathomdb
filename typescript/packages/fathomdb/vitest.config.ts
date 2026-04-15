import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// Locate the fathomdb native binding for the real-engine integration tests.
//
// Resolution order:
//   1. FATHOMDB_NATIVE_BINDING env var (absolute path to a .node file)
//   2. Prebuilt .node files in the prebuilds/ directory (platform-specific)
//   3. Freshly-built cdylib from `cargo build -p fathomdb --features node`
//
// When a cdylib is found it is copied to test/.native/fathomdb.node and
// FATHOMDB_NATIVE_BINDING is set so `src/native.ts` uses it directly.
const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../../..");

// If an explicit env var is set, use it directly (no copy needed).
const envBinding = process.env.FATHOMDB_NATIVE_BINDING;
let targetPath: string;

if (envBinding && existsSync(envBinding)) {
  targetPath = envBinding;
} else {
  // Check for prebuilt .node files first (platform-specific then generic).
  // Also look in the main git worktree when running from a linked worktree,
  // since binary prebuilds are not committed and may only exist in the main tree.
  const platform = process.platform === "win32" ? "win32" : process.platform === "darwin" ? "darwin" : "linux";
  const arch = process.arch;
  const prebuildsDir = resolve(here, "prebuilds");

  // Detect if we are inside a git linked worktree. When we are, repoRoot/.git
  // is a plain file (not a directory) containing "gitdir: <path>".
  const repoGitPath = resolve(repoRoot, ".git");
  let mainWorktreePrebuildsDir: string | null = null;
  try {
    const stat = statSync(repoGitPath);
    if (stat.isFile()) {
      // Parse "gitdir: /abs/path/to/.git/worktrees/<name>" from the file.
      const gitdirLine = readFileSync(repoGitPath, "utf8").trim();
      const match = gitdirLine.match(/^gitdir:\s*(.+)$/);
      if (match) {
        // The main worktree's .git dir is two levels above the worktrees/<name> dir.
        const worktreesGitDir = resolve(match[1], "../..");
        const mainWorkTree = resolve(worktreesGitDir, "..");
        mainWorktreePrebuildsDir = resolve(mainWorkTree, "typescript/packages/fathomdb/prebuilds");
      }
    }
  } catch {
    // Not a git repo or stat failed; ignore.
  }

  const prebuildCandidates = [
    resolve(prebuildsDir, `fathomdb.${platform}-${arch}.node`),
    resolve(prebuildsDir, "fathomdb.node"),
    ...(mainWorktreePrebuildsDir
      ? [
          resolve(mainWorktreePrebuildsDir, `fathomdb.${platform}-${arch}.node`),
          resolve(mainWorktreePrebuildsDir, "fathomdb.node"),
        ]
      : []),
  ];
  const prebuild = prebuildCandidates.find((p) => existsSync(p));

  if (prebuild) {
    targetPath = prebuild;
  } else {
    // Fall back to a freshly-built cdylib.
    const cdylibCandidates = [
      // Linux
      resolve(repoRoot, "target/debug/libfathomdb.so"),
      resolve(repoRoot, "target/release/libfathomdb.so"),
      // macOS
      resolve(repoRoot, "target/debug/libfathomdb.dylib"),
      resolve(repoRoot, "target/release/libfathomdb.dylib"),
      // Windows
      resolve(repoRoot, "target/debug/fathomdb.dll"),
      resolve(repoRoot, "target/release/fathomdb.dll"),
    ];
    const source = cdylibCandidates.find((candidate) => existsSync(candidate));
    if (!source) {
      throw new Error(
        "Missing native binding. Either place a prebuilt .node in prebuilds/ or run `cargo build -p fathomdb --features node` from the repo root.",
      );
    }
    const targetDir = resolve(here, "test/.native");
    mkdirSync(targetDir, { recursive: true });
    targetPath = resolve(targetDir, "fathomdb.node");
    copyFileSync(source, targetPath);
  }
}

export default defineConfig({
  test: {
    env: {
      FATHOMDB_NATIVE_BINDING: targetPath,
    },
    // Real-engine setup (engine open + FTS schema registration + seed data)
    // adds a few hundred ms per test vs the old mocked path; bump the
    // default timeout to give tests headroom on slow CI machines.
    testTimeout: 30_000,
    // Real engines are tempdir-backed and isolated per test, but napi-rs
    // initializes a single shared tracing subscriber inside the .node file
    // and multi-process sharding doesn't play well with that. Keep the
    // test runner single-threaded to avoid flakiness.
    pool: "forks",
    poolOptions: {
      forks: {
        singleFork: true,
      },
    },
  },
});
