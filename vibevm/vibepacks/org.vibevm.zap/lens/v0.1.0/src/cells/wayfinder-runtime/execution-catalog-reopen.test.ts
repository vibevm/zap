/** Catalog-selected project restart and revocation proof. @scope spec://org.vibevm.zap/lens/PROP-015#mock */
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createZapMockManagedControlAdapter } from "../managed-work/index.ts";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import {
  createWorkspaceHttpConnection,
  readProductExecutionCatalog,
  saveProductExecutionCatalog,
  type WorkspaceHttpConnection,
} from "../workspace-client/index.ts";
import { ExecutionHostIdSchema } from "../workspace-model/index.ts";
import { createWayfinderRuntime, type WayfinderRuntime } from "./index.ts";
import { MOCK_PROFILE_ID, writeMockProductScenario } from "./mock-managed.support.ts";
import {
  catalogRuntimeConfig,
  ensureCatalogRuntimeConfiguration,
} from "./execution-catalog-runtime.support.ts";

test("catalog project reopens before launch and disabled selection refuses future start", async () => {
  const root = mkdtempSync(join(tmpdir(), "zap-catalog-reopen-"));
  const projectDirectory = join(root, "registered-project");
  mkdirSync(projectDirectory);
  const behavior = writeMockProductScenario(root, "seed.catalog.reopen");
  const config = catalogRuntimeConfig(root, behavior.path);
  let runtime: WayfinderRuntime | null = null;
  try {
    let opened = await openRuntime(config, root);
    runtime = opened.runtime;
    const configurationId = await configureCatalog(opened.connection);
    const registered = await opened.connection.product.request({
      operation: "product.project.register.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.catalog.reopen.register"),
      directoryPath: projectDirectory,
      displayName: "Catalog reopen project",
      executionConfigurationId: configurationId,
    });
    assert.equal(registered.ok, true, JSON.stringify(registered));
    if (!registered.ok || registered.value.operation !== "product.project.register.v1") return;
    const project = registered.value.project;
    assert.equal(project.executionConfigurationId, configurationId);
    await runtime.close();
    runtime = null;

    opened = await openRuntime(config, root);
    runtime = opened.runtime;
    const setup = await opened.connection.product.request({ operation: "product.setup.get.v1" });
    assert.equal(setup.ok, true, JSON.stringify(setup));
    if (!setup.ok || setup.value.operation !== "product.setup.get.v1") return;
    const restored = setup.value.snapshot.projects.find(
      (candidate) => candidate.projectId === project.projectId,
    );
    assert.notEqual(restored, undefined);
    if (restored === undefined) return;
    const restoredView = await opened.connection.workspace.read({
      operation: "project.get.v1",
      projectId: restored.projectId,
      contextId: restored.contextId,
    });
    assert.equal(restoredView.ok, true, JSON.stringify(restoredView));
    if (!restoredView.ok || restoredView.value.operation !== "project.get.v1") return;
    assert.equal(
      restoredView.value.detail.coordinatorLaunchOptions.some(
        (option) => option.profileId === restored.profileId,
      ),
      true,
    );
    const catalog = await readProductExecutionCatalog(opened.connection.product);
    assert.equal(catalog.ok, true);
    if (!catalog.ok) return;
    const draft = {
      ...catalog.value.snapshot,
      configurations: catalog.value.snapshot.configurations.map((configuration) =>
        configuration.configurationId === configurationId
          ? { ...configuration, enabled: false }
          : configuration,
      ),
    };
    const disabled = await saveProductExecutionCatalog(
      opened.connection.product,
      catalog.value.snapshot,
      draft,
    );
    assert.equal(disabled.ok, true, disabled.ok ? undefined : disabled.error.message);
    await runtime.close();
    runtime = null;

    opened = await openRuntime(config, root);
    runtime = opened.runtime;
    const readable = await opened.connection.workspace.read({
      operation: "project.get.v1",
      projectId: project.projectId,
      contextId: project.contextId,
    });
    assert.equal(readable.ok, true, JSON.stringify(readable));
    if (!readable.ok || readable.value.operation !== "project.get.v1") return;
    const history = await opened.connection.workspace.events({
      cursor: {
        scope: { kind: "context", projectId: project.projectId, contextId: project.contextId },
        afterGlobalSequence: DecimalSchema.parse("0"),
      },
      limit: 64,
    });
    assert.equal(history.ok, true, JSON.stringify(history));
    const launch = readable.value.detail.coordinatorLaunchOptions.find(
      (candidate) => candidate.profileId === project.profileId,
    );
    assert.notEqual(launch, undefined);
    if (launch === undefined) return;
    const refused = await opened.connection.workspace.command({
      operation: "session.start.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.catalog.reopen.refused-start"),
      projectId: project.projectId,
      contextId: project.contextId,
      profileId: project.profileId,
      interactionKind: launch.interactionKind,
    });
    assert.equal(refused.ok, false);
  } finally {
    await runtime?.close();
    rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
});

async function openRuntime(
  config: ReturnType<typeof catalogRuntimeConfig>,
  root: string,
): Promise<{ readonly runtime: WayfinderRuntime; readonly connection: WorkspaceHttpConnection }> {
  const made = createWayfinderRuntime(config, {
    hosts: [catalogNoModelHost()],
    managedControlAdapters: [
      createZapMockManagedControlAdapter({ directory: join(root, "mock-control") }),
    ],
  });
  assert.equal(made.ok, true, JSON.stringify(made));
  const started = await made.value.start();
  assert.equal(started.ok, true, JSON.stringify(started));
  const ticket = made.value.issuePairingTicket();
  assert.equal(ticket.ok, true);
  const connection = createWorkspaceHttpConnection({
    baseUrl: `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`,
    pairingToken: ticket.value.ticket,
    origin: "http://mock-product.test",
  });
  assert.notEqual(connection, null);
  if (connection === null)
    assert.fail(
      "violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: workspace connection did not open; fix surface: inspect the paired gateway",
    );
  return { runtime: made.value, connection };
}

async function configureCatalog(connection: WorkspaceHttpConnection): Promise<string> {
  const configured = await ensureCatalogRuntimeConfiguration(connection.product);
  assert.equal(configured.ok, true, configured.ok ? undefined : configured.error.message);
  if (!configured.ok)
    assert.fail(
      "violates REQ spec://org.vibevm.zap/lens/PROP-015#mock: synthetic catalog configuration was not created; fix surface: inspect the protected mock binding",
    );
  assert.equal(configured.value.synthetic, true);
  return configured.value.configurationId;
}

function catalogNoModelHost() {
  return {
    hostId: ExecutionHostIdSchema.parse("host.catalog.reopen.synthetic"),
    profileIds: [MOCK_PROFILE_ID],
    openCoordinator: () =>
      Promise.resolve({
        ok: false as const,
        error: {
          code: "unsupported" as const,
          message: "Synthetic reopen proof never launches a coordinator.",
          retry: "never" as const,
        },
      }),
  };
}
