import { Engine, WriteRequestBuilder, type ResponseCycleEvent, newId, newRowId } from "fathomdb";
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
    textContent: `observability feedback content for ${label}`,
  });
  return builder.build();
}

export function observabilityFeedbackScenario(): HarnessResult {
  const dbPath = tempDbPath("observability-feedback");
  const engine = Engine.open(dbPath);
  engine.write(makeWrite("seed-0"));

  const eventsByOperation = new Map<string, ResponseCycleEvent[]>();
  const record = (event: ResponseCycleEvent): void => {
    const existing = eventsByOperation.get(event.operationId) ?? [];
    existing.push(event);
    eventsByOperation.set(event.operationId, existing);
  };

  let threwOnce = false;
  engine.write(
    makeWrite("callback-suppression"),
    (event) => {
      record(event);
      if (!threwOnce && event.phase === "started") {
        threwOnce = true;
        throw new Error("intentional callback failure");
      }
    },
    { slowThresholdMs: 1, heartbeatIntervalMs: 1 },
  );
  assert(threwOnce, "expected callback suppression path to execute");

  const normalCallback = (event: ResponseCycleEvent): void => {
    record(event);
  };
  const feedbackConfig = { slowThresholdMs: 1, heartbeatIntervalMs: 1 };

  for (let i = 0; i < 5; i += 1) {
    engine.write(makeWrite(`feedback-${i}`), normalCallback, feedbackConfig);
    engine.nodes("Document").limit(10).execute(normalCallback, feedbackConfig);
    engine.admin.checkIntegrity(normalCallback, feedbackConfig);
    engine.admin.traceSource("source:seed-0", normalCallback, feedbackConfig);
  }

  let completedOperations = 0;
  let suppressedOperations = 0;
  let sawStarted = false;
  let sawFinished = false;

  for (const events of eventsByOperation.values()) {
    assert(events.length >= 1, "expected feedback events");
    assert(events[0].phase === "started", "first feedback phase must be started");

    if (events[events.length - 1].phase === "finished" || events[events.length - 1].phase === "failed") {
      completedOperations += 1;
    } else {
      assert(events.length === 1 && events[0].phase === "started", "suppressed operation must stop after started");
      suppressedOperations += 1;
    }

    for (let i = 1; i < events.length; i += 1) {
      assert(events[i].elapsedMs >= events[i - 1].elapsedMs, "elapsedMs must be nondecreasing");
    }

    sawStarted = sawStarted || events.some((event) => event.phase === "started");
    sawFinished = sawFinished || events.some((event) => event.phase === "finished");
  }

  assert(sawStarted, "expected started feedback events");
  assert(sawFinished, "expected finished feedback events");
  assert(completedOperations > 0, "expected completed operations");
  assert(suppressedOperations <= 1, "expected at most one callback-suppressed operation");

  const integrity = engine.admin.checkIntegrity();
  assert(integrity.physicalOk === true, "physical integrity must pass");
  assert(integrity.foreignKeysOk === true, "foreign keys must be valid");

  engine.close();
  return {
    name: "observability_feedback",
    ok: true,
    detail:
      `operations=${eventsByOperation.size}, completedOperations=${completedOperations}, ` +
      `suppressedOperations=${suppressedOperations}, threwOnce=${threwOnce}`,
  };
}
