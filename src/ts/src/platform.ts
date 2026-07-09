// Platform-resolution + native-binding loader contract for the napi-rs
// per-platform package split (R-REL-4f, dev/design/
// 0.8.18-slice-20-publish-pipeline.md).
//
// The published `fathomdb` package is a THIN main package: it ships no `.node`
// binary itself. Each platform's compiled binding is published as a separate
// `@fathomdb/fathomdb-<triple>` package carrying `os`/`cpu` (+ `libc`) so npm
// installs only the one matching the host and SKIPS the rest (they are
// `optionalDependencies`). The loader below picks the right platform package
// for the running host and — critically — throws a CLEAR "unsupported
// platform" error when no matching binary is present, instead of letting a
// later `require` of a missing/mismatched `.node` segfault at runtime.
//
// This module has NO top-level side effects (it never loads a native binary on
// import) so it is unit-testable without a built binding.

/** Thrown when the host (platform, arch[, libc]) has no prebuilt binary. */
export class UnsupportedPlatformError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UnsupportedPlatformError";
  }
}

/** napi-rs triple label for a host, or `null` if the host is not mapped. */
export function tripleFor(
  platform: NodeJS.Platform,
  arch: string,
  isMusl: boolean,
): string | null {
  switch (platform) {
    case "linux":
      switch (arch) {
        case "x64":
          return isMusl ? "linux-x64-musl" : "linux-x64-gnu";
        case "arm64":
          return isMusl ? "linux-arm64-musl" : "linux-arm64-gnu";
        case "arm":
          return "linux-arm-gnueabihf";
        default:
          return null;
      }
    case "darwin":
      switch (arch) {
        case "x64":
          return "darwin-x64";
        case "arm64":
          return "darwin-arm64";
        default:
          return null;
      }
    case "win32":
      switch (arch) {
        case "x64":
          return "win32-x64-msvc";
        case "ia32":
          return "win32-ia32-msvc";
        case "arm64":
          return "win32-arm64-msvc";
        default:
          return null;
      }
    default:
      return null;
  }
}

/**
 * Resolve the napi triple for a host, throwing `UnsupportedPlatformError`
 * with a clear message when the (platform, arch) pair is not mapped.
 */
export function resolveTriple(
  platform: NodeJS.Platform,
  arch: string,
  isMusl: boolean,
): string {
  const triple = tripleFor(platform, arch, isMusl);
  if (triple === null) {
    throw new UnsupportedPlatformError(
      `Unsupported platform: FathomDB has no prebuilt native binary for ${platform}/${arch}. ` +
        `Supported hosts: linux (x64/arm64/arm), darwin (x64/arm64), win32 (x64/ia32/arm64).`,
    );
  }
  return triple;
}

/** The published binary package name for a triple. */
export function platformPackageName(triple: string): string {
  return `@fathomdb/fathomdb-${triple}`;
}

export interface LoaderSeams {
  platform: NodeJS.Platform;
  arch: string;
  isMusl: boolean;
  /** Returns the local dev binary module if `fathomdb.<triple>.node` exists next
   *  to the loader (napi `--platform` build output), else `null`. */
  loadLocal: (triple: string) => unknown | null;
  /** `require("@fathomdb/fathomdb-<triple>")`; MUST throw if the (optional)
   *  platform package is not installed. */
  requirePackage: (pkg: string) => unknown;
}

/**
 * Load the native binding for the running host. Order:
 *   1. local dev binary (`fathomdb.<triple>.node`) if present;
 *   2. the published `@fathomdb/fathomdb-<triple>` platform package;
 *   3. otherwise throw `UnsupportedPlatformError` — a mac/win install that
 *      skipped the linux optionalDependency lands here at REQUIRE time (loud),
 *      never as a silent `.node` load / runtime segfault.
 */
export function loadPlatformBinding(seams: LoaderSeams): unknown {
  const triple = resolveTriple(seams.platform, seams.arch, seams.isMusl);

  const local = seams.loadLocal(triple);
  if (local != null) {
    return local;
  }

  const pkg = platformPackageName(triple);
  try {
    return seams.requirePackage(pkg);
  } catch (cause) {
    throw new UnsupportedPlatformError(
      `Unsupported platform: no prebuilt native binary for ${seams.platform}/${seams.arch} ` +
        `(${triple}). The optional platform package "${pkg}" is not installed — npm skips ` +
        `platform packages that do not match the host os/cpu. If your platform should be ` +
        `supported, install "${pkg}" or build from source with \`npm run build\`. ` +
        `Cause: ${(cause as Error)?.message ?? String(cause)}`,
    );
  }
}
