import {
  compiledGroupedQueryFromWire,
  compiledQueryFromWire,
  groupedQueryRowsFromWire,
  queryPlanFromWire,
  queryRowsFromWire,
  type CompiledGroupedQuery,
  type CompiledQuery,
  type GroupedQueryRows,
  type QueryAst,
  type QueryPlan,
  type QueryRows,
  type RawJson,
} from "./types.js";
import { parseNativeJson } from "./errors.js";
import type { NativeEngineCore } from "./native.js";

type TraverseDirection = "in" | "out";

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

  toAst(): QueryAst {
    return {
      root_kind: this.#rootKind,
      steps: this.#steps,
      expansions: this.#expansions,
      final_limit: this.#finalLimit
    };
  }

  vectorSearch(query: string, limit: number): Query {
    return this.#withStep({ type: "vector_search", query, limit });
  }

  textSearch(query: string, limit: number): Query {
    return this.#withStep({ type: "text_search", query, limit });
  }

  traverse(args: { direction: TraverseDirection; label: string; maxDepth: number }): Query {
    return this.#withStep({
      type: "traverse",
      direction: args.direction,
      label: args.label,
      max_depth: args.maxDepth
    });
  }

  filterLogicalIdEq(logicalId: string): Query {
    return this.#withStep({ type: "filter_logical_id_eq", logical_id: logicalId });
  }

  filterKindEq(kind: string): Query {
    return this.#withStep({ type: "filter_kind_eq", kind });
  }

  filterSourceRefEq(sourceRef: string): Query {
    return this.#withStep({ type: "filter_source_ref_eq", source_ref: sourceRef });
  }

  filterJsonTextEq(path: string, value: string): Query {
    return this.#withStep({ type: "filter_json_text_eq", path, value });
  }

  filterJsonBoolEq(path: string, value: boolean): Query {
    return this.#withStep({ type: "filter_json_bool_eq", path, value });
  }

  filterJsonIntegerGt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_gt", path, value });
  }

  filterJsonIntegerGte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_gte", path, value });
  }

  filterJsonIntegerLt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_lt", path, value });
  }

  filterJsonIntegerLte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_integer_lte", path, value });
  }

  filterJsonTimestampGt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_gt", path, value });
  }

  filterJsonTimestampGte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_gte", path, value });
  }

  filterJsonTimestampLt(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_lt", path, value });
  }

  filterJsonTimestampLte(path: string, value: number): Query {
    return this.#withStep({ type: "filter_json_timestamp_lte", path, value });
  }

  expand(args: { slot: string; direction: TraverseDirection; label: string; maxDepth: number }): Query {
    return this.#withExpansion({
      slot: args.slot,
      direction: args.direction,
      label: args.label,
      max_depth: args.maxDepth
    });
  }

  limit(limit: number): Query {
    return this.#withLimit(limit);
  }

  compile(): CompiledQuery {
    return compiledQueryFromWire(parseNativeJson(this.#core.compileAst(this.#astJson())));
  }

  compileGrouped(): CompiledGroupedQuery {
    return compiledGroupedQueryFromWire(parseNativeJson(this.#core.compileGroupedAst(this.#astJson())));
  }

  explain(): QueryPlan {
    return queryPlanFromWire(parseNativeJson(this.#core.explainAst(this.#astJson())));
  }

  execute(): QueryRows {
    return queryRowsFromWire(parseNativeJson(this.#core.executeAst(this.#astJson())));
  }

  executeGrouped(): GroupedQueryRows {
    return groupedQueryRowsFromWire(parseNativeJson(this.#core.executeGroupedAst(this.#astJson())));
  }

  #astJson(): string {
    return JSON.stringify(this.toAst());
  }

}
