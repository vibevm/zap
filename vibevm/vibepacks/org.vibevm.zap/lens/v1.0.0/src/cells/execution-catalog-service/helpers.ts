/** Deterministic catalog service mutation helpers. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { createHash } from "node:crypto";
import {
  ExecutionCatalogSnapshotSchema,
  type ExecutionCatalogError,
  type ExecutionCatalogResult,
  type ExecutionCatalogSnapshot,
} from "../execution-catalog/index.ts";
import type { ReplaceCatalogSnapshotRequest } from "../execution-catalog-store/index.ts";

export function nextCatalog(
  snapshot: ExecutionCatalogSnapshot,
  clock: () => Date,
  patch: Partial<Pick<ExecutionCatalogSnapshot, "connections" | "configurations" | "usage">>,
): ExecutionCatalogSnapshot {
  return ExecutionCatalogSnapshotSchema.parse({
    ...snapshot,
    ...patch,
    catalogRevision: increment(snapshot.catalogRevision),
    updatedAt: clock().toISOString(),
  });
}

export function mutation(
  request: {
    readonly clientRequestId: ReplaceCatalogSnapshotRequest["clientRequestId"];
    readonly sourceEventId: string;
    readonly expectedCatalogRevision: ReplaceCatalogSnapshotRequest["expectedCatalogRevision"];
    readonly expectedPreferencesRevision: ReplaceCatalogSnapshotRequest["expectedPreferencesRevision"];
  },
  operation: ReplaceCatalogSnapshotRequest["operation"],
  subjectId: string,
  snapshot: ExecutionCatalogSnapshot,
): ReplaceCatalogSnapshotRequest {
  return {
    clientRequestId: request.clientRequestId,
    requestDigest: digest(request),
    sourceEventId: request.sourceEventId,
    expectedCatalogRevision: request.expectedCatalogRevision,
    expectedPreferencesRevision: request.expectedPreferencesRevision,
    operation,
    subjectId,
    snapshot,
  };
}

export function increment(value: string): string {
  return (BigInt(value) + 1n).toString();
}
export function upsert<T>(values: readonly T[], value: T, key: keyof T): T[] {
  return [...values.filter((candidate) => candidate[key] !== value[key]), value];
}
export function nameConflict(
  values: readonly {
    readonly displayName: string;
    readonly connectionId?: string;
    readonly configurationId?: string;
  }[],
  ownId: string,
  name: string,
): boolean {
  return values.some(
    (candidate) =>
      candidate.displayName.toLocaleLowerCase() === name.toLocaleLowerCase() &&
      ("configurationId" in candidate
        ? candidate.configurationId
        : "connectionId" in candidate
          ? candidate.connectionId
          : "") !== ownId,
  );
}
export function failure(
  code: ExecutionCatalogError["code"],
  message: string,
): ExecutionCatalogResult<never> {
  return { ok: false, error: { code, message } };
}
export function digest(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}
export function stableId(
  kind: "connection" | "configuration",
  principalId: string,
  requestId: string,
): string {
  return `${kind}.${digest({ kind, principalId, requestId }).slice(0, 24)}`;
}
