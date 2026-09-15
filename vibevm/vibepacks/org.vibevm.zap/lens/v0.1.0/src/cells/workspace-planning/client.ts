/** Thin MCP client for Wayfinder-owned planning. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { z } from "zod";
import type { AgentPlanProposalPort } from "../mcp/index.ts";
import {
  BrokerErrorSchema,
  JsonValueSchema,
  type Credential,
  type Result,
} from "../protocol/index.ts";
import type { AdapterSessionId } from "../transport/index.ts";

export interface AgentPlanningHttpClientOptions {
  readonly baseUrl: URL;
  readonly principalToken: Credential;
  readonly timeoutMilliseconds?: number;
}

const EnvelopeSchema = z
  .object({
    protocol: z.literal("lens/1"),
    ok: z.boolean(),
    value: z.unknown().optional(),
    error: z.unknown().optional(),
  })
  .strict();

/** Wayfinder re-proves the binding; caller-supplied actor DTOs confer no authority. */
export function createAgentPlanningHttpClient(
  options: AgentPlanningHttpClientOptions,
): AgentPlanProposalPort {
  const call = (
    operation: string,
    session: AdapterSessionId,
    body: unknown,
  ): Promise<Result<z.infer<typeof JsonValueSchema>>> =>
    planningCall(options, operation, session, body);
  return {
    register: (_actor, session, input) => call("register", session, input),
    submit: (_actor, session, input) => call("submit", session, input),
    preview: (_actor, session, input) => call("preview", session, input),
    apply: (_actor, session, input) => call("apply", session, input),
    reconcile: (_actor, session, input) => call("reconcile", session, input),
    discover: (_actor, session) => call("discover", session, {}),
    author: (_actor, session, input) => call("author", session, input),
    prepareComposite: (_actor, session, input) => call("prepare-composite", session, input),
    authorComposite: (_actor, session, input) => call("author-composite", session, input),
    prepare: (_actor, session, kind, input) => call("prepare", session, { kind, input }),
  };
}

async function planningCall(
  options: AgentPlanningHttpClientOptions,
  operation: string,
  session: AdapterSessionId,
  body: unknown,
): Promise<Result<z.infer<typeof JsonValueSchema>>> {
  try {
    const response = await fetch(new URL(`/v1/agent-plan/${operation}`, options.baseUrl), {
      method: "POST",
      headers: {
        Authorization: `Bearer ${options.principalToken}`,
        "Content-Type": "application/json",
        "X-Codlens-Adapter-Session": session,
      },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(options.timeoutMilliseconds ?? 15_000),
    });
    const envelope = EnvelopeSchema.safeParse(JSON.parse(await response.text()));
    if (!envelope.success)
      return failure("invalid_input", "Wayfinder returned a malformed planning envelope");
    if (!envelope.data.ok) {
      const error = BrokerErrorSchema.safeParse(envelope.data.error);
      return error.success
        ? { ok: false, error: error.data }
        : failure("invalid_input", "Wayfinder returned a malformed planning error");
    }
    const value = JsonValueSchema.safeParse(envelope.data.value);
    return value.success
      ? { ok: true, value: value.data }
      : failure("invalid_input", "Wayfinder returned a non-public planning value");
  } catch {
    return failure(
      "storage_failure",
      "Wayfinder planning response was not observed; reconcile the same durable operation",
    );
  }
}

function failure(code: "invalid_input" | "storage_failure", message: string): Result<never> {
  return {
    ok: false,
    error: {
      code,
      message: `violates REQ spec://org.vibevm.zap/lens/PROP-005#server-ownership: ${message}`,
    },
  };
}
