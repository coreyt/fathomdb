import type { HarnessResult } from "./models.js";

export function summarize(results: HarnessResult[]): string {
  const passed = results.filter((r) => r.ok).length;
  const skipped = results.filter((r) => r.ok && r.detail?.startsWith("skipped:")).length;
  const failed = results.filter((r) => !r.ok).length;
  const parts = [`${passed}/${results.length} scenarios passed`];
  const passedDetails = results
    .filter((r) => r.ok && r.detail && !r.detail.startsWith("skipped:"))
    .map((r) => `${r.name}: ${r.detail}`);
  if (skipped > 0) {
    parts.push(`(${skipped} skipped: native binding not available)`);
  }
  if (passedDetails.length > 0) {
    parts.push(`details:\n${passedDetails.join("\n")}`);
  }
  if (failed > 0) {
    const failedNames = results.filter((r) => !r.ok).map((r) => `${r.name}: ${r.detail}`);
    parts.push(`${failed} failed:\n${failedNames.join("\n")}`);
  }
  return parts.join(" ");
}
