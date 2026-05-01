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

  // napi-rs CLI appends the ABI suffix (e.g. linux-arm64-gnu, linux-x64-gnu)
  // when building locally. Prefer that over the plain arch-only name so a
  // freshly-built binary from `napi build` takes precedence over a stale prebuild.
  const abiSuffix = platform === "linux" ? "-gnu" : "";
  // Local prebuilds take absolute priority — they are committed per-worktree
  // when a team member has chosen to pin a specific binary for CI determinism.
  const localPrebuildCandidates = [
    resolve(prebuildsDir, `fathomdb.${platform}-${arch}${abiSuffix}.node`),
    resolve(prebuildsDir, `fathomdb.${platform}-${arch}.node`),
    resolve(prebuildsDir, "fathomdb.node"),
  ];
  // Main-worktree prebuilds are a best-effort fallback for linked worktrees.
  // Pack G fix: a local freshly-built cdylib takes priority over a main-
  // worktree prebuild, because the main-worktree binary is commonly stale
  // (pre-feature) when a worktree is under active development.
  const mainWorktreePrebuildCandidates = mainWorktreePrebuildsDir
    ? [
        resolve(mainWorktreePrebuildsDir, `fathomdb.${platform}-${arch}${abiSuffix}.node`),
        resolve(mainWorktreePrebuildsDir, `fathomdb.${platform}-${arch}.node`),
        resolve(mainWorktreePrebuildsDir, "fathomdb.node"),
      ]
    : [];
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

  const localPrebuild = localPrebuildCandidates.find((p) => existsSync(p));
  const cdylib = cdylibCandidates.find((p) => existsSync(p));
  const mainPrebuild = mainWorktreePrebuildCandidates.find((p) => existsSync(p));

  if (localPrebuild) {
    targetPath = localPrebuild;
  } else if (cdylib) {
    const targetDir = resolve(here, "test/.native");
    mkdirSync(targetDir, { recursive: true });
    targetPath = resolve(targetDir, "fathomdb.node");
    copyFileSync(cdylib, targetPath);
  } else if (mainPrebuild) {
    targetPath = mainPrebuild;
  } else {
    throw new Error(
      "Missing native binding. Either place a prebuilt .node in prebuilds/ or run `cargo build -p fathomdb --features node` from the repo root.",
    );
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
