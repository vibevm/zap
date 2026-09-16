/** Agent gateway configuration. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { isAbsolute } from "node:path";
import { z } from "zod";
import { ConversationIdSchema, CredentialSchema, WorkspaceIdSchema } from "../protocol/index.ts";

export const WayfinderAgentGatewayConfigSchema = z
  .object({
    databasePath: z.string().min(1).refine(isAbsolute, "broker database path must be absolute"),
    host: z.enum(["127.0.0.1", "localhost", "::1"]),
    port: z.number().int().min(0).max(65_535),
    allowedHosts: z.array(z.string().min(1)).min(1).max(16),
    allowedOrigins: z.array(z.string().min(1)).max(16),
    statusToken: CredentialSchema,
    scopes: z
      .array(
        z
          .object({
            workspaceId: WorkspaceIdSchema,
            conversationId: ConversationIdSchema,
            humanPrincipalToken: CredentialSchema,
            agentPrincipalToken: CredentialSchema,
          })
          .strict(),
      )
      .max(256),
  })
  .strict()
  .superRefine((config, context) => {
    const keys = config.scopes.map((scope) => scope.workspaceId + "\u0000" + scope.conversationId);
    if (new Set(keys).size !== keys.length)
      context.addIssue({
        code: "custom",
        path: ["scopes"],
        message: "agent gateway scopes must be unique",
      });
  });
export type WayfinderAgentGatewayConfig = z.infer<typeof WayfinderAgentGatewayConfigSchema>;
