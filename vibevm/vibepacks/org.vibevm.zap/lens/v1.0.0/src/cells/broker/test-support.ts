/** @scope spec://org.vibevm.zap/lens/PROP-001#verification */
import assert from "node:assert/strict";

import {
  ClientRequestIdSchema,
  ConversationIdSchema,
  WorkspaceIdSchema,
  type BindingAuth,
  type Capability,
  type Connection,
  type PrincipalEnrollment,
  type Result,
} from "../protocol/index.ts";
import { openBroker, type LensBroker } from "./index.ts";

export const WORKSPACE = WorkspaceIdSchema.parse("workspace_main");
export const CONVERSATION = ConversationIdSchema.parse("conversation_main");
export const AGENT_CAPABILITIES: readonly Capability[] = [
  "message:emit",
  "question:ask",
  "question:cancel",
  "inbox:read",
  "inbox:ack",
  "inbox:forward",
  "actor:delegate",
  "actor:expire",
  "plan:propose",
];

export function take<T>(result: Result<T>): T {
  if (!result.ok) assert.fail(result.error.message);
  return result.value;
}

export function expectError<T>(result: Result<T>, code: string): void {
  if (result.ok) assert.fail(`expected ${code}, received success`);
  assert.equal(result.error.code, code);
}

export function request(id: string) {
  return ClientRequestIdSchema.parse(`request_${id}`);
}

export function enroll(
  broker: LensBroker,
  kind: "agent" | "human_responder" | "viewer",
  capabilities: readonly Capability[],
  workspace = WORKSPACE,
  conversation = CONVERSATION,
): PrincipalEnrollment {
  return take(
    broker.enrollPrincipal({
      kind,
      workspaceIds: [workspace],
      conversationIds: [conversation],
      capabilities: [...capabilities],
    }),
  );
}

export function connect(
  broker: LensBroker,
  principal: PrincipalEnrollment,
  id: string,
): Connection {
  return take(
    broker.connect({
      principalToken: principal.principalToken,
      clientRequestId: request(id),
      workspaceId: WORKSPACE,
      conversationId: CONVERSATION,
      capabilities: [...AGENT_CAPABILITIES],
      host: { kind: "test", provenance: "attested" },
      replyPolicy: { kind: "retain" },
    }),
  );
}

export function auth(principal: PrincipalEnrollment, connection: Connection): BindingAuth {
  return {
    principalToken: principal.principalToken,
    bindingToken: connection.credentials.bindingToken,
  };
}

export function memoryBroker(): LensBroker {
  return take(openBroker({ databasePath: ":memory:" }));
}
