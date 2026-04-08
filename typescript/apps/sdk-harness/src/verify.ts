import type { HarnessResult } from "./models.js";

export function summarize(results: HarnessResult[]): string {
  const passed = results.filter((r) => r.ok).length;
  const skipped = results.filter((r) => r.ok && r.detail?.startsWith("skipped:")).length;
  const failed = results.filter((r) => !r.ok).length;
  const parts = [`${passed}/${results.length} scenarios passed`];
  if (skipped > 0) {
    parts.push(`(${skipped} skipped: native binding not available)`);
  }
  if (failed > 0) {
    const failedNames = results.filter((r) => !r.ok).map((r) => `${r.name}: ${r.detail}`);
    parts.push(`${failed} failed:\n${failedNames.join("\n")}`);
  }
  return parts.join(" ");
}
