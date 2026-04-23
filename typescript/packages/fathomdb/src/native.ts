import { createRequire } from "node:module";
import { mapNativeError } from "./errors.js";

const require = createRequire(import.meta.url);

export type NativeBinding = {
  EngineCore: {
    open(
      databasePath: string,
      provenanceMode: string,
      vectorDimension?: number,
      telemetryLevel?: string,
      embedder?: string,
      autoDrainVector?: boolean
    ): NativeEngineCore;
  };
  newId(): string;
  newRowId(): string;
  version?(): string;
  listTokenizerPresets(): Record<string, string>;
};

export type NativeEngineCore = {
  close(): void;
  telemetrySnapshot(): string;
  compileAst(astJson: string): string;
  compileGroupedAst(astJson: string): string;
  explainAst(astJson: string): string;
  executeAst(astJson: string): string;
  executeGroupedAst(astJson: string): string;
  executeSearch(requestJson: string): string;
  submitWrite(requestJson: string): string;
  touchLastAccessed(requestJson: string): string;
  checkIntegrity(): string;
  checkSemantics(): string;
  rebuildProjections(target: string): string;
  rebuildMissingProjections(): string;
  traceSource(sourceRef: string): string;
  exciseSource(sourceRef: string): string;
  restoreLogicalId(logicalId: string): string;
  purgeLogicalId(logicalId: string): string;
  safeExport(destinationPath: string, forceCheckpoint: boolean): string;
  // FTS property schema methods
  registerFtsPropertySchema(kind: string, propertyPathsJson: string, separator?: string): string;
  registerFtsPropertySchemaWithEntries(requestJson: string): string;
  registerFtsPropertySchemaAsync(kind: string, propertyPathsJson: string, separator?: string): string;
  getPropertyFtsRebuildProgress(kind: string): string;
  describeFtsPropertySchema(kind: string): string;
  listFtsPropertySchemas(): string;
  removeFtsPropertySchema(kind: string): string;
  // Operational collection methods
  registerOperationalCollection(requestJson: string): string;
  describeOperationalCollection(name: string): string;
  updateOperationalCollectionFilters(name: string, filterFieldsJson: string): string;
  updateOperationalCollectionValidation(name: string, validationJson: string): string;
  updateOperationalCollectionSecondaryIndexes(name: string, secondaryIndexesJson: string): string;
  traceOperationalCollection(collectionName: string, recordKey?: string): string;
  readOperationalCollection(requestJson: string): string;
  rebuildOperationalCurrent(collectionName?: string): string;
  validateOperationalCollectionHistory(collectionName: string): string;
  rebuildOperationalSecondaryIndexes(collectionName: string): string;
  planOperationalRetention(nowTimestamp: number, collectionNamesJson?: string, maxCollections?: number): string;
  runOperationalRetention(nowTimestamp: number, collectionNamesJson?: string, maxCollections?: number, dryRun?: boolean): string;
  disableOperationalCollection(name: string): string;
  compactOperationalCollection(name: string, dryRun: boolean): string;
  purgeOperationalCollection(name: string, beforeTimestamp: number): string;
  purgeProvenanceEvents(beforeTimestamp: number, optionsJson: string): string;
  // Projection profile methods
  setFtsProfile(requestJson: string): string;
  getFtsProfile(kind: string): string;
  setVecProfile(requestJson: string): string;
  getVecProfile(kind: string): string;
  previewProjectionImpact(kind: string, facet: string): string;
  restoreVectorProfiles(): string;
  regenerateVectorEmbeddings(configJson: string): string;
  drainVectorProjection(requestJson: string): string;
  // Pack H: introspection + batch configure
  capabilities(): string;
  currentConfig(): string;
  describeKind(kind: string): string;
  configureVecKinds(requestJson: string): string;
};

declare global {
  // eslint-disable-next-line no-var
  var __FATHOMDB_NATIVE_MOCK__: NativeBinding | undefined;
}

export function candidatePaths(): string[] {
  const dir = new URL(".", import.meta.url).pathname;
  return [
    process.env.FATHOMDB_NATIVE_BINDING ?? "",
    // Production paths — resolved relative to dist/
    `${dir}/../fathomdb.node`,
    `${dir}/../../fathomdb.node`,
    `${dir}/../fathomdb.${process.platform}-${process.arch}.node`,
    // Development / repo-local paths
    "../../../crates/fathomdb/index.node",
    "../../../../../target/debug/fathomdb.node"
  ].filter(Boolean);
}

let cachedBinding: NativeBinding | null = null;

export function loadNativeBinding(): NativeBinding {
  if (globalThis.__FATHOMDB_NATIVE_MOCK__) {
    return globalThis.__FATHOMDB_NATIVE_MOCK__;
  }
  if (cachedBinding) {
    return cachedBinding;
  }
  let lastError: unknown;
  for (const candidate of candidatePaths()) {
    try {
      cachedBinding = require(candidate) as NativeBinding;
      return cachedBinding;
    } catch (error) {
      lastError = error;
    }
  }
  throw mapNativeError(
    lastError ?? new Error("FATHOMDB_FATHOM_ERROR::native binding could not be loaded")
  );
}
