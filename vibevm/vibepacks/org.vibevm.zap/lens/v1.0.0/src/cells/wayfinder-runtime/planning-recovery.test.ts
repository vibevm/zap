import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { PrincipalIdSchema } from "../protocol/index.ts";
import { openWorkspaceStore, TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import {
  ClientIdSchema,
  ProjectIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { reconcileDetachedPlanningSources } from "./planning-recovery.ts";

test("restart marks configured metadata unavailable when protected source is not restored", async () => {
  const root = mkdtempSync(join(tmpdir(), "zap-planning-recovery-"));
  const opened = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const store = opened.value;
  try {
    const registration = TrustedProjectRegistrationSchema.parse({
      registrationId: "registration.planning-recovery",
      projectId: "project.planning-recovery",
      displayName: "Planning recovery",
      repositoryRootRefs: ["repository.planning-recovery"],
      actions: { startCoordinator: { state: "available" } },
      context: {
        contextId: "context.planning-recovery",
        displayName: "Configured context",
        workspaceRef: "workspace.planning-recovery",
        branchLabel: "main",
        revisionBinding: "revision.fixture",
        planning: {
          state: "configured",
          storeId: "store.fixture",
          campaignId: "campaign.fixture",
          baseId: "base.fixture",
        },
        coordinatorConversationId: "conversation.planning-recovery",
      },
      coordinatorLaunchOptions: [
        {
          profileId: "profile.fixture",
          label: "Fixture",
          interactionKind: "structured",
          availability: { state: "available" },
        },
      ],
      protected: { cwd: root, launchProfileRef: "profile.fixture" },
    });
    assert.equal(store.registerProject(registration).ok, true);
    const recovered = await reconcileDetachedPlanningSources({
      store,
      planning: null,
      repositories: null,
      projectIds: [registration.projectId],
    });
    assert.equal(recovered.ok, true);
    const access = WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.planning-recovery-test"),
      actorId: null,
      clientId: ClientIdSchema.parse("client.planning-recovery-test"),
      authorizedProjectIds: [ProjectIdSchema.parse(registration.projectId)],
    });
    const project = store.read(access, {
      operation: "project.get.v1",
      projectId: ProjectIdSchema.parse(registration.projectId),
    });
    assert.equal(project.ok, true);
    if (project.ok && project.value.operation === "project.get.v1") {
      assert.equal(project.value.detail.contexts[0]?.planning.state, "unavailable");
    }
  } finally {
    store.close();
    rmSync(root, { recursive: true, force: true });
  }
});
