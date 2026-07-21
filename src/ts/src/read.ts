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
  type NativeFilterTermInput,
  type NativeNodeRecord,
  type NativeOpStoreRow,
  type NativePredicateInput,
} from "./binding.js";
import { InvalidArgumentError, rethrowTyped } from "./errors.js";
import { validateFfiString } from "./validation.js";
import type {
  Engine,
  Filter,
  FilterTerm,
  ProjectionRole,
  ProjectionSpec,
} from "./index.js";

/** Slice 30 (G2) — an active canonical node row from `read.get`/`read.getMany`. */
/**
 * 0.8.20 Slice 10b (R-20-RV / R-20-NV) — the read view.
 *
 * Every field is a RELAXATION and every default is the STRICT view, so omitting
 * `view` entirely reproduces the shipped read behaviour exactly. Flags compose
 * independently: each drops exactly one predicate and no other.
 *
 * Mirrors the Python `ReadView` (cross-binding parity; `camelCase` here,
 * `snake_case` there).
 *
 * World-time only — there is deliberately no `historyAsOf`.
 */
export interface ReadView {
  /** Relax `superseded_at IS NULL` — include historical versions. */
  includeSuperseded?: boolean;
  /** Relax `state = 'active'` — include non-active lifecycle states. */
  includeInactive?: boolean;
  /** Relax the validity window entirely (ignores `validAsOf`). */
  includeOutOfWindow?: boolean;
  /** Validity instant, INTEGER epoch SECONDS. Omitted means now. */
  validAsOf?: number;
}

/**
 * 0.8.20 Slice 10b (R-20-NV) — one node that crossed a validity boundary.
 *
 * A node whose window opened AND closed inside the interrogated interval
 * carries both fields, so they are independent nullables rather than an enum.
 *
 * Both boundary fields are ALWAYS PRESENT on this PUBLIC type and are
 * `number | null` — `null` means "this boundary was not crossed in the
 * interrogated interval".
 *
 * That `null` is manufactured HERE, not by napi. The native layer OMITS the
 * property when the Rust `Option<i64>` is `None` (see `NativeBoundaryCrossing`
 * in binding.ts for the napi-derive codegen that proves it), so the mapper
 * below normalises `undefined → null` with `?? null` — the same normalisation
 * `index.ts` already applies to `ceScore`. Public consumers therefore see one
 * shape, and it mirrors the Python `BoundaryCrossing`, whose fields are
 * `int | None` (cross-binding parity; `camelCase` here, `snake_case` there).
 */
export interface BoundaryCrossing {
  node: NodeRecord;
  becameValidAt: number | null;
  becameInvalidAt: number | null;
}

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
    validateFfiString(pred.value);
    native.valueStr = pred.value;
  }
  return native;
}

/**
 * 0.8.11 Slice 40 (#17) — convert a unified `FilterTerm` to the native
 * `NativeFilterTermInput` shape. String values clear the FFI surrogate guard;
 * the engine performs the authoritative total dispatch (constant-folds +
 * `json_extract`).
 */
function toNativeFilterTerm(t: FilterTerm): NativeFilterTermInput {
  switch (t.term) {
    case "source_type":
      validateFfiString(t.value);
      return { term: "source_type", valueStr: t.value };
    case "kind":
      validateFfiString(t.value);
      return { term: "kind", valueStr: t.value };
    case "created_after":
      if (!Number.isInteger(t.value)) {
        throw new InvalidArgumentError(
          `created_after must be an integer (unix seconds); got ${t.value}`,
        );
      }
      return { term: "created_after", valueInt: t.value };
    case "status":
      validateFfiString(t.value);
      return { term: "status", valueStr: t.value };
    case "json":
      return { term: "json", predicate: toNativePredicate(t.predicate) };
  }
}

export const read = {
  /**
   * `read.get` — return the ACTIVE node carrying `logicalId`, or `null` if
   * absent. Active-only (`superseded_at IS NULL`): a superseded version is never
   * returned. A missing/superseded id is a normal `null`, not a thrown error.
   */
  async get(engine: Engine, logicalId: string, view?: ReadView): Promise<NodeRecord | null> {
    validateFfiString(logicalId);
    const n = await intercept(() => native.readGet(engine._native, logicalId, view));
    return n === null ? null : toNodeRecord(n);
  },

  /**
   * `read.getMany` — return one slot per requested id, in REQUEST ORDER. A
   * missing/superseded id yields `null` in its slot (partial, never
   * all-or-nothing).
   */
  async getMany(
    engine: Engine,
    logicalIds: string[],
    view?: ReadView,
  ): Promise<(NodeRecord | null)[]> {
    for (const id of logicalIds) validateFfiString(id);
    const rows = await intercept(() => native.readGetMany(engine._native, logicalIds, view));
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
    filter?: Filter,
    view?: ReadView,
  ): Promise<NodeRecord[]> {
    validateFfiString(kind);
    validateLimit(limit);
    // 0.8.11 Slice 40 (#17): the additive unified-grammar path. Passing `filter`
    // routes to the engine's authoritative total dispatch (`json` →
    // `json_extract`; `status`/`created_after` → allowlisted json-paths;
    // `kind`/`source_type` constant-fold vs the partition `kind` — a
    // contradicting fold returns `[]` without touching SQL). `predicates` and
    // `filter` are mutually exclusive. This stays the SAME governed `read.list`
    // verb (no new surface member). Mirrors the Python `read.list(filter=...)`.
    if (filter !== undefined) {
      if (predicates !== undefined && predicates.length > 0) {
        throw new InvalidArgumentError(
          "read.list: pass either `predicates` or `filter`, not both",
        );
      }
      const terms = filter.terms.map(toNativeFilterTerm);
      const rows = await intercept(() =>
        native.readListFilter(engine._native, kind, terms, limit, view),
      );
      return rows.map(toNodeRecord);
    }
    const nativePredicates = predicates?.map(toNativePredicate);
    const rows = await intercept(() =>
      native.readList(engine._native, kind, nativePredicates, limit, view),
    );
    return rows.map(toNodeRecord);
  },

  /**
   * `read.crossedBoundarySince` (R-20-NV) — nodes that crossed a validity
   * boundary in `(since, asOf]`.
   *
   * `since` is an INTEGER epoch-second instant and the upper bound is the
   * view's own `validAsOf` (defaulting to now). Both are bound parameters, so
   * the answer is deterministic for a fixed pair.
   *
   * A node whose window opened AND closed inside the interval reports both
   * boundaries. Rows with no window (every row predating schema step 22) can
   * never cross one, so they never appear.
   *
   * World-time only — this asks what was true in the world, never what the
   * database believed.
   */
  async crossedBoundarySince(
    engine: Engine,
    since: number,
    view?: ReadView,
  ): Promise<BoundaryCrossing[]> {
    if (!Number.isInteger(since)) {
      throw new InvalidArgumentError(
        `read.crossedBoundarySince requires an integer \`since\`; got ${since}`,
      );
    }
    const rows = await intercept(() =>
      native.crossedBoundarySince(engine._native, since, view),
    );
    return rows.map((c) => ({
      node: toNodeRecord(c.node),
      // napi OMITS these when the Rust `Option` is `None` → `undefined`.
      // Normalise to `null` so the public shape is total (see the doc comment
      // on `BoundaryCrossing`).
      becameValidAt: c.becameValidAt ?? null,
      becameInvalidAt: c.becameInvalidAt ?? null,
    }));
  },

  /**
   * 0.8.20 Slice 15d (R-20-PR) — `read.projections` introspection. Returns every
   * declared {@link ProjectionSpec} (sorted by name), so a caller can inspect
   * current registry state — and the destructive delta a change would cause —
   * BEFORE calling `Engine.configureProjections`. Pure read.
   */
  async projections(engine: Engine): Promise<ProjectionSpec[]> {
    const specs = await rethrowTyped(() => engine._native.readProjections());
    return specs.map((s) => ({
      name: s.name,
      roles: s.roles as ProjectionRole[],
      fts: s.fts,
      ftsTokenizer: s.ftsTokenizer ?? null,
      vector: s.vector,
      vectorEmbedder: s.vectorEmbedder ?? null,
    }));
  },
};
