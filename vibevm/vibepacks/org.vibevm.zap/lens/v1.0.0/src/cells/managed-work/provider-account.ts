/** Managed launch account binding resolution. @scope spec://org.vibevm.zap/lens/PROP-015#execution */
import type { ExecutionAccountIsolationPort } from "../execution-accounts/index.ts";
import type { ManagedAgentProfile, ProtectedEnvironmentPort } from "./providers.ts";

export async function resolveManagedProviderEnvironment(
  profile: ManagedAgentProfile,
  environment: ProtectedEnvironmentPort,
  accounts?: ExecutionAccountIsolationPort,
): Promise<
  | { readonly ok: true; readonly value: Readonly<Record<string, string>> }
  | { readonly ok: false; readonly message: string }
> {
  const base = await environment.resolve(profile.environmentRef);
  if (!base.ok) return base;
  const bindingId = profile.accountBindingId;
  if (bindingId === undefined) return base;
  if (accounts === undefined || profile.executionHostId === undefined)
    return { ok: false, message: "managed account binding has no trusted host resolver" };
  const account = await accounts.resolve({
    bindingId,
    hostId: profile.executionHostId,
    agentProduct: profile.provider,
  });
  if (!account.ok) return { ok: false, message: account.error.message };
  if (
    account.value.environmentRef !== null &&
    account.value.environmentRef !== profile.environmentRef
  )
    return { ok: false, message: "managed profile environment does not match its account binding" };
  return { ok: true, value: { ...base.value, ...account.value.environment } };
}
