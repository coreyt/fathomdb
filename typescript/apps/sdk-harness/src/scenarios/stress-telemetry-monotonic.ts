import { Engine, WriteRequestBuilder, newId, newRowId, type TelemetrySnapshot } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

function stressDurationMs(): number {
  return Number(process.env.FATHOM_TS_STRESS_DURATION_SECONDS ?? "5") * 1000;
}

function makeWrite(label: string) {
  const builder = new WriteRequestBuilder(label);
  const logicalId = `doc:${label}`;
  const node = builder.addNode({
    rowId: newRowId(), logicalId, kind: "Document",
    properties: { title: label }, sourceRef: `source:${label}`,
    upsert: true, chunkPolicy: "replace",
  });
  builder.addChunk({ id: `chunk:${logicalId}:0`, node, textContent: `telemetry content ${label}` });
  return builder.build();
}

export function stressTelemetryMonotonicScenario(): HarnessResult {
  try {
    const dbPath = tempDbPath("stress-telemetry");
    const engine = Engine.open(dbPath, { telemetryLevel: "counters" });
    const durationMs = stressDurationMs();

    for (let i = 0; i < 100; i++) engine.write(makeWrite(`seed-${i}`));

    const snapshots: TelemetrySnapshot[] = [];
    let writes = 0;
    let reads = 0;
    const deadline = Date.now() + durationMs;

    while (Date.now() < deadline) {
      engine.write(makeWrite(`tw-${writes}`));
      writes++;
      for (let r = 0; r < 4; r++) {
        engine.nodes("Document").limit(10).execute();
        reads++;
      }
      snapshots.push(engine.telemetrySnapshot());
    }
    snapshots.push(engine.telemetrySnapshot());

    // Monotonicity assertions.
    for (let i = 1; i < snapshots.length; i++) {
      const prev = snapshots[i - 1];
      const curr = snapshots[i];
      assert(curr.queriesTotal >= prev.queriesTotal, "queriesTotal decreased");
      assert(curr.writesTotal >= prev.writesTotal, "writesTotal decreased");
      assert(curr.writeRowsTotal >= prev.writeRowsTotal, "writeRowsTotal decreased");
      assert(curr.errorsTotal >= prev.errorsTotal, "errorsTotal decreased");
      assert(curr.adminOpsTotal >= prev.adminOpsTotal, "adminOpsTotal decreased");
    }

    const last = snapshots[snapshots.length - 1];
    assert(last.queriesTotal > 0, "telemetry must observe reads");
    assert(last.writesTotal > 0, "telemetry must observe writes");
    assert(last.writeRowsTotal >= last.writesTotal, "write rows >= write count");
    assert(last.errorsTotal === 0, "errors_total must be zero");
    assert(last.cacheHits + last.cacheMisses > 0, "cache must observe activity");

    const integrity = engine.admin.checkIntegrity();
    assert(integrity.physicalOk, "physical integrity must pass");
    assert(integrity.foreignKeysOk, "foreign keys must be valid");
    engine.close();

    return {
      name: "stress_telemetry_monotonic",
      ok: true,
      detail:
        `duration_seconds=${durationMs / 1000}, writes=${writes}, reads=${reads}, ` +
        `telemetry_samples=${snapshots.length}, queries_total=${last.queriesTotal}, ` +
        `writes_total=${last.writesTotal}, write_rows_total=${last.writeRowsTotal}, ` +
        `errors_total=${last.errorsTotal}, admin_ops_total=${last.adminOpsTotal}, ` +
        `cache_hits=${last.cacheHits}, cache_misses=${last.cacheMisses}`,
    };
  } catch (error) {
    return handleScenarioError("stress_telemetry_monotonic", error);
  }
}
