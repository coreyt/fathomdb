import { canonicalScenario } from "./scenarios/canonical.js";
import { graphScenario } from "./scenarios/graph.js";
import { recoveryScenario } from "./scenarios/recovery.js";
import { runtimeScenario } from "./scenarios/runtime.js";
import { vectorScenario } from "./scenarios/vector.js";
import { summarize } from "./verify.js";

export function runHarness(mode: "baseline" | "vector"): string {
  const results = [canonicalScenario(), graphScenario(), runtimeScenario(), recoveryScenario()];
  if (mode === "vector") {
    results.push(vectorScenario());
  }
  return summarize(results);
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1])) {
  const mode = (process.argv[2] as "baseline" | "vector" | undefined) ?? "baseline";
  console.log(runHarness(mode));
}
