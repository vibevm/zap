/** Dynamic project authorization under one Wayfinder owner. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  WorkspaceAccessContextSchema,
  type WorkspaceClientPort,
  type WorkspaceResult,
  type ProjectId,
} from "../workspace-model/index.ts";
import {
  WORKSPACE_SERVICE_ACTIONS,
  type WorkspaceService,
  type WorkspaceServiceAction,
} from "../workspace-service/index.ts";
import type { ProductAppService } from "./service.ts";

export function createDynamicWorkspacePort(options: {
  readonly service: WorkspaceService;
  readonly product: ProductAppService;
  readonly clientId: string;
  readonly principalId: string;
  readonly baselineProjectIds?: readonly ProjectId[];
  readonly allowedActions?: readonly WorkspaceServiceAction[];
}): WorkspaceClientPort {
  const current = (): WorkspaceClientPort => {
    const projectIds = [
      ...(options.baselineProjectIds ?? []),
      ...options.product
        .projectIds()
        .filter((projectId) => !(options.baselineProjectIds ?? []).includes(projectId)),
    ];
    if (projectIds.length === 0) return emptyWorkspacePort();
    return options.service.bind({
      access: WorkspaceAccessContextSchema.parse({
        principalId: PrincipalIdSchema.parse(options.principalId),
        actorId: null,
        clientId: ClientIdSchema.parse(options.clientId),
        authorizedProjectIds: projectIds,
      }),
      allowedActions: options.allowedActions ?? WORKSPACE_SERVICE_ACTIONS,
    });
  };
  return {
    read: (request) => current().read(request),
    command: (request) => current().command(request),
    events: (request) => current().events(request),
    subscribe: (request) => current().subscribe(request),
  };
}

function emptyWorkspacePort(): WorkspaceClientPort {
  return {
    read: (request) =>
      request.operation === "project.list.v1"
        ? { ok: true, value: { operation: "project.list.v1", projects: [] } }
        : missingProject(),
    command: () => Promise.resolve(missingProject()),
    events: (request) => ({
      ok: true,
      value: {
        events: [],
        resume: request.cursor,
        next: null,
        coverage: { state: "complete" },
      },
    }),
    subscribe: () =>
      (async function* () {
        await Promise.resolve();
        yield missingProject();
      })(),
  };
}

function missingProject(): WorkspaceResult<never> {
  return {
    ok: false,
    error: {
      code: "not_found",
      message: "No project is registered yet; add an existing project directory in setup.",
    },
  };
}
