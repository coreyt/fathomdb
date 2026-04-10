import { canonicalScenario } from "./scenarios/canonical.js";
import { graphScenario } from "./scenarios/graph.js";
import { observabilityFeedbackScenario } from "./scenarios/observability-feedback.js";
import { observabilityTelemetryScenario } from "./scenarios/observability-telemetry.js";
import { recoveryScenario } from "./scenarios/recovery.js";
import { runtimeScenario } from "./scenarios/runtime.js";
import { stressExternalContentScenario } from "./scenarios/stress-external-content.js";
import { stressReadsUnderWriteLoadScenario } from "./scenarios/stress-reads-under-write-load.js";
import { stressTelemetryMonotonicScenario } from "./scenarios/stress-telemetry-monotonic.js";
import { vectorScenario } from "./scenarios/vector.js";
import { summarize } from "./verify.js";

export function runHarness(mode: "baseline" | "vector" | "observability" | "stress"): string {
  if (mode === "observability") {
    return summarize([observabilityTelemetryScenario(), observabilityFeedbackScenario()]);
  }
  if (mode === "stress") {
    return summarize([
      stressReadsUnderWriteLoadScenario(),
      stressTelemetryMonotonicScenario(),
      stressExternalContentScenario(),
    ]);
  }
  const results = [canonicalScenario(), graphScenario(), runtimeScenario(), recoveryScenario()];
  if (mode === "vector") {
    results.push(vectorScenario());
  }
  return summarize(results);
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1])) {
  const mode = (process.argv[2] as "baseline" | "vector" | "observability" | "stress" | undefined) ?? "baseline";
  console.log(runHarness(mode));
}
