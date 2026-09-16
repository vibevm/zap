/** Exact generated Zap communication permission coverage. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import assert from "node:assert/strict";
import test from "node:test";

import { ActorIdSchema, ConversationIdSchema, WorkspaceIdSchema } from "../protocol/index.ts";
import { modelParameters } from "./helpers.ts";
import { CodexCoordinatorProfileSchema } from "./profile.ts";
import { codexProfileFixture } from "./test-support.ts";

test("generated Codex policy approves scoped Zap work tools and leaves plan tools prompting", () => {
  const fixture = codexProfileFixture();
  const profile = CodexCoordinatorProfileSchema.parse({
    ...fixture,
    lensMcp: {
      ...fixture.lensMcp,
      communicationPreauthorization: { allowDelegation: true },
    },
  });
  const serialized = JSON.stringify(
    modelParameters(
      profile.model,
      profile.effort,
      profile,
      {
        workspaceId: WorkspaceIdSchema.parse("workspace.preauthorized"),
        conversationId: ConversationIdSchema.parse("conversation.preauthorized"),
      },
      {
        actorId: ActorIdSchema.parse("actor.preauthorized"),
        adapterSessionId: "adapter.preauthorized.00000001",
      },
    ),
  );
  assert.match(serialized, /"default_tools_approval_mode":"prompt"/);
  for (const tool of [
    "codlens_assigned_context",
    "codlens_inbox_wait",
    "codlens_managed_work_start",
    "codlens_native_work_before",
    "codlens_delegate",
  ])
    assert.match(serialized, new RegExp(`"${tool}":\\{"approval_mode":"approve"\\}`));
  assert.doesNotMatch(serialized, /codlens_plan_apply/);
});
