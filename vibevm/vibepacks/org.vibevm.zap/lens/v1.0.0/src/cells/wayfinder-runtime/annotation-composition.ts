/** Default annotation runtime composition. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import { resolve } from "node:path";
import type { ManagedAgentBackend } from "../managed-work/index.ts";
import type { RepositoryWorkspaceService } from "../repository-workspaces/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { annotationDeliveryMarkdown } from "../workspace-annotations/index.ts";
import type { WorkspacePlanningFeature } from "../workspace-planning/index.ts";
import type { WorkspaceService } from "../workspace-service/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "../workspace-model/index.ts";
import {
  openWayfinderAnnotationsRuntime,
  type AnnotationHistoryRecorder,
  type AnnotationNotificationPort,
  type AnnotationRestoreIntentPort,
  type WayfinderAnnotationsRuntime,
} from "./annotations.ts";

export function openRuntimeAnnotations(options: {
  readonly databasePath: string;
  readonly configuredPath?: string;
  readonly store: WorkspaceStore;
  readonly planning?: WorkspacePlanningFeature;
  readonly managedWork?: ManagedAgentBackend;
  readonly repositories?: RepositoryWorkspaceService;
  readonly restoreIntent?: AnnotationRestoreIntentPort;
}):
  | {
      readonly ok: true;
      readonly value: {
        readonly runtime: WayfinderAnnotationsRuntime;
        bindService(service: WorkspaceService): void;
      };
    }
  | { readonly ok: false; readonly message: string } {
  const serviceHolder: { value?: WorkspaceService } = {};
  const history: AnnotationHistoryRecorder = {
    record(input) {
      options.store.ingestEvent({
        projectId: ProjectIdSchema.parse(input.projectId),
        contextId: WorkContextIdSchema.parse(input.contextId),
        kind: input.kind,
        source: "lens",
        actorId: null,
        occurrenceAt: new Date().toISOString(),
        sourceEventId: `annotation.${input.kind}.${input.sourceEventId}`,
        correlationId: null,
        causationId: null,
        planProvenance: null,
        sourceSequence: null,
        payload: { sourceEventId: input.sourceEventId },
      });
    },
  };
  const notifications: AnnotationNotificationPort = {
    async enqueue(input) {
      if (serviceHolder.value === undefined)
        return failure("unavailable", "workspace service is not ready");
      const detail = options.store.read(input.access, {
        operation: "project.get.v1",
        projectId: input.note.projectId,
      });
      if (!detail.ok || detail.value.operation !== "project.get.v1")
        return failure("unavailable", "coordinator conversation is unavailable");
      const context = detail.value.detail.contexts.find(
        (item) => item.contextId === input.note.contextId,
      );
      if (context === undefined)
        return failure("not_found", "annotation work context is unavailable");
      const client = serviceHolder.value.bind({
        access: input.access,
        allowedActions: ["chat.post.v1"],
      });
      const posted = await client.command({
        operation: "chat.post.v1",
        clientRequestId: ClientRequestIdSchema.parse(
          `request.annotation.send.${input.note.noteId}.${input.expectedRevision}`,
        ),
        projectId: input.note.projectId,
        contextId: input.note.contextId,
        conversationId: context.coordinatorConversationId,
        bodyMarkdown: annotationDeliveryMarkdown(input.note),
        artifactRefs: [],
        correlationId: input.note.noteId,
        causationMessageId: null,
      });
      return posted.ok && posted.value.operation === "chat.post.v1"
        ? { ok: true, value: { observation: "queued", messageId: posted.value.message.messageId } }
        : failure("unavailable", "annotation instruction could not be queued");
    },
  };
  const opened = openWayfinderAnnotationsRuntime({
    databasePath: resolve(options.configuredPath ?? `${options.databasePath}.annotations`),
    workspaceStore: options.store,
    notifications,
    history,
    ...(options.planning === undefined ? {} : { planning: options.planning }),
    ...(options.managedWork === undefined ? {} : { managedWork: options.managedWork }),
    ...(options.repositories === undefined ? {} : { repositories: options.repositories }),
    ...(options.restoreIntent === undefined ? {} : { restoreIntent: options.restoreIntent }),
  });
  return opened.ok
    ? {
        ok: true,
        value: {
          runtime: opened.value,
          bindService(service) {
            serviceHolder.value = service;
          },
        },
      }
    : { ok: false, message: opened.error.message };
}

function failure(
  code: "unavailable" | "not_found",
  message: string,
): {
  readonly ok: false;
  readonly error: { readonly code: "unavailable" | "not_found"; readonly message: string };
} {
  return { ok: false, error: { code, message } };
}
