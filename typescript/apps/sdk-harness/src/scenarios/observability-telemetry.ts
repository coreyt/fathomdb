import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, tempDbPath } from "../skip.js";

function makeWrite(label: string) {
  const builder = new WriteRequestBuilder(label);
  const logicalId = `doc:${label}`;
  const node = builder.addNode({
    rowId: newRowId(),
    logicalId,
    kind: "Document",
    properties: { title: label },
    sourceRef: `source:${label}`,
    upsert: true,
    chunkPolicy: "replace",
  });
  builder.addChunk({
    id: newId(),
    node,
    textContent: `observability telemetry content for ${label}`,
  });
  return builder.build();
}

export function observabilityTelemetryScenario(): HarnessResult {
  const dbPath = tempDbPath("observability-telemetry");
  const engine = Engine.open(dbPath, { telemetryLevel: "counters" });

  engine.write(makeWrite("seed-0"));
  const first = engine.telemetrySnapshot();
  engine.write(makeWrite("seed-1"));
  engine.nodes("Document").limit(10).execute();
  const second = engine.telemetrySnapshot();
  engine.nodes("Document").limit(10).execute();
  const third = engine.telemetrySnapshot();

  assert(second.writesTotal >= first.writesTotal, "writesTotal must be monotonic");
  assert(third.queriesTotal >= second.queriesTotal, "queriesTotal must be monotonic");
  assert(third.writeRowsTotal >= second.writeRowsTotal, "writeRowsTotal must be monotonic");
  assert(third.errorsTotal >= 0, "errorsTotal must be non-negative");
  assert(third.cacheHits >= 0, "cacheHits must be non-negative");
  assert(third.cacheMisses >= 0, "cacheMisses must be non-negative");
  assert(third.writesTotal > 0, "writesTotal must observe writes");
  assert(third.queriesTotal > 0, "queriesTotal must observe queries");

  engine.close();
  return {
    name: "observability_telemetry",
    ok: true,
    detail:
      `writesTotal=${third.writesTotal}, queriesTotal=${third.queriesTotal}, ` +
      `writeRowsTotal=${third.writeRowsTotal}, errorsTotal=${third.errorsTotal}, ` +
      `cacheHits=${third.cacheHits}, cacheMisses=${third.cacheMisses}`,
  };
}
