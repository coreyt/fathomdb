import {
  compiledGroupedQueryFromWire,
  compiledQueryFromWire,
  groupedQueryRowsFromWire,
  queryPlanFromWire,
  queryRowsFromWire,
  searchRowsFromWire,
  type CompiledGroupedQuery,
  type CompiledQuery,
  type GroupedQueryRows,
  type QueryAst,
  type QueryPlan,
  type QueryRows,
  type RawJson,
  type SearchRows,
} from "./types.js";
import { callNative, parseNativeJson } from "./errors.js";
import { runWithFeedback } from "./feedback.js";
import type { NativeEngineCore } from "./native.js";
import type { FeedbackConfig, ProgressCallback } from "./types.js";

type TraverseDirection = "in" | "out";

/**
 * Fluent, immutable query builder for fetching nodes from fathomdb.
 *
 * Instances are created via {@link Engine.nodes}. Each filter or traversal
 * method returns a new Query, leaving the original unchanged. Terminal
 * methods ({@link Query.execute}, {@link Query.compile}, {@link Query.explain})
 * send the assembled AST to the engine.
 */
export class Query {
  readonly #core: NativeEngineCore;
  readonly #rootKind: string;
  readonly #steps: Array<Record<string, RawJson>>;
  readonly #expansions: Array<Record<string, RawJson>>;
  readonly #finalLimit: number | null;

  constructor(
    core: NativeEngineCore,
    rootKind: string,
    steps: Array<Record<string, RawJson>> = [],
    expansions: Array<Record<string, RawJson>> = [],
    finalLimit: number | null = null
  ) {
    this.#core = core;
    this.#rootKind = rootKind;
    this.#steps = steps;
    this.#expansions = expansions;
    this.#finalLimit = finalLimit;
  }

  #withStep(step: Record<string, RawJson>): Query {
    return new Query(this.#core, this.#rootKind, [...this.#steps, step], this.#expansions, this.#finalLimit);
  }

  #withExpansion(expansion: Record<string, RawJson>): Query {
    return new Query(this.#core, this.#rootKind, this.#steps, [...this.#expansions, expansion], this.#finalLimit);
  }

  #withLimit(limit: number | null): Query {
    return new Query(this.#core, this.#rootKind, this.#steps, this.#expansions, limit);
  }

  /**
   * Serialize the query into its AST representation.
   *
   * @returns The query AST payload.
   */
  toAst(): QueryAst {
    return {
      root_kind: this.#rootKind,
      steps: this.#steps,
      expansions: this.#expansions,
      final_limit: this.#finalLimit
    };
  }

  /**
   * Add a vector similarity search step.
   *
   * @param query - The text query to embed and search against.
   * @param limit - Maximum number of nearest neighbours to return.
   * @returns A new Query with the vector search step appended.
   */
  vectorSearch(query: string, limit: number): Query {
    return this.#withStep({ type: "vector_search", query, limit });
  }

  /**
   * Start an adaptive full-text search rooted at the current query's kind.
   *
   * Returns a distinct {@link TextSearchBuilder} whose `.execute()` returns
   * {@link SearchRows}, not {@link QueryRows}. The adaptive pipeline tries
   * the strict query first and automatically derives a relaxed branch on
   * strict-miss.
   *
   * @param query - The FTS query string.
   * @param limit - Maximum number of candidate hits to return.
   * @returns A new {@link TextSearchBuilder} tethered to the engine core.
   */
  textSearch(query: string, limit: number): TextSearchBuilder {
    return new TextSearchBuilder(this.#core, this.#rootKind, query, limit);
  }

  /**
   * Traverse edges from matched nodes.
   *
   * @param args - Traversal configuration.
   * @param args.direction - `"in"` or `"out"` relative to current nodes.
   * @param args.label - Edge kind to follow.
   * @param args.maxDepth - Maximum traversal depth.
   * @returns A new Query with the traversal step appended.
   */
  traverse(args: { direction: TraverseDirection; label: string; maxDepth: number }): Query {
    return this.#withStep({
      type: "traverse",
      direction: args.direction,
      label: args.label,
      max_depth: args.maxDepth
    });
  }

  /**
   * Filter nodes to those with the given logical ID.
   *
   * @param logicalId - The logical ID to match.
   * @returns A new Query with the filter applied.
   */
  filterLogicalIdEq(logicalId: string): Query {
    return this.#withStep({ type: "filter_logical_id_eq", logical_id: logicalId });
  }

  /**
   * Filter nodes to those with the given kind.
   *
   * @param kind - The node kind to match.
   * @returns A new Query with the filter applied.
   */
  filterKindEq(kind: string): Query {
    return this.#withStep({ type: "filter_kind_eq", kind });
  }

  /**
   * Filter nodes to those with the given source reference.
   *
   * @param sourceRef - The source reference to match.
   * @returns A new Query with the filter applied.
   */
  filterSourceRefEq(sourceRef: string): Query {
    return this.#withStep({ type: "filter_source_ref_eq", source_ref: sourceRef });
  }

  /**
   * Filter nodes to those where `content_ref` is not NULL (i.e. content proxy nodes).
   *
   * @returns A new Query with the filter applied.
   */
  filterContentRefNotNull(): Query {
    return this.#withStep({ type: "filter_content_ref_not_null" });
  }

  /**
   * Filter nodes to those with the given `content_ref` URI.
   *
   * @param contentRef - The content reference URI to match.
   * @returns A new Query with the filter applied.
   */
  filterContentRefEq(contentRef: string): Query {
    return this.#withStep({ type: "filter_content_ref_eq", content_ref: contentRef });
  }

  /**
   * Filter nodes where the JSON property at `path` equals `value`.
   *
   * @param path - JSON path expression.
   * @param value - The string value to match.
   * @returns A new Query with the filter applied.
   */
  filterJsonTextEq(path: string, value: string): Query {
    return this.#withStep({ type: "filter_json_text_eq", path, value });
  }

  /**
   * Filter nodes where the JSON boolean at `path` equals `value`.
   *
   * @param path - JSON path expression.
   * @param value - The boolean value to match.
   * @returns A new Query with the filter applied.
   */
  filterJsonBoolEq(path: string, value: boolean): Query {
    return this.#withStep({ type: "filter_json_bool_eq", path, value });
  }

  /**
   * Filter nodes where the JSON integer at `path` is greater than `value`.
   *
   * @param path - JSON path expression.
   * @param value - The threshold value.
   * @returns A new Query with the filter applied.
   */
  filterJsonIntegerGt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_gt", path, value });
  }

  /**
   * Filter nodes where the JSON integer at `path` is greater than or equal to `value`.
   *
   * @param path - JSON path expression.
   * @param value - The threshold value.
   * @returns A new Query with the filter applied.
   */
  filterJsonIntegerGte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_gte", path, value });
  }

  /**
   * Filter nodes where the JSON integer at `path` is less than `value`.
   *
   * @param path - JSON path expression.
   * @param value - The threshold value.
   * @returns A new Query with the filter applied.
   */
  filterJsonIntegerLt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_lt", path, value });
  }

  /**
   * Filter nodes where the JSON integer at `path` is less than or equal to `value`.
   *
   * @param path - JSON path expression.
   * @param value - The threshold value.
   * @returns A new Query with the filter applied.
   */
  filterJsonIntegerLte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_lte", path, value });
  }

  /**
   * Filter nodes where the JSON timestamp at `path` is after `value`.
   *
   * @param path - JSON path expression.
   * @param value - The timestamp threshold.
   * @returns A new Query with the filter applied.
   */
  filterJsonTimestampGt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_gt", path, value });
  }

  /**
   * Filter nodes where the JSON timestamp at `path` is at or after `value`.
   *
   * @param path - JSON path expression.
   * @param value - The timestamp threshold.
   * @returns A new Query with the filter applied.
   */
  filterJsonTimestampGte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_gte", path, value });
  }

  /**
   * Filter nodes where the JSON timestamp at `path` is before `value`.
   *
   * @param path - JSON path expression.
   * @param value - The timestamp threshold.
   * @returns A new Query with the filter applied.
   */
  filterJsonTimestampLt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_lt", path, value });
  }

  /**
   * Filter nodes where the JSON timestamp at `path` is at or before `value`.
   *
   * @param path - JSON path expression.
   * @param value - The timestamp threshold.
   * @returns A new Query with the filter applied.
   */
  filterJsonTimestampLte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_lte", path, value });
  }

  /**
   * Register a named expansion slot for grouped query execution.
   *
   * @param args - Expansion configuration.
   * @param args.slot - Name for this expansion in the grouped result.
   * @param args.direction - `"in"` or `"out"` relative to root nodes.
   * @param args.label - Edge kind to follow.
   * @param args.maxDepth - Maximum traversal depth.
   * @returns A new Query with the expansion registered.
   */
  expand(args: { slot: string; direction: TraverseDirection; label: string; maxDepth: number }): Query {
    return this.#withExpansion({
      slot: args.slot,
      direction: args.direction,
      label: args.label,
      max_depth: args.maxDepth
    });
  }

  /**
   * Cap the number of result rows returned by the query.
   *
   * @param limit - Maximum number of rows to return.
   * @returns A new Query with the limit set.
   */
  limit(limit: number): Query {
    return this.#withLimit(limit);
  }

  /**
   * Compile the query into SQL without executing it.
   *
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns The compiled SQL query.
   */
  compile(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): CompiledQuery {
    return this.#run("query.compile", () =>
      compiledQueryFromWire(parseNativeJson(callNative(() => this.#core.compileAst(this.#astJson())))),
      progressCallback, feedbackConfig,
    );
  }

  /**
   * Compile the query with expansions into SQL without executing it.
   *
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns The compiled grouped SQL query.
   */
  compileGrouped(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): CompiledGroupedQuery {
    return this.#run("query.compile_grouped", () =>
      compiledGroupedQueryFromWire(parseNativeJson(callNative(() => this.#core.compileGroupedAst(this.#astJson())))),
      progressCallback, feedbackConfig,
    );
  }

  /**
   * Return the query execution plan without running the query.
   *
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns The query execution plan.
   */
  explain(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): QueryPlan {
    return this.#run("query.explain", () =>
      queryPlanFromWire(parseNativeJson(callNative(() => this.#core.explainAst(this.#astJson())))),
      progressCallback, feedbackConfig,
    );
  }

  /**
   * Execute the query and return matching rows.
   *
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns The matching rows.
   */
  execute(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): QueryRows {
    return this.#run("query.execute", () =>
      queryRowsFromWire(parseNativeJson(callNative(() => this.#core.executeAst(this.#astJson())))),
      progressCallback, feedbackConfig,
    );
  }

  /**
   * Execute the query with expansions and return grouped rows.
   *
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns The grouped result rows.
   */
  executeGrouped(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): GroupedQueryRows {
    return this.#run("query.execute_grouped", () =>
      groupedQueryRowsFromWire(parseNativeJson(callNative(() => this.#core.executeGroupedAst(this.#astJson())))),
      progressCallback, feedbackConfig,
    );
  }

  #run<T>(operationKind: string, operation: () => T, progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): T {
    return runWithFeedback({ operationKind, metadata: { root_kind: this.#rootKind }, progressCallback, feedbackConfig, operation });
  }

  #astJson(): string {
    return JSON.stringify(this.toAst());
  }

}

type SearchFilter = Record<string, RawJson>;

type SearchMode = "text_search" | "fallback_search";

function buildSearchRequest(args: {
  mode: SearchMode;
  rootKind: string;
  strictQuery: string;
  relaxedQuery: string | null;
  limit: number;
  filters: SearchFilter[];
  attributionRequested: boolean;
}): string {
  return JSON.stringify({
    mode: args.mode,
    root_kind: args.rootKind,
    strict_query: args.strictQuery,
    relaxed_query: args.relaxedQuery,
    limit: args.limit,
    filters: args.filters,
    attribution_requested: args.attributionRequested,
  });
}

function runSearch(
  core: NativeEngineCore,
  operationKind: string,
  rootKind: string,
  requestJson: string,
  progressCallback?: ProgressCallback,
  feedbackConfig?: FeedbackConfig,
): SearchRows {
  return runWithFeedback({
    operationKind,
    metadata: { root_kind: rootKind },
    progressCallback,
    feedbackConfig,
    operation: () =>
      searchRowsFromWire(parseNativeJson(callNative(() => core.executeSearch(requestJson)))),
  });
}

/**
 * Tethered builder for an adaptive text search.
 *
 * Created via {@link Query.textSearch}. Each filter method returns a new
 * builder, leaving the original unchanged. Terminal method
 * {@link TextSearchBuilder.execute} dispatches the request through the
 * native FFI and returns {@link SearchRows}.
 */
export class TextSearchBuilder {
  readonly #core: NativeEngineCore;
  readonly #rootKind: string;
  readonly #strictQuery: string;
  readonly #limit: number;
  readonly #filters: SearchFilter[];
  readonly #attributionRequested: boolean;

  constructor(
    core: NativeEngineCore,
    rootKind: string,
    strictQuery: string,
    limit: number,
    filters: SearchFilter[] = [],
    attributionRequested = false,
  ) {
    this.#core = core;
    this.#rootKind = rootKind;
    this.#strictQuery = strictQuery;
    this.#limit = limit;
    this.#filters = filters;
    this.#attributionRequested = attributionRequested;
  }

  #withFilter(filter: SearchFilter): TextSearchBuilder {
    return new TextSearchBuilder(
      this.#core,
      this.#rootKind,
      this.#strictQuery,
      this.#limit,
      [...this.#filters, filter],
      this.#attributionRequested,
    );
  }

  /** Request per-hit match attribution payloads from the engine. */
  withMatchAttribution(): TextSearchBuilder {
    return new TextSearchBuilder(
      this.#core,
      this.#rootKind,
      this.#strictQuery,
      this.#limit,
      this.#filters,
      true,
    );
  }

  /** Filter hits to those whose node kind equals `kind`. */
  filterKindEq(kind: string): TextSearchBuilder {
    return this.#withFilter({ type: "filter_kind_eq", kind });
  }

  /** Filter hits to those with the given logical ID. */
  filterLogicalIdEq(logicalId: string): TextSearchBuilder {
    return this.#withFilter({ type: "filter_logical_id_eq", logical_id: logicalId });
  }

  /** Filter hits to those with the given source reference. */
  filterSourceRefEq(sourceRef: string): TextSearchBuilder {
    return this.#withFilter({ type: "filter_source_ref_eq", source_ref: sourceRef });
  }

  /** Filter hits to those with the given content reference URI. */
  filterContentRefEq(contentRef: string): TextSearchBuilder {
    return this.#withFilter({ type: "filter_content_ref_eq", content_ref: contentRef });
  }

  /** Filter hits to those where `content_ref` is not NULL. */
  filterContentRefNotNull(): TextSearchBuilder {
    return this.#withFilter({ type: "filter_content_ref_not_null" });
  }

  /** Filter hits where the JSON property at `path` equals the string `value`. */
  filterJsonTextEq(path: string, value: string): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_text_eq", path, value });
  }

  /** Filter hits where the JSON boolean at `path` equals `value`. */
  filterJsonBoolEq(path: string, value: boolean): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_bool_eq", path, value });
  }

  /** Filter hits where the JSON integer at `path` is greater than `value`. */
  filterJsonIntegerGt(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_gt", path, value });
  }

  /** Filter hits where the JSON integer at `path` is greater than or equal to `value`. */
  filterJsonIntegerGte(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_gte", path, value });
  }

  /** Filter hits where the JSON integer at `path` is less than `value`. */
  filterJsonIntegerLt(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_lt", path, value });
  }

  /** Filter hits where the JSON integer at `path` is less than or equal to `value`. */
  filterJsonIntegerLte(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_lte", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is after `value`. */
  filterJsonTimestampGt(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_gt", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is at or after `value`. */
  filterJsonTimestampGte(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_gte", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is before `value`. */
  filterJsonTimestampLt(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_lt", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is at or before `value`. */
  filterJsonTimestampLte(path: string, value: number): TextSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_lte", path, value });
  }

  /**
   * Execute the search and return the matched rows.
   *
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns The matched {@link SearchRows}.
   */
  execute(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): SearchRows {
    const requestJson = buildSearchRequest({
      mode: "text_search",
      rootKind: this.#rootKind,
      strictQuery: this.#strictQuery,
      relaxedQuery: null,
      limit: this.#limit,
      filters: this.#filters,
      attributionRequested: this.#attributionRequested,
    });
    return runSearch(
      this.#core,
      "query.text_search",
      this.#rootKind,
      requestJson,
      progressCallback,
      feedbackConfig,
    );
  }
}

/**
 * Tethered builder for an explicit two-shape fallback search.
 *
 * Created via {@link Engine.fallbackSearch}. The caller provides both the
 * strict and relaxed queries (or `null` for strict-only). Unlike
 * {@link TextSearchBuilder}, this path is not subject to the adaptive branch
 * cap.
 */
export class FallbackSearchBuilder {
  readonly #core: NativeEngineCore;
  readonly #rootKind: string;
  readonly #strictQuery: string;
  readonly #relaxedQuery: string | null;
  readonly #limit: number;
  readonly #filters: SearchFilter[];
  readonly #attributionRequested: boolean;

  constructor(
    core: NativeEngineCore,
    rootKind: string,
    strictQuery: string,
    relaxedQuery: string | null,
    limit: number,
    filters: SearchFilter[] = [],
    attributionRequested = false,
  ) {
    this.#core = core;
    this.#rootKind = rootKind;
    this.#strictQuery = strictQuery;
    this.#relaxedQuery = relaxedQuery;
    this.#limit = limit;
    this.#filters = filters;
    this.#attributionRequested = attributionRequested;
  }

  #withFilter(filter: SearchFilter): FallbackSearchBuilder {
    return new FallbackSearchBuilder(
      this.#core,
      this.#rootKind,
      this.#strictQuery,
      this.#relaxedQuery,
      this.#limit,
      [...this.#filters, filter],
      this.#attributionRequested,
    );
  }

  /** Request per-hit match attribution payloads from the engine. */
  withMatchAttribution(): FallbackSearchBuilder {
    return new FallbackSearchBuilder(
      this.#core,
      this.#rootKind,
      this.#strictQuery,
      this.#relaxedQuery,
      this.#limit,
      this.#filters,
      true,
    );
  }

  /** Filter hits to those whose node kind equals `kind`. */
  filterKindEq(kind: string): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_kind_eq", kind });
  }

  /** Filter hits to those with the given logical ID. */
  filterLogicalIdEq(logicalId: string): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_logical_id_eq", logical_id: logicalId });
  }

  /** Filter hits to those with the given source reference. */
  filterSourceRefEq(sourceRef: string): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_source_ref_eq", source_ref: sourceRef });
  }

  /** Filter hits to those with the given content reference URI. */
  filterContentRefEq(contentRef: string): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_content_ref_eq", content_ref: contentRef });
  }

  /** Filter hits to those where `content_ref` is not NULL. */
  filterContentRefNotNull(): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_content_ref_not_null" });
  }

  /** Filter hits where the JSON property at `path` equals the string `value`. */
  filterJsonTextEq(path: string, value: string): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_text_eq", path, value });
  }

  /** Filter hits where the JSON boolean at `path` equals `value`. */
  filterJsonBoolEq(path: string, value: boolean): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_bool_eq", path, value });
  }

  /** Filter hits where the JSON integer at `path` is greater than `value`. */
  filterJsonIntegerGt(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_gt", path, value });
  }

  /** Filter hits where the JSON integer at `path` is greater than or equal to `value`. */
  filterJsonIntegerGte(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_gte", path, value });
  }

  /** Filter hits where the JSON integer at `path` is less than `value`. */
  filterJsonIntegerLt(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_lt", path, value });
  }

  /** Filter hits where the JSON integer at `path` is less than or equal to `value`. */
  filterJsonIntegerLte(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_integer_lte", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is after `value`. */
  filterJsonTimestampGt(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_gt", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is at or after `value`. */
  filterJsonTimestampGte(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_gte", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is before `value`. */
  filterJsonTimestampLt(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_lt", path, value });
  }

  /** Filter hits where the JSON timestamp at `path` is at or before `value`. */
  filterJsonTimestampLte(path: string, value: number): FallbackSearchBuilder {
    return this.#withFilter({ type: "filter_json_timestamp_lte", path, value });
  }

  /**
   * Execute the fallback search and return the matched rows.
   *
   * @param progressCallback - Optional callback invoked with feedback events.
   * @param feedbackConfig - Timing thresholds for progress feedback.
   * @returns The matched {@link SearchRows}.
   */
  execute(progressCallback?: ProgressCallback, feedbackConfig?: FeedbackConfig): SearchRows {
    const requestJson = buildSearchRequest({
      mode: "fallback_search",
      rootKind: this.#rootKind,
      strictQuery: this.#strictQuery,
      relaxedQuery: this.#relaxedQuery,
      limit: this.#limit,
      filters: this.#filters,
      attributionRequested: this.#attributionRequested,
    });
    return runSearch(
      this.#core,
      "query.fallback_search",
      this.#rootKind,
      requestJson,
      progressCallback,
      feedbackConfig,
    );
  }
}
