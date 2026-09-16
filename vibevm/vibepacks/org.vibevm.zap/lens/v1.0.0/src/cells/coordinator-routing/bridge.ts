/** Workspace-service composition bridge for coordinator routing. @scope spec://org.vibevm.zap/lens/PROP-008#model-routing */
import { ClientRequestIdSchema } from "../protocol/index.ts";
import {
  AttemptIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  WorkContextIdSchema,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import {
  ModelPolicyStoreAccessSchema,
  type ModelPolicyStore,
  type ModelPolicyStoreAccess,
} from "../model-policy-store/index.ts";
import { parametersFromSelection, resolveCoordinatorLaunch } from "./routing.ts";
import {
  CoordinatorRoutingRequestSchema,
  type CoordinatorRoutingResult,
  type CoordinatorRoutingResultValue,
  type CoordinatorRoutingProvider,
} from "./types.ts";

export const CoordinatorRoutingDefaultsSchema = CoordinatorRoutingRequestSchema.pick({
  purpose: true,
  taskClass: true,
  productId: true,
  productVersion: true,
})
  .extend({ policyEnabled: CoordinatorRoutingRequestSchema.shape.policyEnabled })
  .strict();
export type CoordinatorRoutingDefaults = (typeof CoordinatorRoutingDefaultsSchema)["_output"];
export interface WorkspaceCoordinatorRoutingBridge {
  resolve(
    access: ModelPolicyStoreAccess,
    input: BridgeStartInput,
  ): Promise<CoordinatorRoutingResult<CoordinatorRoutingResultValue>>;
  resume(
    access: ModelPolicyStoreAccess,
    input: BridgeResumeInput,
  ): Promise<CoordinatorRoutingResult<CoordinatorRoutingResultValue>>;
}
export interface BridgeStartInput {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly sessionId: string;
  readonly runId: string;
  readonly attemptId: string;
  readonly clientRequestId: string;
  readonly sourceEventId: string;
  readonly explicitProfileId: string;
}
export interface BridgeResumeInput {
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly sessionId: string;
  readonly runId: string;
  readonly attemptId: string;
  readonly explicitProfileId: string;
}

export function createWorkspaceCoordinatorRoutingBridge(options: {
  readonly store: ModelPolicyStore;
  readonly provider: CoordinatorRoutingProvider;
  readonly defaults: CoordinatorRoutingDefaults;
}): WorkspaceCoordinatorRoutingBridge {
  const defaults = CoordinatorRoutingDefaultsSchema.parse(options.defaults);
  return {
    async resolve(access, input) {
      const request = CoordinatorRoutingRequestSchema.parse({
        ...defaults,
        ...input,
        selectionRef: `selection:${input.sessionId}`,
        role: "coordinator",
        executionMode: "native",
        invocationScope: "coordinator",
        override: null,
      });
      return resolveCoordinatorLaunch(access, request, {
        store: options.store,
        provider: options.provider,
      });
    },
    async resume(access, input) {
      const parsedAccess = ModelPolicyStoreAccessSchema.safeParse(access);
      if (!parsedAccess.success)
        return {
          ok: false,
          error: { code: "unauthorized", message: "trusted routing access is malformed" },
        };
      const projectId = ProjectIdSchema.parse(input.projectId);
      const contextId = WorkContextIdSchema.parse(input.contextId);
      const runId = RunIdSchema.parse(input.runId);
      const attemptId = AttemptIdSchema.parse(input.attemptId);
      if (defaults.policyEnabled) {
        const stored = options.store.readSelection(
          parsedAccess.data,
          projectId,
          contextId,
          runId,
          attemptId,
        );
        if (!stored.ok)
          return {
            ok: false,
            error: {
              code: stored.error.code === "not_found" ? "not_found" : "storage_failure",
              message: stored.error.message,
            },
          };
        return {
          ok: true,
          value: {
            selection: stored.value.selection,
            parameters: parametersFromSelection(stored.value.selection),
            pinned: true,
          },
        };
      }
      const request = CoordinatorRoutingRequestSchema.parse({
        ...defaults,
        policyEnabled: false,
        projectId,
        contextId,
        sessionId: input.sessionId,
        runId,
        attemptId,
        clientRequestId: ClientRequestIdSchema.parse(`resume:${input.sessionId}`),
        sourceEventId: `resume:${input.sessionId}`,
        selectionRef: `selection:${input.sessionId}`,
        role: "coordinator",
        executionMode: "native",
        invocationScope: "coordinator",
        override: null,
        explicitProfileId: input.explicitProfileId,
      });
      return resolveCoordinatorLaunch(parsedAccess.data, request, {
        store: options.store,
        provider: options.provider,
      });
    },
  };
}
