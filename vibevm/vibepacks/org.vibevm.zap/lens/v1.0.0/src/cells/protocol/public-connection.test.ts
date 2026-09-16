import assert from "node:assert/strict";
import { test } from "node:test";

import { ConnectionSchema, publicConnection, type Connection } from "./index.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-001#verification */
void test("public connection projection succeeds and excludes both credentials", () => {
  const connection: Connection = ConnectionSchema.parse({
    actor: {
      principalId: "prn_example",
      actorId: "act_example",
      workspaceId: "workspace_example",
      conversationId: "conversation_example",
      parentActorId: null,
      state: "active",
      capabilities: ["question:ask"],
      hostKind: "test",
      hostProvenance: "attested",
    },
    handle: {
      actorId: "act_example",
      bindingId: "bnd_example",
      workspaceId: "workspace_example",
      conversationId: "conversation_example",
      generation: "1",
    },
    credentials: {
      bindingToken: "binding_token_long_enough_for_schema",
      resumeCredential: "resume_token_long_enough_for_schema",
    },
  });

  const projected = publicConnection(connection);
  assert.deepEqual(Object.keys(projected).sort(), ["actor", "handle"]);
  assert.equal("credentials" in projected, false);
  assert.equal(JSON.stringify(projected).includes("binding_token"), false);
  assert.equal(JSON.stringify(projected).includes("resume_token"), false);
});
