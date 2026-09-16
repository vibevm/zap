/** Durable scoped request identities. @scope spec://org.vibevm.zap/lens/PROP-014#recovery */
import type { RepositoryWorkspaceResult } from "./contracts.ts";
import { fail, requestDigest, type RepositoryWorkspaceRuntime } from "./runtime.ts";
import type { RepositoryOperation } from "./store.ts";

export type OperationResultKind = "repository" | "plan" | "worktree" | "integration";
export type OperationStart =
  | {
      readonly state: "new";
      readonly digest: string;
      readonly createdAt: string;
      readonly resultId: string;
    }
  | { readonly state: "resume"; readonly operation: RepositoryOperation }
  | { readonly state: "complete"; readonly operation: RepositoryOperation };

export function beginOperation(
  runtime: RepositoryWorkspaceRuntime,
  input: { readonly principalId: string; readonly requestId: string },
  scope: string,
  payload: unknown,
  reservation: { readonly kind: OperationResultKind; readonly id: string },
): RepositoryWorkspaceResult<OperationStart> {
  const digest = requestDigest(payload);
  const prior = runtime.store.getOperation(input.principalId, scope, input.requestId);
  if (prior !== null) {
    if (prior.digest !== digest)
      return fail("conflict", "request identity was already used for different content");
    return {
      ok: true,
      value:
        prior.state === "complete"
          ? { state: "complete", operation: prior }
          : { state: "resume", operation: prior },
    };
  }
  const createdAt = runtime.now();
  const stored = runtime.store.putOperation({
    principalId: input.principalId,
    scope,
    requestId: input.requestId,
    digest,
    state: "pending",
    resultKind: reservation.kind,
    resultId: reservation.id,
    errorCode: null,
    createdAt,
    updatedAt: createdAt,
  });
  if (!stored) return fail("unavailable", "repository operation intent could not be persisted");
  const claimed = runtime.store.getOperation(input.principalId, scope, input.requestId);
  if (claimed === null) return fail("unavailable", "repository operation intent is unavailable");
  if (claimed.digest !== digest)
    return fail("conflict", "request identity was concurrently used for different content");
  return claimed.createdAt === createdAt && claimed.resultId === reservation.id
    ? { ok: true, value: { state: "new", digest, createdAt, resultId: reservation.id } }
    : {
        ok: true,
        value:
          claimed.state === "complete"
            ? { state: "complete", operation: claimed }
            : { state: "resume", operation: claimed },
      };
}

export function completeOperation(
  runtime: RepositoryWorkspaceRuntime,
  input: { readonly principalId: string; readonly requestId: string },
  scope: string,
  digest: string,
  createdAt: string,
  resultKind: OperationResultKind,
  resultId: string,
): RepositoryWorkspaceResult<null> {
  return runtime.store.putOperation({
    principalId: input.principalId,
    scope,
    requestId: input.requestId,
    digest,
    state: "complete",
    resultKind,
    resultId,
    errorCode: null,
    createdAt,
    updatedAt: runtime.now(),
  })
    ? { ok: true, value: null }
    : fail("unavailable", "repository operation completion could not be persisted");
}

export function operationIdentity(start: OperationStart): { digest: string; createdAt: string } {
  return start.state === "new"
    ? { digest: start.digest, createdAt: start.createdAt }
    : { digest: start.operation.digest, createdAt: start.operation.createdAt };
}
