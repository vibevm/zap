/** Trusted managed-agent runtime composition. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { randomUUID } from "node:crypto";
import {
  createManagedAgentBackend,
  createManagedProviderDrivers,
  openManagedWorkStore,
  type ManagedActorBindingPort,
  type ManagedAgentBackend,
  type ManagedAgentProfile,
  type ManagedExecutionFencePort,
  type ManagedWorkStore,
  type WorkAttachmentPort,
  type ManagedParentPort,
  type ManagedWorkResult,
  type ManagedWorkspaceProvisioningPort,
  type ManagedProviderControlAdapter,
  type ManagedSessionControlPort,
  type ProtectedEnvironmentPort,
} from "../managed-work/index.ts";
import type { ManagedTerminalServicePort } from "../managed-terminal-service/index.ts";
import type { ModelPolicyStore } from "../model-policy-store/index.ts";
import type { CoordinatorRoutingProvider } from "../coordinator-routing/index.ts";
import type { ProxyPolicy } from "../proxy-policy/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import {
  createAnnotationWorkAttachmentPort,
  type AnnotationTargetResolver,
  type AnnotationStore,
} from "../workspace-annotations/index.ts";
import { createManagedControlComposition } from "./managed-control-composition.ts";
import { createDefaultManagedEnvironment } from "./managed-environment.ts";
import type { ExecutionAccountIsolationPort } from "../execution-accounts/index.ts";
import type { ManagedCatalogRuntime } from "./managed-catalog-selection.ts";
import { createManagedSelectionPort } from "./managed-selection.ts";

export interface ConfiguredManagedWorkRuntime {
  readonly backend: ManagedAgentBackend;
  readonly control: ManagedSessionControlPort;
  readonly store: ManagedWorkStore;
  close(): void;
}

export function openConfiguredManagedWorkRuntime(input: {
  readonly profiles: readonly ManagedAgentProfile[];
  readonly databasePath: string;
  readonly terminals: ManagedTerminalServicePort;
  readonly bindings: ManagedActorBindingPort;
  readonly policyStore: ModelPolicyStore;
  readonly routingProvider: CoordinatorRoutingProvider | undefined;
  readonly attachments?: WorkAttachmentPort;
  readonly annotationStore?: AnnotationStore;
  readonly annotationResolver?: AnnotationTargetResolver;
  readonly proxyPolicy?: ProxyPolicy;
  readonly environment?: ProtectedEnvironmentPort;
  readonly accounts?: ExecutionAccountIsolationPort;
  readonly executionCatalog?: ManagedCatalogRuntime;
  readonly workspaceStore: WorkspaceStore;
  readonly controlAdapters?: readonly ManagedProviderControlAdapter[];
  readonly workspaces?: ManagedWorkspaceProvisioningPort;
  readonly canOpenWriter?: (
    access: Parameters<ManagedExecutionFencePort["canStart"]>[0],
    projectId: string,
    contextId: string,
  ) => ManagedWorkResult<null>;
}):
  | { readonly ok: true; readonly value: ConfiguredManagedWorkRuntime }
  | { readonly ok: false; readonly message: string } {
  const opened = openManagedWorkStore(input.databasePath);
  if (!opened.ok) return { ok: false, message: opened.error.message };
  const store = opened.value;
  const control = createManagedControlComposition({
    terminals: input.terminals,
    directory: `${input.databasePath}.control`,
    adapters: input.controlAdapters ?? [],
  });
  const backend = createManagedAgentBackend({
    store,
    terminals: input.terminals,
    profiles: input.profiles,
    drivers: createManagedProviderDrivers(
      input.proxyPolicy === undefined ? {} : { proxyPolicy: input.proxyPolicy },
    ),
    environment: input.environment ?? createDefaultManagedEnvironment(),
    ...(input.accounts === undefined ? {} : { accounts: input.accounts }),
    bindings: input.bindings,
    selections: createManagedSelectionPort(
      input.policyStore,
      input.routingProvider,
      store,
      input.executionCatalog,
    ),
    parents: parentPort(store, input.workspaceStore),
    attachments:
      input.attachments ??
      (input.annotationStore === undefined
        ? attachmentPort()
        : createAnnotationWorkAttachmentPort({
            store: input.annotationStore,
            ...(input.annotationResolver === undefined
              ? {}
              : { resolver: input.annotationResolver }),
          })),
    execution: {
      canStart(access, claim) {
        const writer = input.canOpenWriter?.(
          access,
          claim.packet.projectId,
          claim.packet.contextId,
        );
        if (writer !== undefined && !writer.ok) return writer;
        const execution = input.workspaceStore.readProjectExecution(
          claim.packet.projectId,
          claim.packet.contextId,
        );
        if (!execution.ok)
          return {
            ok: false,
            error: { code: "unavailable", message: execution.error.message },
          };
        return access.authorizedProjectIds.includes(claim.packet.projectId) &&
          (execution.value.state === "uninitialized" || execution.value.state === "running")
          ? { ok: true, value: null }
          : {
              ok: false,
              error: { code: "conflict", message: "project execution gate blocks managed start" },
            };
      },
    },
    control,
    ...(input.workspaces === undefined ? {} : { workspaces: input.workspaces }),
    id: (kind) => `${kind}.${randomUUID().replaceAll("-", "")}`,
  });
  return {
    ok: true,
    value: {
      backend,
      control,
      store,
      close() {
        control.close();
        store.close();
      },
    },
  };
}

function parentPort(store: ManagedWorkStore, workspaceStore: WorkspaceStore): ManagedParentPort {
  return {
    validate(access, request) {
      if (!access.authorizedProjectIds.includes(request.projectId))
        return {
          ok: false,
          error: { code: "forbidden", message: "parent managed work is outside scope" },
        };
      if ((request.parentTaskId === null) !== (request.parentRunId === null))
        return {
          ok: false,
          error: { code: "invalid_input", message: "parent task and run must be paired" },
        };
      if (request.depth > 32)
        return {
          ok: false,
          error: { code: "forbidden", message: "managed work nesting depth is bounded" },
        };
      let parentActorId: string | null = null;
      if (request.parentRunId !== null) {
        const parent = store.load(request.parentRunId);
        if (!parent.ok)
          return {
            ok: false,
            error: { code: "forbidden", message: "parent managed run is unavailable" },
          };
        if (
          parent.value.taskId !== request.parentTaskId ||
          parent.value.packet.projectId !== request.projectId ||
          parent.value.packet.contextId !== request.contextId ||
          request.projectedParentActorId !== parent.value.actorId ||
          ["failed", "stopped", "uncertain"].includes(parent.value.state) ||
          request.depth !== parent.value.packet.depth + 1
        )
          return {
            ok: false,
            error: { code: "forbidden", message: "parent managed run is outside scope" },
          };
        parentActorId = parent.value.actorId;
      } else {
        if (request.depth !== 0)
          return {
            ok: false,
            error: { code: "invalid_input", message: "root managed work must have depth zero" },
          };
        if (request.projectedParentActorId !== null) {
          const network = workspaceStore.read(access, {
            operation: "agent.network.v1",
            projectId: request.projectId,
            contextId: request.contextId,
          });
          if (
            !network.ok ||
            network.value.operation !== "agent.network.v1" ||
            !network.value.network.agents.some(
              (agent) =>
                agent.actorId === request.projectedParentActorId && agent.role === "coordinator",
            )
          )
            return {
              ok: false,
              error: { code: "forbidden", message: "coordinator parent actor is unavailable" },
            };
          parentActorId = request.projectedParentActorId;
        }
      }
      return {
        ok: true,
        value: {
          parentTaskId: request.parentTaskId,
          parentRunId: request.parentRunId,
          parentActorId,
          depth: request.depth,
        },
      };
    },
  };
}

function attachmentPort(): WorkAttachmentPort {
  return {
    async prepareBeforeWork(input) {
      await Promise.resolve();
      return {
        ok: true,
        value: {
          state: input.targets.length === 0 ? ("ready" as const) : ("waiting_for_target" as const),
          instructions: [],
        },
      };
    },
    async acknowledge() {
      await Promise.resolve();
      return { ok: true, value: null };
    },
  };
}
