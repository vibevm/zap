/** Scoped annotation service and passive/deferred delivery hooks. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import type {
  AnnotationCommandRequest,
  AnnotationCommandResponse,
  AnnotationNote,
  AnnotationReadRequest,
  AnnotationReadResponse,
  AnnotationSourceObservation,
  AnnotationTrashEntry,
  ProjectObjectReference,
  WorkContextId,
} from "../workspace-model/index.ts";
import type { AnnotationTargetResolver } from "./attachments.ts";
import type { AnnotationErrorCode, AnnotationResult, AnnotationStore } from "./types.ts";

type AnnotationAccess = Parameters<AnnotationStore["read"]>[0];

export interface AnnotationNotificationPort {
  enqueue(input: {
    readonly access: AnnotationAccess;
    readonly note: AnnotationNote;
    readonly mode: "manual_send";
    readonly clientRequestId: string;
    readonly expectedRevision: string;
  }): Promise<
    | {
        readonly ok: true;
        readonly value: {
          readonly observation: "offered" | "queued";
          readonly messageId: string | null;
        };
      }
    | {
        readonly ok: false;
        readonly error: { readonly code: AnnotationErrorCode; readonly message: string };
      }
  >;
}

export interface AnnotationService {
  read(
    access: AnnotationAccess,
    request: AnnotationReadRequest,
  ): AnnotationResult<AnnotationReadResponse>;
  command(
    access: AnnotationAccess,
    request: AnnotationCommandRequest,
  ): Promise<AnnotationResult<AnnotationCommandResponse>>;
  observeSource(
    access: AnnotationAccess,
    observation: AnnotationSourceObservation,
  ): AnnotationResult<readonly AnnotationTrashEntry[]>;
  sendNow(
    access: AnnotationAccess,
    input: {
      readonly projectId: AnnotationAccess["authorizedProjectIds"][number];
      readonly contextId: WorkContextId;
      readonly noteId: string;
    },
  ): Promise<
    AnnotationResult<{
      readonly observation: "offered" | "queued";
      readonly messageId: string | null;
    }>
  >;
  readonly resolver: AnnotationTargetResolver | undefined;
}

export interface AnnotationRestoreIntentPort {
  create(input: {
    readonly access: AnnotationAccess;
    readonly projectId: AnnotationAccess["authorizedProjectIds"][number];
    readonly contextId: WorkContextId;
    readonly trashId: string;
    readonly expectedRevision: string;
  }): Promise<AnnotationResult<{ readonly intentRef: string }>>;
}

export function createAnnotationService(options: {
  readonly store: AnnotationStore;
  readonly resolver?: AnnotationTargetResolver;
  readonly notifications?: AnnotationNotificationPort;
  readonly restoreIntent?: AnnotationRestoreIntentPort;
}): AnnotationService {
  return {
    resolver: options.resolver,
    read: (access, request) => options.store.read(access, request),
    async command(access, request) {
      if (request.operation === "annotation.note.send.v1") {
        if (options.notifications === undefined)
          return failure("unavailable", "annotation notification delivery is not configured");
        const read = options.store.read(access, {
          operation: "annotation.note.get.v1",
          projectId: request.projectId,
          contextId: request.contextId,
          noteId: request.noteId,
        });
        if (!read.ok) return failure(read.error.code, read.error.message);
        if (read.value.operation !== "annotation.note.get.v1")
          return failure("storage_failure", "annotation note read returned another operation");
        if (read.value.note.currentVersion !== request.expectedRevision)
          return failure("stale_revision", "annotation note revision changed");
        if (read.value.note.state !== "active" || read.value.note.kind !== "deferred")
          return failure("conflict", "only active deferred instructions can be sent");
        const queued = await options.notifications.enqueue({
          access,
          note: read.value.note,
          mode: "manual_send",
          clientRequestId: request.clientRequestId,
          expectedRevision: request.expectedRevision,
        });
        return queued.ok
          ? { ok: true, value: { operation: request.operation, ...queued.value } }
          : failure(queued.error.code, queued.error.message);
      }
      if (request.operation === "annotation.object.restore.intent.v1") {
        if (options.restoreIntent === undefined)
          return failure("unavailable", "object restore intent routing is not configured");
        const intent = await options.restoreIntent.create({
          access,
          projectId: request.projectId,
          contextId: request.contextId,
          trashId: request.trashId,
          expectedRevision: request.expectedRevision,
        });
        return intent.ok
          ? { ok: true, value: { operation: request.operation, intentRef: intent.value.intentRef } }
          : intent;
      }
      if (request.operation === "annotation.note.restore.v1") {
        if (options.resolver === undefined)
          return failure("unavailable", "trusted target resolution is required for note restore");
        if (!access.authorizedProjectIds.includes(request.projectId))
          return failure("forbidden", "annotation project is outside authenticated scope");
        const trash = options.store.readTrash(access, request.trashId);
        if (!trash.ok) return failure(trash.error.code, trash.error.message);
        if (trash.value.entryKind !== "note")
          return failure("conflict", "only a trashed note can be restored as a note");
        const scope = validateTargetScope(
          access,
          request.projectId,
          request.contextId,
          trash.value.target,
        );
        if (!scope.ok) return scope;
        const resolved = await options.resolver.resolve(trash.value.target);
        if (resolved.state === "missing")
          return failure("conflict", "annotation target is missing; explicit relink is required");
        if (resolved.state === "unavailable") return failure("unavailable", resolved.reason);
        return options.store.command(access, request);
      }
      if (
        request.operation !== "annotation.note.create.v1" &&
        request.operation !== "annotation.note.update.v1" &&
        request.operation !== "annotation.note.relink.v1"
      )
        return options.store.command(access, request);
      if (options.resolver === undefined)
        return failure("unavailable", "trusted target resolution is required for note writes");
      let target: ProjectObjectReference;
      if (request.operation === "annotation.note.create.v1") {
        target = request.target;
      } else {
        const existing = options.store.read(access, {
          operation: "annotation.note.get.v1",
          projectId: request.projectId,
          contextId: request.contextId,
          noteId: request.noteId,
        });
        if (!existing.ok) return failure(existing.error.code, existing.error.message);
        if (existing.value.operation !== "annotation.note.get.v1")
          return failure("storage_failure", "annotation note read returned another operation");
        target =
          request.operation === "annotation.note.relink.v1"
            ? request.target
            : existing.value.note.target;
      }
      const scope = validateTargetScope(access, request.projectId, request.contextId, target);
      if (!scope.ok) return scope;
      const resolved = await options.resolver.resolve(target);
      if (resolved.state === "missing")
        return failure("not_found", "annotation target is no longer present");
      if (resolved.state === "unavailable") return failure("unavailable", resolved.reason);
      return options.store.command(access, {
        ...request,
        sourceBasisRef: resolved.snapshot.basisRef,
        targetSnapshot: resolved.snapshot,
      });
    },
    observeSource: (access, observation) => options.store.observeSource(access, observation),
    async sendNow(access, input) {
      if (options.notifications === undefined)
        return failure("unavailable", "annotation notification delivery is not configured");
      const read = options.store.read(access, {
        operation: "annotation.note.get.v1",
        projectId: input.projectId,
        contextId: input.contextId,
        noteId: input.noteId,
      });
      if (!read.ok) return failure(read.error.code, read.error.message);
      if (read.value.operation !== "annotation.note.get.v1")
        return failure("storage_failure", "annotation note read returned another operation");
      if (read.value.note.state !== "active" || read.value.note.kind !== "deferred")
        return failure("conflict", "only active deferred instructions can be sent");
      const queued = await options.notifications.enqueue({
        access,
        note: read.value.note,
        mode: "manual_send",
        clientRequestId: "manual-send",
        expectedRevision: read.value.note.currentVersion,
      });
      return queued.ok ? queued : queued;
    },
  };
}

function failure<T>(code: AnnotationErrorCode, message: string): AnnotationResult<T> {
  return { ok: false, error: { code, message } };
}

function validateTargetScope(
  access: AnnotationAccess,
  projectId: ProjectObjectReference["projectId"],
  contextId: ProjectObjectReference["contextId"],
  target: ProjectObjectReference,
): AnnotationResult<null> {
  if (!access.authorizedProjectIds.includes(projectId))
    return failure("forbidden", "annotation project is outside authenticated scope");
  if (target.projectId !== projectId || target.contextId !== contextId)
    return failure("forbidden", "annotation target is outside command scope");
  if (target.domain === "project" && target.ref !== target.projectId)
    return failure("invalid_input", "project annotation target must reference its project ID");
  return { ok: true, value: null };
}
