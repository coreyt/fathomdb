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

/** Opaque reference to a node added via {@link WriteRequestBuilder}. */
export type NodeHandle = HandleBase & { rowId: string; logicalId: string };

/** Opaque reference to an edge added via {@link WriteRequestBuilder}. */
export type EdgeHandle = HandleBase & { logicalId: string };

/** Opaque reference to a run added via {@link WriteRequestBuilder}. */
export type RunHandle = HandleBase & { id: string };

/** Opaque reference to a step added via {@link WriteRequestBuilder}. */
export type StepHandle = HandleBase & { id: string };

/** Opaque reference to an action added via {@link WriteRequestBuilder}. */
export type ActionHandle = HandleBase & { id: string };

/** Opaque reference to a chunk added via {@link WriteRequestBuilder}. */
export type ChunkHandle = HandleBase & { id: string };

function toJsonString(value: unknown): string {
  if (value instanceof PreserializedJson) return value.json;
  return JSON.stringify(value ?? null);
}

/**
 * Mutable builder that assembles a write request from individual mutations.
 *
 * Handles are returned when adding nodes, edges, runs, steps, actions, and
 * chunks so they can be cross-referenced within the same request. Call
 * {@link build} to produce the finalized wire-format object.
 */
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

  /**
   * Add a node to the write request.
   *
   * @param input - Node properties including rowId, logicalId, kind, and properties.
   * @returns A handle that can be used to reference this node in edges and chunks.
   */
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

  /**
   * Mark a node as retired (soft-delete) by logical ID or handle.
   *
   * @param logicalId - The node handle or logical ID string to retire.
   * @param sourceRef - Optional provenance source reference.
   */
  retireNode(logicalId: NodeHandle | string, sourceRef?: string): void {
    (this.#request.node_retires as Array<Record<string, unknown>>).push({
      logical_id: this.#resolveNode(logicalId),
      source_ref: sourceRef ?? null
    });
  }

  /**
   * Add an edge connecting two nodes to the write request.
   *
   * @param input - Edge properties including source, target, kind, and properties.
   * @returns A handle that can be used to reference this edge.
   */
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

  /**
   * Mark an edge as retired (soft-delete) by logical ID or handle.
   *
   * @param logicalId - The edge handle or logical ID string to retire.
   * @param sourceRef - Optional provenance source reference.
   */
  retireEdge(logicalId: EdgeHandle | string, sourceRef?: string): void {
    (this.#request.edge_retires as Array<Record<string, unknown>>).push({
      logical_id: this.#resolveEdge(logicalId),
      source_ref: sourceRef ?? null
    });
  }

  /**
   * Add a text chunk associated with a node.
   *
   * @param input - Chunk properties including id, owning node, and text content.
   * @returns A handle for referencing this chunk in vector inserts.
   */
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

  /**
   * Add a run to the write request.
   *
   * @param input - Run properties including id, kind, status, and properties.
   * @returns A handle for referencing this run when adding steps.
   */
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

  /**
   * Add a step belonging to a run.
   *
   * @param input - Step properties including id, owning run, kind, status, and properties.
   * @returns A handle for referencing this step when adding actions.
   */
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

  /**
   * Add an action belonging to a step.
   *
   * @param input - Action properties including id, owning step, kind, status, and properties.
   * @returns A handle for referencing this action.
   */
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

  /**
   * Queue an optional projection backfill task (e.g. FTS or vector).
   *
   * @param target - Which projection to backfill (`"fts"`, `"vec"`, or `"all"`).
   * @param payload - Projection-specific payload data.
   */
  addOptionalBackfill(target: "fts" | "vec" | "all", payload: unknown): void {
    (this.#request.optional_backfills as Array<Record<string, unknown>>).push({
      target,
      payload: toJsonString(payload),
    });
  }

  /**
   * Add a vector embedding associated with a chunk.
   *
   * @param input - The chunk reference and embedding vector.
   */
  addVecInsert(input: { chunk: ChunkHandle | string; embedding: number[] }): void {
    (this.#request.vec_inserts as Array<Record<string, unknown>>).push({
      chunk_id: this.#resolveChunk(input.chunk),
      embedding: input.embedding
    });
  }

  /**
   * Append a mutation to an operational collection.
   *
   * @param input - Append input including collection name, record key, and payload.
   */
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

  /**
   * Put (upsert) a record into an operational collection.
   *
   * @param input - Put input including collection name, record key, and payload.
   */
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

  /**
   * Delete a record from an operational collection.
   *
   * @param input - Delete input including collection name and record key.
   */
  addOperationalDelete(input: OperationalDeleteInput): void {
    (this.#request.operational_writes as Array<Record<string, unknown>>).push({
      type: "delete",
      collection: input.collection,
      record_key: input.recordKey,
      source_ref: input.sourceRef ?? null,
    });
  }

  /**
   * Resolve all handles and produce the finalized write request object.
   *
   * @returns A deep clone of the assembled wire-format request.
   * @throws {BuilderValidationError} If any handle belongs to a different builder.
   */
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
