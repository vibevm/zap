/** @scope spec://org.vibevm.zap/lens/PROP-001#transport */
import { z, type ZodType } from "zod";
import {
  AckResultSchema,
  ActorDescriptorSchema,
  ForwardResultSchema,
  InboxPageSchema,
  MessageEnvelopeSchema,
  PublicConnectionSchema,
  QuestionSchema,
  type Credential,
  type Result,
} from "../protocol/index.ts";
import {
  AdapterSessionIdSchema,
  failure,
  type AdapterSessionId,
  type AgentTransportPort,
} from "../transport/index.ts";

export interface AgentHttpClientOptions {
  readonly baseUrl: URL;
  readonly principalToken: Credential;
}

const PublicAdapterConnectionSchema = z
  .object({
    adapterSessionId: AdapterSessionIdSchema,
    connection: PublicConnectionSchema,
  })
  .strict();

/** Production MCP-facing client for the already-running background broker. */
export function createAgentHttpClient(options: AgentHttpClientOptions): AgentTransportPort {
  const sessionCall = <O>(
    path: string,
    session: AdapterSessionId,
    input: unknown,
    schema: ZodType<O>,
  ): Promise<Result<O>> =>
    httpCommand(options.baseUrl, path, options.principalToken, input, schema, session);
  return {
    connect: (input) =>
      httpCommand(
        options.baseUrl,
        "/v1/connect",
        options.principalToken,
        input,
        PublicAdapterConnectionSchema,
      ),
    delegate: (session, input) =>
      sessionCall("/v1/delegate", session, input, PublicAdapterConnectionSchema),
    emit: (session, input) => sessionCall("/v1/emit", session, input, MessageEnvelopeSchema),
    ask: (session, input) => sessionCall("/v1/ask", session, input, QuestionSchema),
    inbox: (session, input) => sessionCall("/v1/inbox", session, input, InboxPageSchema),
    ack: (session, input) => sessionCall("/v1/ack", session, input, AckResultSchema),
    finish: (session, clientRequestId) =>
      sessionCall("/v1/finish", session, { clientRequestId }, ActorDescriptorSchema),
    forward: (session, input) => sessionCall("/v1/forward", session, input, ForwardResultSchema),
  };
}

async function httpCommand<O>(
  baseUrl: URL,
  path: string,
  bearer: string,
  input: unknown,
  schema: ZodType<O>,
  session?: AdapterSessionId,
): Promise<Result<O>> {
  try {
    const response = await fetch(new URL(path, baseUrl), {
      method: "POST",
      headers: {
        Authorization: `Bearer ${bearer}`,
        "Content-Type": "application/json",
        ...(session === undefined ? {} : { "X-Codlens-Adapter-Session": session }),
      },
      body: JSON.stringify(input),
    });
    const raw: unknown = JSON.parse(await response.text());
    const envelope = z
      .object({
        protocol: z.literal("lens/1"),
        ok: z.boolean(),
        value: z.unknown().optional(),
        error: z.unknown().optional(),
      })
      .strict()
      .safeParse(raw);
    if (!envelope.success) {
      return failure("invalid_input", "HTTP broker returned a malformed lens/1 envelope");
    }
    if (!envelope.data.ok) {
      const error = BrokerErrorSchema.safeParse(envelope.data.error);
      return error.success
        ? { ok: false, error: error.data }
        : failure("invalid_input", "HTTP broker returned a malformed typed error");
    }
    const parsed = schema.safeParse(envelope.data.value);
    return parsed.success
      ? { ok: true, value: parsed.data }
      : failure("invalid_input", "HTTP broker returned a malformed command result");
  } catch {
    return failure(
      "storage_failure",
      "HTTP broker request failed before a typed result arrived",
      "reconnect to the configured background broker and reconcile by client request ID",
    );
  }
}

const BrokerErrorSchema = z
  .object({
    code: z.enum([
      "invalid_input",
      "unauthorized",
      "forbidden",
      "not_found",
      "conflict",
      "stale_revision",
      "already_answered",
      "stale_binding",
      "idempotency_conflict",
      "backpressure",
      "resync_required",
      "unsupported_operation",
      "storage_failure",
      "closed",
    ]),
    message: z.string().startsWith("violates REQ spec://"),
    details: z.record(z.string(), z.unknown()).optional(),
  })
  .strict();
