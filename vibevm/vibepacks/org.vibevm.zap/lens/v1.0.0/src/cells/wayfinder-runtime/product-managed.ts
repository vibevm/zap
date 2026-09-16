/** Per-project managed profile registration. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import type { CodexCoordinatorProfile } from "../codex-coordinator/index.ts";
import { ManagedAgentProfileSchema, type ManagedAgentBackend } from "../managed-work/index.ts";
import type { ManagedWorkerTemplate } from "../product-app/index.ts";
import type { ProductProviderProfile } from "../workspace-model/index.ts";
import type { ProviderCoordinatorProfile } from "../provider-coordinators/index.ts";
import type { ProxyPolicy } from "../proxy-policy/index.ts";
import type { TrustedProjectRegistration } from "../workspace-store/index.ts";

export function registerProductManagedProfiles(input: {
  readonly backend: ManagedAgentBackend;
  readonly policyKey: string;
  readonly registration: TrustedProjectRegistration;
  readonly codexProfiles: readonly CodexCoordinatorProfile[];
  readonly providerProfiles: readonly ProviderCoordinatorProfile[];
  readonly products: readonly ProductProviderProfile[];
  readonly templates: readonly ManagedWorkerTemplate[];
  readonly mcpRoot: string;
  readonly globalProxy: ProxyPolicy;
}):
  | {
      readonly ok: true;
      readonly value: readonly {
        readonly profileId: string;
        readonly tier: ManagedWorkerTemplate["tier"];
        readonly modelId: string;
        readonly provider: ProductProviderProfile["provider"];
        readonly effort: ManagedWorkerTemplate["effort"];
      }[];
    }
  | { readonly ok: false; readonly message: string } {
  const projected = [];
  for (const template of input.templates) {
    const product = input.products.find(
      (candidate) => candidate.profileId === template.sourceProfileId,
    );
    const codex = input.codexProfiles.find(
      (candidate) => candidate.profileId === template.sourceProfileId,
    );
    const provider = input.providerProfiles.find(
      (candidate) => candidate.profileId === template.sourceProfileId,
    );
    if (
      product === undefined ||
      !product.installed ||
      !product.configured ||
      !product.launchable ||
      (codex === undefined && provider === undefined)
    )
      return { ok: false, message: "managed worker template source is unavailable" };
    const effortSupported = product.provider === "codex" || product.provider === "claude_code";
    if (!effortSupported && template.effort !== null)
      return { ok: false, message: "managed worker effort is unsupported" };
    const templateKey = createHash("sha256")
      .update(`${input.policyKey}\u0000${template.profileId}`)
      .digest("hex")
      .slice(0, 24);
    const registered = input.backend.registerProfile(
      ManagedAgentProfileSchema.parse({
        profileId: `profile.managed.${templateKey}`,
        tier: template.tier,
        projectId: input.registration.projectId,
        contextId: input.registration.context.contextId,
        provider: product.provider,
        executablePath: codex?.executablePath ?? provider?.executablePath,
        argumentPrefix: provider?.argumentPrefix ?? [],
        cwd: input.registration.protected.cwd,
        modelId: template.modelId,
        effort: template.effort,
        effortSupported,
        environmentRef: provider?.environmentRef ?? null,
        mcpConfigPath: join(input.mcpRoot, `${templateKey}.json`),
        mcpCommandPath: process.execPath,
        mcpArgs: [mcpEntrypoint()],
        proxy: effectiveManagedProxy(codex, provider, input.globalProxy),
        capabilities: {
          provider: product.provider,
          observedVersion: null,
          installed: product.installed,
          launchable: product.launchable,
          authenticated: product.authenticated,
          structuredConversation: "unsupported",
          nativeChildren: "unsupported",
          nativeQuestions: "supported",
          nativeApprovals: "unsupported",
          interrupt: "supported",
          resume: "unsupported",
          interactiveTerminal: "supported",
          evidence: [...product.evidence, "Zap managed PTY and MCP scope prepared"],
        },
      }),
    );
    if (!registered.ok) return { ok: false, message: registered.error.message };
    projected.push({
      profileId: registered.value.profileId,
      tier: registered.value.tier,
      modelId: registered.value.modelId,
      provider: product.provider,
      effort: template.effort,
    });
  }
  return { ok: true, value: projected };
}

export function effectiveManagedProxy(
  codex: Pick<CodexCoordinatorProfile, "proxy"> | undefined,
  provider: Pick<ProviderCoordinatorProfile, "proxy"> | undefined,
  globalProxy: ProxyPolicy,
): ProxyPolicy {
  return codex?.proxy ?? provider?.proxy ?? globalProxy;
}

function mcpEntrypoint(): string {
  const installed = resolve(import.meta.dirname, "../../mcp.js");
  return existsSync(installed) ? installed : resolve(import.meta.dirname, "../../mcp.ts");
}
