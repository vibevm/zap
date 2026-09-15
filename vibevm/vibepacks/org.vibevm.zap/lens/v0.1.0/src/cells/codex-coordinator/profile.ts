/** Codex profile and observed capabilities. @scope spec://org.vibevm.zap/lens/PROP-006#availability-policy */
import { z } from "zod";
import { isAbsolute } from "node:path";
import { ReasoningEffortSchema } from "../model-policy/index.ts";
import type {
  CoordinatorCapabilities,
  CoordinatorLifecycleCapabilities,
} from "../agent-runtime/index.ts";
import { CodexProcessProfileSchema } from "./process.ts";

export const CodexCoordinatorProfileSchema = CodexProcessProfileSchema.extend({
  profileId: z.string().min(1).max(160),
  model: z.string().min(1).max(256),
  effort: ReasoningEffortSchema.nullable().optional(),
  approvalPolicy: z.enum(["untrusted", "on-request", "never"]),
  sandbox: z.enum(["read-only", "workspace-write", "danger-full-access"]),
  personality: z.enum(["none", "friendly", "pragmatic"]).default("pragmatic"),
  serviceName: z.string().min(1).max(160).default("quicklens"),
  lensMcp: z
    .object({
      serverName: z.string().regex(/^[A-Za-z][A-Za-z0-9_-]{1,63}$/),
      commandPath: z.string().min(1).max(32_768).refine(isAbsolute, "MCP command must be absolute"),
      args: z.array(z.string().max(16_384)).max(64).default(["mcp", "serve"]),
      brokerUrl: z.url().refine((value) => {
        const url = new URL(value);
        return (
          url.protocol === "http:" && new Set(["127.0.0.1", "localhost", "[::1]"]).has(url.hostname)
        );
      }, "MCP broker must be loopback HTTP"),
      credentialFile: z
        .string()
        .min(1)
        .max(32_768)
        .refine(isAbsolute, "credential file must be absolute")
        .optional(),
      scopeCredentials: z
        .array(
          z
            .object({
              workspaceId: z.string().min(3).max(160),
              conversationId: z.string().min(3).max(160),
              credentialFile: z
                .string()
                .min(1)
                .max(32_768)
                .refine(isAbsolute, "credential file must be absolute"),
            })
            .strict(),
        )
        .min(1)
        .max(256)
        .optional(),
      planConfigFile: z
        .string()
        .min(1)
        .max(32_768)
        .refine(isAbsolute, "plan config must be absolute")
        .optional(),
      communicationPreauthorization: z
        .object({ allowDelegation: z.boolean().default(false) })
        .strict()
        .optional(),
    })
    .strict()
    .superRefine((mcp, context) => {
      if (mcp.credentialFile === undefined && mcp.scopeCredentials === undefined) {
        context.addIssue({ code: "custom", message: "Lens MCP needs a credential source" });
      }
      const keys = mcp.scopeCredentials?.map(
        (entry) => `${entry.workspaceId}\u0000${entry.conversationId}`,
      );
      if (keys !== undefined && new Set(keys).size !== keys.length) {
        context.addIssue({ code: "custom", message: "Lens MCP credential scopes must be unique" });
      }
    })
    .optional(),
}).strict();
export type CodexCoordinatorProfile = z.infer<typeof CodexCoordinatorProfileSchema>;

export const CODEX_COORDINATOR_CAPABILITIES: CoordinatorCapabilities = {
  persistentThreads: true,
  turnStart: true,
  activeTurnSteer: true,
  turnInterrupt: true,
  historyRead: true,
  nativeChildObservation: true,
  nativeChildDirectInput: false,
  structuredUserInput: true,
  commandApproval: true,
  managedTerminal: false,
};

export const CODEX_LIFECYCLE_CAPABILITIES: CoordinatorLifecycleCapabilities = {
  pause: "interrupt_known_turns",
  stop: "owned_process",
  continue: "saved_thread_resume",
  nativeChildren: "known_active_turns",
};
