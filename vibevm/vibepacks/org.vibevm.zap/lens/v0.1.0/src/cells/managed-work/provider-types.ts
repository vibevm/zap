/** Provider capability evidence. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { z } from "zod";

export const ManagedProviderIdSchema = z.enum(["codex", "claude_code", "opencode", "qwen_code"]);
export type ManagedProviderId = z.infer<typeof ManagedProviderIdSchema>;
export const ProviderCapabilitySchema = z.enum(["supported", "conditional", "unsupported"]);
export const ManagedProviderCapabilitiesSchema = z
  .object({
    provider: ManagedProviderIdSchema,
    observedVersion: z.string().min(1).max(160).nullable(),
    installed: z.boolean(),
    launchable: z.boolean(),
    authenticated: z.enum(["observed", "not_observed", "unavailable"]),
    structuredConversation: ProviderCapabilitySchema,
    nativeChildren: ProviderCapabilitySchema,
    nativeQuestions: ProviderCapabilitySchema,
    nativeApprovals: ProviderCapabilitySchema,
    interrupt: ProviderCapabilitySchema,
    resume: ProviderCapabilitySchema,
    interactiveTerminal: ProviderCapabilitySchema,
    evidence: z.array(z.string().min(1).max(512)).max(32),
  })
  .strict();
export type ManagedProviderCapabilities = z.infer<typeof ManagedProviderCapabilitiesSchema>;
