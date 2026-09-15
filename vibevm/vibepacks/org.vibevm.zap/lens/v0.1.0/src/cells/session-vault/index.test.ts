import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { openBroker } from "../broker/index.ts";
import {
  CredentialSchema,
  DecimalSchema,
  EnrollPrincipalInputSchema,
  HostBindingSchema,
} from "../protocol/index.ts";
import { AdapterSessionIdSchema } from "../transport/index.ts";
import { openSqliteAdapterSessionVault } from "./index.ts";

test("SQLite adapter vault preserves native routing and offer cursor across reopen", () => {
  const path = join(mkdtempSync(join(tmpdir(), "codlens-vault-")), "lens.sqlite");
  const broker = openBroker({ databasePath: path });
  assert.equal(broker.ok, true);
  if (!broker.ok) return;
  const enrolled = broker.value.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "agent",
      workspaceIds: ["workspace.vault"],
      conversationIds: ["conversation.vault"],
      capabilities: ["question:ask"],
    }),
  );
  assert.equal(enrolled.ok, true);
  if (!enrolled.ok) return;
  const host = HostBindingSchema.parse({
    kind: "test",
    sessionId: "native.session",
    subagentId: "native.child",
    provenance: "explicit_handle",
  });
  const connected = broker.value.connect({
    principalToken: enrolled.value.principalToken,
    clientRequestId: "request.vault.connect",
    workspaceId: "workspace.vault",
    conversationId: "conversation.vault",
    capabilities: ["question:ask"],
    host,
    replyPolicy: { kind: "forward_parent" },
  });
  assert.equal(connected.ok, true);
  if (!connected.ok) return;
  const id = AdapterSessionIdSchema.parse("adapter.vault.session.000001");
  const first = openSqliteAdapterSessionVault(path);
  assert.equal(first.ok, true);
  if (!first.ok) return;
  assert.equal(
    first.value.put(id, {
      principalToken: CredentialSchema.parse(enrolled.value.principalToken),
      connection: connected.value,
      host,
      replyPolicy: { kind: "forward_parent" },
    }).ok,
    true,
  );
  assert.equal(
    first.value.setOfferCursor(connected.value.actor.actorId, DecimalSchema.parse("17")).ok,
    true,
  );
  first.value.close();
  const reopened = openSqliteAdapterSessionVault(path);
  assert.equal(reopened.ok, true);
  if (!reopened.ok) return;
  const stored = reopened.value.get(id);
  assert.equal(stored.ok, true);
  if (!stored.ok) return;
  assert.equal(stored.value.host.sessionId, "native.session");
  assert.equal(stored.value.host.subagentId, "native.child");
  assert.equal(reopened.value.offerCursor(connected.value.actor.actorId).ok, true);
  const cursor = reopened.value.offerCursor(connected.value.actor.actorId);
  assert.equal(cursor.ok && cursor.value, "17");
  reopened.value.close();
  broker.value.close();
});
