/** Password-protected Wayfinder web composition. @scope spec://org.vibevm.zap/lens/PROP-003#web-sessions */
import { isAbsolute } from "node:path";
import { z } from "zod";
import { loadPasswordAuthenticator } from "../web-auth/index.ts";
import { createQuicklensWebGateway, type QuicklensWebGateway } from "../web-gateway/index.ts";
import { unavailableDataSource } from "../quicklens-model/index.ts";
import { PrincipalIdSchema } from "../protocol/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import {
  WORKSPACE_SERVICE_ACTIONS,
  type WorkspaceService,
  type WorkspaceServiceAction,
  type WorkspaceServiceAuthorization,
} from "../workspace-service/index.ts";

const AbsolutePathSchema = z.string().min(1).refine(isAbsolute, "path must be absolute");
const WorkspaceActionSchema = z.enum(WORKSPACE_SERVICE_ACTIONS);
export const WayfinderWebConfigSchema = z
  .object({
    enabled: z.boolean(),
    host: z.string().min(1).max(255),
    port: z.number().int().min(0).max(65_535),
    rendererRoot: AbsolutePathSchema,
    passwordVerifierPath: AbsolutePathSchema,
    publicOrigin: z
      .url()
      .refine((value) => new URL(value).protocol === "https:", "public origin must use HTTPS"),
    proxyProofToken: z.string().min(24).max(512),
    trustedProjectIds: z.array(ProjectIdSchema).min(1).max(256),
    role: z.enum(["viewer", "operator", "owner"]),
    webOperations: z
      .array(
        z.enum([
          "read",
          "answer",
          "intent",
          "preview",
          "apply",
          "reconcile",
          "decide",
          "invalidations",
        ]),
      )
      .min(1)
      .max(8),
    workspaceActions: z.array(WorkspaceActionSchema).max(32),
  })
  .strict();
export type WayfinderWebConfig = z.infer<typeof WayfinderWebConfigSchema>;

export type WayfinderWebResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: { readonly code: "invalid_config" | "unavailable"; readonly message: string };
    };

export interface WayfinderWebRuntime {
  readonly gateway: QuicklensWebGateway;
  start(): Promise<WayfinderWebResult<{ readonly host: string; readonly port: number }>>;
  close(): Promise<void>;
}

export async function openWayfinderWebRuntime(
  rawConfig: unknown,
  service: WorkspaceService,
): Promise<WayfinderWebResult<WayfinderWebRuntime | undefined>> {
  const parsed = WayfinderWebConfigSchema.safeParse(rawConfig);
  if (!parsed.success) return failure("invalid_config", "Wayfinder web configuration is invalid");
  if (!parsed.data.enabled) return { ok: true, value: undefined };
  const auth = await loadPasswordAuthenticator(parsed.data.passwordVerifierPath);
  if (!auth.ok) return failure("unavailable", "Wayfinder web password verifier is unavailable");
  const opened = createQuicklensWebGateway({
    source: unavailableDataSource("Wayfinder shared workspace is the configured web source."),
    authenticator: auth.value,
    rendererRoot: parsed.data.rendererRoot,
    publicOrigin: parsed.data.publicOrigin,
    proxyProofToken: parsed.data.proxyProofToken,
    role: parsed.data.role,
    allowedOperations: parsed.data.webOperations,
    workspaceClientFactory: ({ sessionId }) => {
      const access = WorkspaceAccessContextSchema.parse({
        principalId: PrincipalIdSchema.parse(`principal.web.${sessionId.slice(0, 32)}`),
        actorId: null,
        clientId: ClientIdSchema.parse(`client.web.${sessionId.slice(0, 32)}`),
        authorizedProjectIds: parsed.data.trustedProjectIds,
      });
      const authorization: WorkspaceServiceAuthorization = {
        access,
        allowedActions: actionsForRole(parsed.data.role, parsed.data.workspaceActions),
      };
      return service.bind(authorization);
    },
  });
  if (!opened.ok)
    return failure("invalid_config", "Wayfinder web gateway configuration is invalid");
  const gateway = opened.value;
  return {
    ok: true,
    value: {
      gateway,
      start: async () => {
        const started = await gateway.start({ host: parsed.data.host, port: parsed.data.port });
        return started.ok
          ? { ok: true, value: { host: started.value.address, port: started.value.port } }
          : failure("unavailable", "Wayfinder web gateway could not bind");
      },
      close: () => gateway.close(),
    },
  };
}

function actionsForRole(
  role: WayfinderWebConfig["role"],
  configured: readonly WorkspaceServiceAction[],
): readonly WorkspaceServiceAction[] {
  if (role === "viewer") return ["read", "events", "subscribe"];
  const allowed = new Set(configured);
  if (role === "operator") {
    allowed.delete("project.stop.v1");
    allowed.delete("project.continue.v1");
  }
  return [...allowed];
}

function failure(
  code: "invalid_config" | "unavailable",
  message: string,
): WayfinderWebResult<never> {
  return { ok: false, error: { code, message } };
}
