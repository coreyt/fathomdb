import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
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
  builder.addChunk({ id: `chunk:${logicalId}:0`, node, textContent: `stress content ${label}` });
  return builder.build();
}

export function stressReadsUnderWriteLoadScenario(): HarnessResult {
  try {
    const dbPath = tempDbPath("stress-reads-write");
    const engine = Engine.open(dbPath);
    const durationMs = stressDurationMs();

    // Seed.
    for (let i = 0; i < 100; i++) engine.write(makeWrite(`seed-${i}`));

    let writes = 0;
    let reads = 0;
    const deadline = Date.now() + durationMs;

    while (Date.now() < deadline) {
      engine.write(makeWrite(`w-${writes}`));
      writes++;
      // Interleave 4 reads per write.
      for (let r = 0; r < 4; r++) {
        const rows = engine.nodes("Document").limit(10).execute();
        assert(!rows.wasDegraded, "read was degraded");
        reads++;
      }
    }

    const integrity = engine.admin.checkIntegrity();
    assert(integrity.physicalOk, "physical integrity must pass");
    assert(integrity.foreignKeysOk, "foreign keys must be valid");
    assert(integrity.missingFtsRows === 0, "no missing FTS rows");
    assert(integrity.duplicateActiveLogicalIds === 0, "no duplicate active logical ids");
    engine.close();

    return {
      name: "stress_reads_under_write_load",
      ok: true,
      detail: `duration_seconds=${durationMs / 1000}, writes=${writes}, reads=${reads}`,
    };
  } catch (error) {
    return handleScenarioError("stress_reads_under_write_load", error);
  }
}
