import assert from "node:assert/strict";
import test from "node:test";
import { ConnectionSchema, CredentialSchema, HostBindingSchema } from "../protocol/index.ts";
import { AdapterSessions } from "./index.ts";

test("adapter sessions reuse an actor handle and never expose broker credentials", () => {
  let ids = 0;
  const sessions = new AdapterSessions(() => `adapter.transport.${String(++ids).padStart(8, "0")}`);
  const principal = CredentialSchema.parse("principal-transport-test-000001");
  const connection = ConnectionSchema.parse({
    actor: {
      principalId: "principal.transport",
      actorId: "actor.transport",
      workspaceId: "workspace.transport",
      conversationId: "conversation.transport",
      parentActorId: null,
      state: "active",
      capabilities: ["question:ask"],
      hostKind: "test",
      hostProvenance: "explicit_handle",
    },
    handle: {
      actorId: "actor.transport",
      bindingId: "binding.transport",
      workspaceId: "workspace.transport",
      conversationId: "conversation.transport",
      generation: "1",
    },
    credentials: {
      bindingToken: "binding-transport-test-000001",
      resumeCredential: "resume-transport-test-0000001",
    },
  });
  const host = HostBindingSchema.parse({
    kind: "test",
    sessionId: "native.parent",
    subagentId: "native.child",
    provenance: "explicit_handle",
  });
  const first = sessions.retain(principal, connection, host, { kind: "retain" });
  const duplicate = sessions.retain(principal, connection, host, { kind: "retain" });
  assert.equal(first.ok, true);
  assert.equal(duplicate.ok, true);
  if (!first.ok || !duplicate.ok) return;
  assert.equal(first.value.adapterSessionId, duplicate.value.adapterSessionId);
  const publicJson = JSON.stringify(first.value);
  assert.equal(publicJson.includes("bindingToken"), false);
  assert.equal(publicJson.includes("resumeCredential"), false);
});
