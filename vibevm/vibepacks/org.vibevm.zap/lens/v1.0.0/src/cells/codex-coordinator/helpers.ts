/** Codex adapter validation and normalization helpers. @scope spec://org.vibevm.zap/lens/PROP-005#history */
import { resolve } from "node:path";
import type { z } from "zod";
import {
  jsonValue,
  runtimeFailure,
  type AgentRuntimeResult,
  type CoordinatorResumeInput,
  type CoordinatorSessionDescriptor,
  type CoordinatorStartInput,
  type CoordinatorTurnReceipt,
  type CoordinatorTurnInput,
  type PendingHostRequest,
} from "../agent-runtime/index.ts";
import { zapPreauthorizedToolNames, type JsonValueSchema } from "../protocol/index.ts";
import { NativeRefSchema } from "../workspace-model/index.ts";
import {
  CodexCommandApprovalResponseSchema,
  CodexFileApprovalResponseSchema,
  CodexPermissionsResponseSchema,
  CodexThreadResponseSchema,
  CodexUserInputResponseSchema,
  type CodexRpcId,
  type CodexServerRequest,
  type CodexThread,
} from "./protocol.ts";
import type { CodexProcessResult } from "./process.ts";
import { CODEX_COORDINATOR_CAPABILITIES } from "./profile.ts";
import type { SessionState, WorkerState } from "./state.ts";
import type { CodexCoordinatorProfile } from "./profile.ts";
import type { CodexProcessProfile } from "./process.ts";
import type { ReasoningEffort } from "../model-policy/index.ts";

export function modelParameters(
  modelId: string | undefined,
  reasoningEffort: ReasoningEffort | null | undefined,
  profile: CodexCoordinatorProfile,
  agentScope?: CoordinatorStartInput["agentScope"],
  agentBinding?: CoordinatorStartInput["agentBinding"],
) {
  const effort = reasoningEffort === undefined ? profile.effort : reasoningEffort;
  const credentialFile = lensCredentialFile(profile, agentScope);
  const config = {
    ...(effort === undefined || effort === null ? {} : { model_reasoning_effort: effort }),
    ...(profile.contextWindowTokens === undefined
      ? {}
      : { model_context_window: profile.contextWindowTokens }),
    ...(profile.lensMcp === undefined ||
    agentScope === undefined ||
    agentScope === null ||
    credentialFile === undefined
      ? {}
      : {
          mcp_servers: {
            [profile.lensMcp.serverName]: {
              command: profile.lensMcp.commandPath,
              args: profile.lensMcp.args,
              ...(agentBinding === undefined || agentBinding === null
                ? {}
                : { disabled_tools: ["codlens_connect"] }),
              ...(profile.lensMcp.communicationPreauthorization === undefined
                ? {}
                : {
                    default_tools_approval_mode: "prompt",
                    tools: communicationToolPolicy(
                      profile.lensMcp.communicationPreauthorization.allowDelegation,
                    ),
                  }),
              env: {
                CODLENS_URL: profile.lensMcp.brokerUrl,
                CODLENS_CREDENTIAL_FILE: credentialFile,
                CODLENS_WORKSPACE_ID: agentScope.workspaceId,
                CODLENS_CONVERSATION_ID: agentScope.conversationId,
                ...(agentBinding === undefined || agentBinding === null
                  ? {}
                  : { CODLENS_ADAPTER_SESSION_ID: agentBinding.adapterSessionId }),
                ...(profile.lensMcp.planConfigFile === undefined
                  ? {}
                  : { CODLENS_PLAN_CONFIG_FILE: profile.lensMcp.planConfigFile }),
              },
            },
          },
        }),
  };
  return {
    model: modelId ?? profile.model,
    ...(Object.keys(config).length === 0 ? {} : { config }),
  };
}

export function lensCredentialFile(
  profile: CodexCoordinatorProfile,
  scope: CoordinatorStartInput["agentScope"],
): string | undefined {
  if (profile.lensMcp === undefined || scope === undefined || scope === null) return undefined;
  const scoped = profile.lensMcp.scopeCredentials?.find(
    (entry) =>
      entry.workspaceId === scope.workspaceId && entry.conversationId === scope.conversationId,
  );
  return (
    scoped?.credentialFile ??
    (profile.lensMcp.scopeCredentials === undefined ? profile.lensMcp.credentialFile : undefined)
  );
}

function communicationToolPolicy(allowDelegation: boolean) {
  const approved = { approval_mode: "approve" as const };
  return Object.fromEntries(
    zapPreauthorizedToolNames(allowDelegation).map((tool) => [tool, approved]),
  );
}

export function validateExplicitThreadModel(
  thread: CodexThread,
  requestedModel: string,
): AgentRuntimeResult<void> {
  const observed = Reflect.get(thread, "model");
  if (observed === undefined || observed === null) return { ok: true, value: undefined };
  if (typeof observed !== "string") {
    return runtimeFailure("protocol_error", "Codex thread reported a malformed model observation");
  }
  return observed === requestedModel
    ? { ok: true, value: undefined }
    : runtimeFailure("host_refused", "Codex thread reported a different model than requested");
}

export function descriptor(
  input: CoordinatorStartInput | CoordinatorResumeInput,
  worker: WorkerState,
  response: { thread: CodexThread; instructionSources: readonly string[] | undefined },
  bootstrap: "submitted" | "not_observed",
): CoordinatorSessionDescriptor {
  const thread = response.thread;
  return {
    ...scopeOf(input),
    profileId: input.profileId,
    productId: "codex",
    role: "coordinator",
    launchOrigin: "lens",
    interactionKind: "structured",
    state: stateOf(thread.status),
    nativeThreadRef: NativeRefSchema.parse({
      namespace: "codex.thread",
      value: thread.id,
      incarnation: worker.incarnation,
    }),
    nativeSessionId: thread.sessionId,
    cwd: thread.cwd,
    processEpoch: worker.epoch,
    bootstrap,
    instructionSources: [...(response.instructionSources ?? [])],
    capabilities: CODEX_COORDINATOR_CAPABILITIES,
  };
}

export function scopeOf(input: CoordinatorStartInput | CoordinatorResumeInput) {
  return {
    coordinatorSessionId: input.coordinatorSessionId,
    projectId: input.projectId,
    contextId: input.contextId,
    conversationId: input.conversationId,
    coordinatorActorId: input.coordinatorActorId,
    hostId: input.hostId,
  };
}

export function parseThread(
  result: CodexProcessResult<unknown>,
  expectedCwd: string,
  expectedThreadId?: string,
): AgentRuntimeResult<z.infer<typeof CodexThreadResponseSchema>> {
  if (!result.ok) return processFailure(result.error);
  const parsed = CodexThreadResponseSchema.safeParse(result.value);
  if (!parsed.success) return runtimeFailure("protocol_error", "Codex thread response is invalid");
  if (
    !samePath(parsed.data.thread.cwd, expectedCwd) ||
    (expectedThreadId !== undefined && parsed.data.thread.id !== expectedThreadId)
  ) {
    return runtimeFailure("host_refused", "Codex resumed a different thread or working directory");
  }
  return { ok: true, value: parsed.data };
}

function samePath(left: string, right: string): boolean {
  const a = resolve(left);
  const b = resolve(right);
  return process.platform === "win32" ? a.toLowerCase() === b.toLowerCase() : a === b;
}

export function activeTurn(thread: CodexThread): string | null {
  return thread.turns.findLast((turn) => turn.status === "inProgress")?.id ?? null;
}

export function stateOf(status: CodexThread["status"]): CoordinatorSessionDescriptor["state"] {
  if (status.type === "active") {
    return status.activeFlags.includes("waitingOnApproval") ? "waiting_for_user" : "running";
  }
  return status.type === "systemError" ? "failed" : "ready";
}

export function receipt(
  session: SessionState,
  nativeTurnId: string,
  processEpoch: string,
): CoordinatorTurnReceipt {
  return {
    coordinatorSessionId: session.descriptor.coordinatorSessionId,
    nativeThreadId: session.descriptor.nativeThreadRef.value,
    nativeTurnId,
    observation: "host_accepted",
    processEpoch,
  };
}

export function turnStartParams(session: SessionState, input: CoordinatorTurnInput) {
  return {
    threadId: session.descriptor.nativeThreadRef.value,
    input: [{ type: "text", text: input.text }],
    clientUserMessageId: input.clientMessageId,
    ...(session.reasoningEffort === null ? {} : { effort: session.reasoningEffort }),
  };
}

export function processProfile(profile: CodexCoordinatorProfile): CodexProcessProfile {
  return {
    executablePath: profile.executablePath,
    requestTimeoutMs: profile.requestTimeoutMs,
    ...(profile.accountBindingId === undefined
      ? {}
      : { accountBindingId: profile.accountBindingId }),
    ...(profile.proxy === undefined ? {} : { proxy: profile.proxy }),
  };
}

export function requestKind(method: CodexServerRequest["method"]): PendingHostRequest["kind"] {
  if (method === "item/tool/requestUserInput") return "user_input";
  if (method === "item/commandExecution/requestApproval") return "command_approval";
  if (method === "item/fileChange/requestApproval") return "file_approval";
  return "permission_approval";
}

export function requestKey(id: CodexRpcId): string {
  return `${typeof id}:${String(id)}`;
}

export function validateHostAnswer(
  kind: PendingHostRequest["kind"],
  answer: unknown,
): AgentRuntimeResult<z.infer<typeof JsonValueSchema>> {
  const schema =
    kind === "user_input"
      ? CodexUserInputResponseSchema
      : kind === "command_approval"
        ? CodexCommandApprovalResponseSchema
        : kind === "file_approval"
          ? CodexFileApprovalResponseSchema
          : CodexPermissionsResponseSchema;
  const parsed = schema.safeParse(answer);
  return parsed.success
    ? jsonValue(parsed.data)
    : runtimeFailure("invalid_input", "Answer does not match the pending Codex request kind");
}

export function publicThread(thread: CodexThread) {
  return {
    id: thread.id,
    sessionId: thread.sessionId,
    cwd: thread.cwd,
    status: thread.status,
    parentThreadId: thread.parentThreadId ?? null,
    canAcceptDirectInput: thread.canAcceptDirectInput ?? null,
    cliVersion: thread.cliVersion,
  };
}

export function coordinatorBootstrap(
  projectBootstrap: string,
  input?: Pick<CoordinatorStartInput, "coordinatorSessionId" | "agentScope" | "agentBinding">,
  lensMcpAvailable = false,
): string {
  const connection =
    !lensMcpAvailable || input?.agentScope === undefined || input.agentScope === null
      ? ""
      : input.agentBinding !== undefined && input.agentBinding !== null
        ? [
            `Your broker actor is preprovisioned. Use adapterSessionId ${input.agentBinding.adapterSessionId} for codlens tools; do not call codlens_connect again.`,
            `This binding is scoped to workspaceId ${input.agentScope.workspaceId} and conversationId ${input.agentScope.conversationId}.`,
            "A native child must receive its own delegated adapterSessionId; never reuse the coordinator handle as the child's identity.",
          ].join("\n") + "\n\n"
        : [
            "Before using /ZapAskUserQuestion, call codlens_connect once and retain its returned adapterSessionId.",
            `Use this exact connection body: ${JSON.stringify({ clientRequestId: `request.connect.${input.coordinatorSessionId}`, workspaceId: input.agentScope.workspaceId, conversationId: input.agentScope.conversationId, capabilities: ["message:emit", "question:ask", "inbox:read", "inbox:ack", "actor:delegate", "actor:expire", "inbox:forward", "plan:propose"], host: { kind: "codex", provenance: "explicit_handle" }, replyPolicy: { kind: "retain" } })}`,
            "A native child must receive its own delegated adapterSessionId; never reuse the coordinator handle as the child's identity.",
          ].join("\n") + "\n\n";
  return [
    "You are the long-lived project coordinator for this exact Lens work context.",
    "Follow the complete applicable project instructions and boot material supplied below.",
    "This coordinator launch is not a delegated worker and does not use a worker quiet clause.",
    ...(lensMcpAvailable
      ? [
          "Send user clarification through /ZapAskUserQuestion and post a short ordinary-text notice after publishing. Do not poll for the answer or treat a normal answer as an approval.",
          "For managed delegation, inspect the available managed work profiles, declare the task specialization explicitly, and use catalog_policy unless the human intentionally requested a named catalog override. Never substitute another account, premium model, effort, or context when the catalog reports no eligible configuration.",
        ]
      : []),
    connection,
    projectBootstrap,
  ]
    .filter((part) => part.length > 0)
    .join("\n\n");
}

export function processFailure(error: {
  kind: string;
  message: string;
}): AgentRuntimeResult<never> {
  if (error.kind === "rpc") return runtimeFailure("host_refused", error.message);
  if (error.kind === "protocol") return runtimeFailure("protocol_error", error.message);
  return runtimeFailure("transport_lost", error.message, "after_reconcile");
}
