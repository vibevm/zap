/** Managed backend selection and binding ports. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import type { ModelSelection } from "../model-policy/index.ts";
import type { ExecutionSelection } from "../execution-catalog/index.ts";
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";
import type { ManagedWorkClaim, ManagedWorkRequest, ManagedWorkResult } from "./contracts.ts";
import type { ManagedAgentProfile } from "./providers.ts";

export interface ManagedActorBindingPort {
  prepare(input: {
    readonly request: ManagedWorkRequest;
    readonly taskId: string;
    readonly runId: string;
    readonly attemptId: string;
    readonly requesterActorId: string | null;
    readonly mcpConfigPath: string;
    readonly provider: ManagedAgentProfile["provider"];
    readonly mcpCommandPath: string | undefined;
    readonly mcpArgs: readonly string[] | undefined;
  }): Promise<
    ManagedWorkResult<{
      readonly actorId: string;
      readonly adapterSessionId: string;
      readonly mcpConfigPath: string;
      readonly environment: Readonly<Record<string, string>>;
    }>
  >;
  activate(input: {
    readonly runId: string;
    readonly actorId: string;
    readonly adapterSessionId: string;
    readonly mcpConfigPath: string;
    readonly provider: ManagedAgentProfile["provider"];
    readonly mcpCommandPath: string | undefined;
    readonly mcpArgs: readonly string[] | undefined;
  }): Promise<
    ManagedWorkResult<{
      readonly mcpConfigPath: string;
      readonly environment: Readonly<Record<string, string>>;
    }>
  >;
}

export interface ManagedSelectionPort {
  resolve(
    access: WorkspaceAccessContext,
    request: ManagedWorkRequest,
    profiles: readonly ManagedAgentProfile[],
    identity: { readonly runId: string; readonly attemptId: string },
  ): Promise<
    ManagedWorkResult<{
      readonly profile: ManagedAgentProfile;
      readonly modelSelection: ModelSelection;
      readonly executionSelection: ExecutionSelection | null;
    }>
  >;
  revalidate?(
    access: WorkspaceAccessContext,
    claim: ManagedWorkClaim,
  ): Promise<ManagedWorkResult<null>>;
}

export interface ManagedParentPort {
  validate(
    access: WorkspaceAccessContext,
    request: ManagedWorkRequest,
  ): ManagedWorkResult<{
    readonly parentTaskId: string | null;
    readonly parentRunId: string | null;
    readonly parentActorId: string | null;
    readonly depth: number;
  }>;
}
