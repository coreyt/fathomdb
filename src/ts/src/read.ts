// The governed `read.*` namespace (Slice 30 — G2 / G3; Slice 35 — G4).
//
// Per `dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md` (B1 `read.*`), this
// module exposes the governed read verbs beside `admin`:
//
//   * read.get / read.getMany — active-only point lookup by `logicalId`
//     (active = `superseded_at IS NULL`). Not-found is a normal `null` (a typed
//     NotFound class is reserved for a later slice), never a thrown error.
//   * read.collection / read.mutations — paginated op-store read-back over
//     `operational_mutations` with a MANDATORY `limit` + `afterId` cursor.
//   * read.list (G4 / Slice 35) — list active canonical nodes of a given `kind`,
//     optionally filtered by typed predicates (AND-combined), up to `limit` rows.
//     Injection-safe: values are bound parameters; paths must be from the allowlist.
//
// The runtime is the napi-rs binding in `fathomdb-napi`; this module funnels
// every native error through `rethrowTyped` and converts native rows into the
// public SDK shapes. Reads ride the ReaderWorkerPool DEFERRED-tx path inside the
// engine; they NEVER take the writer lock.

import {
  native,
  type NativeNodeRecord,
  type NativeOpStoreRow,
  type NativePredicateInput,
} from "./binding.js";
import { InvalidArgumentError, rethrowTyped } from "./errors.js";
import { validateFfiString } from "./validation.js";
import type { Engine } from "./index.js";

/** Slice 30 (G2) — an active canonical node row from `read.get`/`read.getMany`. */
export interface NodeRecord {
  logicalId: string;
  kind: string;
  body: string;
  /** Interim id carrier (parity with `SearchHit.id`). */
  writeCursor: number;
}

/** Slice 30 (G3) — one `operational_mutations` row from `read.collection`/`read.mutations`. */
export interface OpStoreRow {
  /** Autoincrement PK and the `afterId` cursor key. */
  id: number;
  collection: string;
  recordKey: string;
  opKind: string;
  /** The stored `payload_json`. */
  payload: string;
  schemaId: string | null;
  writeCursor: number;
}

/** Options for `read.collection` / `read.mutations`. `limit` is MANDATORY. */
export interface ReadCollectionOptions {
  /** Exclusive after-id cursor for the next page. */
  afterId?: number;
  /** Required page cap; the engine clamps it to the ~1M cap (no unbounded read). */
  limit: number;
}

/**
 * G4 (Slice 35) — closed predicate for `read.list` filter.
 *
 * `type` ∈ `{"eq","gt","gte","lt","lte"}` (exactly, per ADR D-F1).
 * `path` must be from the engine allowlist (`$.status`, `$.priority`, `$.tags`,
 * `$.kind`, `$.created_at`); non-allowlisted paths throw `InvalidFilterError`.
 * `value` is `string | number | boolean` — mapped to the Rust `ScalarValue`.
 */
export interface Predicate {
  type: "eq" | "gt" | "gte" | "lt" | "lte";
  path: string;
  value: string | number | boolean;
}

async function intercept<T>(fn: () => Promise<T>): Promise<T> {
  try {
    return await fn();
  } catch (err) {
    rethrowTyped(err);
  }
}

function toNodeRecord(n: NativeNodeRecord): NodeRecord {
  return { logicalId: n.logicalId, kind: n.kind, body: n.body, writeCursor: n.writeCursor };
}

function toOpStoreRow(n: NativeOpStoreRow): OpStoreRow {
  return {
    id: n.id,
    collection: n.collection,
    recordKey: n.recordKey,
    opKind: n.opKind,
    payload: n.payload,
    schemaId: n.schemaId,
    writeCursor: n.writeCursor,
  };
}

function validateLimit(limit: number): void {
  if (!Number.isInteger(limit) || limit < 0) {
    throw new RangeError("read.collection/read.mutations require a non-negative integer limit");
  }
}

/** Convert a public `Predicate` to the native `NativePredicateInput` shape. */
function toNativePredicate(pred: Predicate): NativePredicateInput {
  const native: NativePredicateInput = { type: pred.type, path: pred.path };
  if (typeof pred.value === "boolean") {
    native.valueBool = pred.value;
  } else if (typeof pred.value === "number") {
    if (!Number.isInteger(pred.value)) {
      throw new InvalidArgumentError(
        `predicate numeric value must be an integer; got ${pred.value}`,
      );
    }
    native.valueInt = pred.value;
  } else {
    native.valueStr = pred.value;
  }
  return native;
}

export const read = {
  /**
   * `read.get` — return the ACTIVE node carrying `logicalId`, or `null` if
   * absent. Active-only (`superseded_at IS NULL`): a superseded version is never
   * returned. A missing/superseded id is a normal `null`, not a thrown error.
   */
  async get(engine: Engine, logicalId: string): Promise<NodeRecord | null> {
    validateFfiString(logicalId);
    const n = await intercept(() => native.readGet(engine._native, logicalId));
    return n === null ? null : toNodeRecord(n);
  },

  /**
   * `read.getMany` — return one slot per requested id, in REQUEST ORDER. A
   * missing/superseded id yields `null` in its slot (partial, never
   * all-or-nothing).
   */
  async getMany(engine: Engine, logicalIds: string[]): Promise<(NodeRecord | null)[]> {
    for (const id of logicalIds) validateFfiString(id);
    const rows = await intercept(() => native.readGetMany(engine._native, logicalIds));
    return rows.map((n) => (n === null ? null : toNodeRecord(n)));
  },

  /**
   * `read.collection` — paginated op-store read-back over
   * `operational_mutations`, `ORDER BY id`. `options.limit` is MANDATORY (the
   * engine clamps it to a ~1M cap); `options.afterId` is the exclusive cursor.
   */
  async collection(
    engine: Engine,
    collection: string,
    options: ReadCollectionOptions,
  ): Promise<OpStoreRow[]> {
    validateFfiString(collection);
    validateLimit(options.limit);
    const rows = await intercept(() => native.readCollection(engine._native, collection, options));
    return rows.map(toOpStoreRow);
  },

  /**
   * `read.mutations` — mutation-log-oriented alias surface over the same
   * op-store read-back as `read.collection` (identical args + semantics).
   */
  async mutations(
    engine: Engine,
    collection: string,
    options: ReadCollectionOptions,
  ): Promise<OpStoreRow[]> {
    validateFfiString(collection);
    validateLimit(options.limit);
    const rows = await intercept(() => native.readMutations(engine._native, collection, options));
    return rows.map(toOpStoreRow);
  },

  /**
   * G4 (Slice 35) — `read.list`: list active `canonical_nodes` of a given `kind`,
   * optionally filtered by closed `Predicate` objects (AND-combined), up to `limit`
   * rows (default 100). Returns active-only nodes (`superseded_at IS NULL`).
   *
   * `predicates` must use paths from the allowlist (`$.status`, `$.priority`,
   * `$.tags`, `$.kind`, `$.created_at`); non-allowlisted paths throw `InvalidFilterError`.
   * Values are always bound as SQL parameters — injection-safe per ADR D-F4.
   * An empty or omitted `predicates` returns all active nodes of the kind (unfiltered).
   */
  async list(
    engine: Engine,
    kind: string,
    predicates?: Predicate[],
    limit = 100,
  ): Promise<NodeRecord[]> {
    validateFfiString(kind);
    validateLimit(limit);
    const nativePredicates = predicates?.map(toNativePredicate);
    const rows = await intercept(() =>
      native.readList(engine._native, kind, nativePredicates, limit),
    );
    return rows.map(toNodeRecord);
  },
};
