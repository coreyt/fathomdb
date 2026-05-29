// Native-binding loader. Locates the platform-tagged `.node` artifact
// produced by `napi build` and re-exports it as a typed module.

import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));

// `napi build` writes `<name>.<triple>.node` next to the package.json
// (cwd of the build script). Walk up from this file's compiled location
// to find candidates.
const SEARCH_ROOTS = [
  join(here, ".."), // dist/src/ -> dist/
  join(here, "..", ".."), // dist/src/ -> dist/.. (pkg root)
  here, // co-located fallback
];

const TRIPLES = [
  "linux-x64-gnu",
  "linux-x64-musl",
  "linux-arm64-gnu",
  "linux-arm64-musl",
  "linux-arm-gnueabihf",
  "darwin-x64",
  "darwin-arm64",
  "darwin-universal",
  "win32-x64-msvc",
  "win32-ia32-msvc",
  "win32-arm64-msvc",
  "freebsd-x64",
  "android-arm64",
  "android-arm-eabi",
];

function loadNative(): unknown {
  for (const root of SEARCH_ROOTS) {
    for (const triple of TRIPLES) {
      const candidate = join(root, `fathomdb.${triple}.node`);
      if (existsSync(candidate)) {
        return require(candidate);
      }
    }
  }
  throw new Error(
    "fathomdb native binding not found. Run `npm run build` to compile it.",
  );
}

interface NativeWriteReceipt {
  cursor: number;
}

interface NativeSoftFallback {
  branch: string;
}

interface NativeSearchResult {
  projectionCursor: number;
  softFallback: NativeSoftFallback | null;
  results: string[];
}

interface NativeMigrationStepReport {
  stepId: number;
  durationMs: number | null;
  failed: boolean;
}

interface NativeEmbedderIdentity {
  name: string;
  revision: string;
  dimension: number;
}

interface NativeEmbedderEvent {
  kind: string;
  file?: string | null;
  url?: string | null;
  bytes?: number | null;
  sha256?: string | null;
  cachePath?: string | null;
  durationMs?: number | null;
  dim?: number | null;
  docCount?: number | null;
}

interface NativeOpenReport {
  schemaVersionBefore: number;
  schemaVersionAfter: number;
  migrationSteps: NativeMigrationStepReport[];
  embedderWarmupMs: number;
  queryBackend: string;
  defaultEmbedder: NativeEmbedderIdentity;
  embedderDownloadMs: number | null;
  embedderEvents: NativeEmbedderEvent[];
  embedderMeanCenteringRequired: boolean;
  embedderMeanVecPinned: boolean;
}

interface NativeCounterSnapshot {
  queries: number;
  writes: number;
  writeRows: number;
  adminOps: number;
  cacheHit: number;
  cacheMiss: number;
}

interface NativeAttachSubscriberOptions {
  heartbeatIntervalMs?: number;
}

interface NativeEngineConfig {
  embedderPoolSize?: number;
  schedulerRuntimeThreads?: number;
  provenanceRowCap?: number;
  embedderCallTimeoutMs?: number;
  slowThresholdMs?: number;
}

interface NativeEngineOpenOptions {
  engineConfig?: NativeEngineConfig;
  useDefaultEmbedder?: boolean;
}

interface NativeAdminConfigureOptions {
  name: string;
  body: string;
}

export interface NativeEngine {
  write(batch: unknown[]): Promise<NativeWriteReceipt>;
  search(query: string): Promise<NativeSearchResult>;
  close(): Promise<void>;
  drain(timeoutMs: number): Promise<void>;
  counters(): NativeCounterSnapshot;
  openReport(): NativeOpenReport;
  setProfiling(enabled: boolean): void;
  setSlowThresholdMs(value: number): void;
  attachSubscriber(callback: unknown, options?: NativeAttachSubscriberOptions): void;
  // EU-6 test-hooks-gated seam. Present only when the napi binding is
  // built with `--features test-hooks`; the TS surface forwards calls
  // unconditionally and the runtime fails fast if absent.
  configureVectorKindForTest?(kind: string): Promise<void>;
  writeVectorForTest?(kind: string, text: string): Promise<void>;
}

export interface NativeModule {
  Engine: {
    open(path: string, options?: NativeEngineOpenOptions): Promise<NativeEngine>;
  };
  adminConfigure(
    engine: NativeEngine,
    options: NativeAdminConfigureOptions,
  ): Promise<NativeWriteReceipt>;
  forcePanicForTest?: () => void;
  forcePanicInAccessorForTest?: () => void;
}

export const native = loadNative() as NativeModule;
