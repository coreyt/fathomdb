import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";
import type { HarnessResult } from "../models.js";
import { assert, handleScenarioError, tempDbPath } from "../skip.js";

function stressDurationMs(): number {
  return Number(process.env.FATHOM_TS_STRESS_DURATION_SECONDS ?? "5") * 1000;
}

function makeWrite(label: string, contentRef?: string, contentHash?: string) {
  const builder = new WriteRequestBuilder(label);
  const logicalId = `doc:${label}`;
  const node = builder.addNode({
    rowId: newRowId(), logicalId, kind: "Document",
    properties: { title: label }, sourceRef: `source:${label}`,
    upsert: true, chunkPolicy: "replace", contentRef,
  });
  builder.addChunk({
    id: `chunk:${logicalId}:0`, node,
    textContent: `ext content ${label}`,
    contentHash,
  });
  return builder.build();
}

export function stressExternalContentScenario(): HarnessResult {
  try {
    const dbPath = tempDbPath("stress-ext-content");
    const engine = Engine.open(dbPath);
    const durationMs = stressDurationMs();

    // Seed a mix of content and non-content nodes.
    for (let i = 0; i < 50; i++) {
      const contentRef = i % 2 === 0 ? `s3://docs/seed-${i}.pdf` : undefined;
      const contentHash = contentRef ? `sha256:seed${i}` : undefined;
      engine.write(makeWrite(`seed-${i}`, contentRef, contentHash));
    }

    let contentWrites = 0;
    let plainWrites = 0;
    let filteredReads = 0;
    let unfilteredReads = 0;
    const deadline = Date.now() + durationMs;

    while (Date.now() < deadline) {
      // Content write.
      const cLabel = `ext-${contentWrites}`;
      engine.write(makeWrite(cLabel, `s3://docs/${cLabel}.pdf`, `sha256:${cLabel}`));
      contentWrites++;

      // Plain write.
      engine.write(makeWrite(`plain-${plainWrites}`));
      plainWrites++;

      // Filtered reads — every returned node must have contentRef.
      for (let r = 0; r < 2; r++) {
        const rows = engine.nodes("Document").filterContentRefNotNull().limit(10).execute();
        for (const node of rows.nodes) {
          assert(node.contentRef != null, `filtered read returned node without contentRef: ${node.logicalId}`);
        }
        filteredReads++;
      }

      // Unfiltered reads.
      for (let r = 0; r < 2; r++) {
        engine.nodes("Document").limit(10).execute();
        unfilteredReads++;
      }
    }

    const integrity = engine.admin.checkIntegrity();
    assert(integrity.physicalOk, "physical integrity must pass");
    assert(integrity.foreignKeysOk, "foreign keys must be valid");
    assert(integrity.missingFtsRows === 0, "no missing FTS rows");
    assert(integrity.duplicateActiveLogicalIds === 0, "no duplicate active logical ids");
    engine.close();

    return {
      name: "stress_external_content",
      ok: true,
      detail:
        `duration_seconds=${durationMs / 1000}, content_writes=${contentWrites}, ` +
        `plain_writes=${plainWrites}, filtered_reads=${filteredReads}, ` +
        `unfiltered_reads=${unfilteredReads}`,
    };
  } catch (error) {
    return handleScenarioError("stress_external_content", error);
  }
}
