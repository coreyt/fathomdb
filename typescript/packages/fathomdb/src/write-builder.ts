import { BuilderValidationError } from "./errors.js";
import {
  PreserializedJson,
  type ActionInsertInput,
  type ChunkInsertInput,
  type EdgeInsertInput,
  type NodeInsertInput,
  type OperationalAppendInput,
  type OperationalDeleteInput,
  type OperationalPutInput,
  type RunInsertInput,
  type StepInsertInput,
} from "./types.js";

type HandleBase = {
  readonly _builderId: number;
};

let nextBuilderId = 1;

export type NodeHandle = HandleBase & { rowId: string; logicalId: string };
export type EdgeHandle = HandleBase & { logicalId: string };
export type RunHandle = HandleBase & { id: string };
export type StepHandle = HandleBase & { id: string };
export type ActionHandle = HandleBase & { id: string };
export type ChunkHandle = HandleBase & { id: string };

function toJsonString(value: unknown): string {
  if (value instanceof PreserializedJson) return value.json;
  return JSON.stringify(value ?? null);
}

export class WriteRequestBuilder {
  readonly #builderId = nextBuilderId++;
  readonly #request: Record<string, unknown>;

  constructor(label: string) {
    this.#request = {
      label,
      nodes: [],
      node_retires: [],
      edges: [],
      edge_retires: [],
      chunks: [],
      runs: [],
      steps: [],
      actions: [],
      optional_backfills: [],
      vec_inserts: [],
      operational_writes: []
    };
  }

  addNode(input: NodeInsertInput): NodeHandle {
    const properties = toJsonString(input.properties);
    (this.#request.nodes as Array<Record<string, unknown>>).push({
      row_id: input.rowId,
      logical_id: input.logicalId,
      kind: input.kind,
      properties,
      source_ref: input.sourceRef ?? null,
      upsert: input.upsert ?? false,
      chunk_policy: input.chunkPolicy ?? "preserve",
    });
    return { _builderId: this.#builderId, rowId: input.rowId, logicalId: input.logicalId };
  }

  retireNode(logicalId: NodeHandle | string, sourceRef?: string): void {
    (this.#request.node_retires as Array<Record<string, unknown>>).push({
      logical_id: this.#resolveNode(logicalId),
      source_ref: sourceRef ?? null
    });
  }

  addEdge(input: EdgeInsertInput & { source: NodeHandle | string; target: NodeHandle | string }): EdgeHandle {
    const properties = toJsonString(input.properties);
    (this.#request.edges as Array<Record<string, unknown>>).push({
      row_id: input.rowId,
      logical_id: input.logicalId,
      source_logical_id: this.#resolveNode(input.source),
      target_logical_id: this.#resolveNode(input.target),
      kind: input.kind,
      properties,
      source_ref: input.sourceRef ?? null,
      upsert: input.upsert ?? false,
    });
    return { _builderId: this.#builderId, logicalId: input.logicalId };
  }

  retireEdge(logicalId: EdgeHandle | string, sourceRef?: string): void {
    (this.#request.edge_retires as Array<Record<string, unknown>>).push({
      logical_id: this.#resolveEdge(logicalId),
      source_ref: sourceRef ?? null
    });
  }

  addChunk(input: ChunkInsertInput & { node: NodeHandle | string }): ChunkHandle {
    (this.#request.chunks as Array<Record<string, unknown>>).push({
      id: input.id,
      node_logical_id: this.#resolveNode(input.node),
      text_content: input.textContent,
      byte_start: input.byteStart ?? null,
      byte_end: input.byteEnd ?? null,
    });
    return { _builderId: this.#builderId, id: input.id };
  }

  addRun(input: RunInsertInput): RunHandle {
    const properties = toJsonString(input.properties);
    (this.#request.runs as Array<Record<string, unknown>>).push({
      id: input.id,
      kind: input.kind,
      status: input.status,
      properties,
      source_ref: input.sourceRef ?? null,
      upsert: input.upsert ?? false,
      supersedes_id: input.supersedesId ?? null,
    });
    return { _builderId: this.#builderId, id: input.id };
  }

  addStep(input: StepInsertInput & { run: RunHandle | string }): StepHandle {
    const properties = toJsonString(input.properties);
    (this.#request.steps as Array<Record<string, unknown>>).push({
      id: input.id,
      run_id: this.#resolveRun(input.run),
      kind: input.kind,
      status: input.status,
      properties,
      source_ref: input.sourceRef ?? null,
      upsert: input.upsert ?? false,
      supersedes_id: input.supersedesId ?? null,
    });
    return { _builderId: this.#builderId, id: input.id };
  }

  addAction(input: ActionInsertInput & { step: StepHandle | string }): ActionHandle {
    const properties = toJsonString(input.properties);
    (this.#request.actions as Array<Record<string, unknown>>).push({
      id: input.id,
      step_id: this.#resolveStep(input.step),
      kind: input.kind,
      status: input.status,
      properties,
      source_ref: input.sourceRef ?? null,
      upsert: input.upsert ?? false,
      supersedes_id: input.supersedesId ?? null,
    });
    return { _builderId: this.#builderId, id: input.id };
  }

  addOptionalBackfill(target: "fts" | "vec" | "all", payload: unknown): void {
    (this.#request.optional_backfills as Array<Record<string, unknown>>).push({
      target,
      payload: toJsonString(payload),
    });
  }

  addVecInsert(input: { chunk: ChunkHandle | string; embedding: number[] }): void {
    (this.#request.vec_inserts as Array<Record<string, unknown>>).push({
      chunk_id: this.#resolveChunk(input.chunk),
      embedding: input.embedding
    });
  }

  addOperationalAppend(input: OperationalAppendInput): void {
    const payloadJson = toJsonString(input.payloadJson);
    (this.#request.operational_writes as Array<Record<string, unknown>>).push({
      type: "append",
      collection: input.collection,
      record_key: input.recordKey,
      payload_json: payloadJson,
      source_ref: input.sourceRef ?? null,
    });
  }

  addOperationalPut(input: OperationalPutInput): void {
    const payloadJson = toJsonString(input.payloadJson);
    (this.#request.operational_writes as Array<Record<string, unknown>>).push({
      type: "put",
      collection: input.collection,
      record_key: input.recordKey,
      payload_json: payloadJson,
      source_ref: input.sourceRef ?? null,
    });
  }

  addOperationalDelete(input: OperationalDeleteInput): void {
    (this.#request.operational_writes as Array<Record<string, unknown>>).push({
      type: "delete",
      collection: input.collection,
      record_key: input.recordKey,
      source_ref: input.sourceRef ?? null,
    });
  }

  build(): Record<string, unknown> {
    return structuredClone(this.#request);
  }

  #assertOwnership(handle: HandleBase): void {
    if (handle._builderId !== this.#builderId) {
      throw new BuilderValidationError("handle belongs to a different WriteRequestBuilder");
    }
  }

  #resolveNode(handleOrId: NodeHandle | string): string {
    if (typeof handleOrId === "string") {
      return handleOrId;
    }
    this.#assertOwnership(handleOrId);
    return handleOrId.logicalId;
  }

  #resolveEdge(handleOrId: EdgeHandle | string): string {
    if (typeof handleOrId === "string") {
      return handleOrId;
    }
    this.#assertOwnership(handleOrId);
    return handleOrId.logicalId;
  }

  #resolveRun(handleOrId: RunHandle | string): string {
    if (typeof handleOrId === "string") {
      return handleOrId;
    }
    this.#assertOwnership(handleOrId);
    return handleOrId.id;
  }

  #resolveStep(handleOrId: StepHandle | string): string {
    if (typeof handleOrId === "string") {
      return handleOrId;
    }
    this.#assertOwnership(handleOrId);
    return handleOrId.id;
  }

  #resolveChunk(handleOrId: ChunkHandle | string): string {
    if (typeof handleOrId === "string") {
      return handleOrId;
    }
    this.#assertOwnership(handleOrId);
    return handleOrId.id;
  }
}
