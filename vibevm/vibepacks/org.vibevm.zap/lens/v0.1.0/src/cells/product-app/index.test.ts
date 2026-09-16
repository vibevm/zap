/** @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { AgentRuntimeErrorSchema } from "../agent-runtime/index.ts";
import { ClientRequestIdSchema } from "../protocol/index.ts";
import { createWorkspaceService } from "../workspace-service/index.ts";
import { unavailableDataSource } from "../quicklens-model/index.ts";
import { createQuicklensGateway } from "../quicklens-service/index.ts";
import { createWorkspaceHttpConnection } from "../workspace-client/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import type {
  ProductProviderProfile,
  WorkspaceReadResponse,
  WorkspaceResult,
} from "../workspace-model/index.ts";
import {
  createDynamicWorkspacePort,
  createProductAppService,
  openProductAppRegistry,
  startLocalProductUi,
} from "./index.ts";

const provider: ProductProviderProfile = {
  profileId: "profile.codex.local",
  provider: "codex",
  displayName: "Codex · local profile",
  modelId: "configured-model",
  effort: "low",
  interactionKind: "structured",
  installed: true,
  configured: true,
  authenticated: "not_observed",
  launchable: true,
  evidence: ["Synthetic protected profile for setup proof"],
};

test("two clients discover durable project registrations without starting a provider", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "zap-product-start-"));
  const firstDirectory = await mkdtemp(join(root, "first-"));
  const secondDirectory = await mkdtemp(join(root, "second-"));
  const storeResult = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(storeResult.ok, true);
  if (!storeResult.ok) return;
  const registryResult = openProductAppRegistry(join(root, "product-projects.json"));
  assert.equal(registryResult.ok, true);
  if (!registryResult.ok) return;
  let adapterResolutions = 0;
  const missingAdapter = AgentRuntimeErrorSchema.parse({
    code: "not_found",
    message: "fixture has no provider process",
    retry: "never",
  });
  const workspace = createWorkspaceService({
    store: storeResult.value,
    adapters: {
      resolve: () => {
        adapterResolutions += 1;
        return Promise.resolve({ ok: false, error: missingAdapter });
      },
    },
  });
  t.after(() => {
    workspace.close();
    storeResult.value.close();
  });
  const productResult = createProductAppService({
    registry: registryResult.value,
    workspaceStore: storeResult.value,
    providers: [provider],
  });
  assert.equal(productResult.ok, true);
  if (!productResult.ok) return;
  assert.equal(productResult.value.hydrate().ok, true);
  const firstClient = createDynamicWorkspacePort({
    service: workspace,
    product: productResult.value,
    clientId: "client.setup.first",
    principalId: "principal.setup.owner",
  });
  const secondClient = createDynamicWorkspacePort({
    service: workspace,
    product: productResult.value,
    clientId: "client.setup.second",
    principalId: "principal.setup.owner",
  });

  const empty = await firstClient.read({ operation: "project.list.v1" });
  assert.equal(projectCount(empty), 0);
  const first = await productResult.value.request({
    operation: "product.project.register.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.setup.first"),
    directoryPath: firstDirectory,
    displayName: "First project",
    profileId: provider.profileId,
  });
  assert.equal(first.ok, true);
  assert.equal(adapterResolutions, 0);
  const second = await productResult.value.request({
    operation: "product.project.register.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.setup.second"),
    directoryPath: secondDirectory,
    displayName: "Second project",
    profileId: provider.profileId,
  });
  assert.equal(second.ok, true);
  assert.equal(adapterResolutions, 0);

  assert.equal(projectCount(await firstClient.read({ operation: "project.list.v1" })), 2);
  assert.equal(projectCount(await secondClient.read({ operation: "project.list.v1" })), 2);
  if (!first.ok || first.value.operation !== "product.project.register.v1") return;
  const started = await firstClient.command({
    operation: "session.start.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.start.first"),
    projectId: first.value.project.projectId,
    contextId: first.value.project.contextId,
    interactionKind: provider.interactionKind,
    profileId: provider.profileId,
  });
  assert.equal(started.ok, false);
  assert.equal(adapterResolutions, 1);
});

test("local product UI serves only bounded built assets", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "zap-product-ui-"));
  await writeFile(join(root, "index.html"), "<main>Zap Quick Lens setup</main>");
  const started = await startLocalProductUi({ rendererRoot: root, port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  t.after(() => started.value.close());
  const page = await fetch(started.value.origin);
  assert.equal(page.status, 200);
  assert.match(await page.text(), /Zap Quick Lens setup/);
  assert.match(page.headers.get("content-security-policy") ?? "", /connect-src/);
  const escaped = await fetch(`${started.value.origin}/../outside.txt`);
  assert.equal(escaped.status, 404);
});

test("two authenticated HTTP clients share one live product registration catalog", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "zap-product-http-"));
  const directory = await mkdtemp(join(root, "project-"));
  const storeResult = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
  assert.equal(storeResult.ok, true);
  if (!storeResult.ok) return;
  const registryResult = openProductAppRegistry(join(root, "product-projects.json"));
  assert.equal(registryResult.ok, true);
  if (!registryResult.ok) return;
  const workspace = createWorkspaceService({
    store: storeResult.value,
    adapters: { resolve: () => Promise.resolve({ ok: false, error: missingAdapterError() }) },
  });
  const productResult = createProductAppService({
    registry: registryResult.value,
    workspaceStore: storeResult.value,
    providers: [provider],
  });
  assert.equal(productResult.ok, true);
  if (!productResult.ok) return;
  const origin = "http://127.0.0.1:4174";
  const gatewayResult = createQuicklensGateway({
    source: unavailableDataSource("Product setup proof has no ZAP source."),
    namespace: "productsetup",
    pairingToken: randomBytes(32).toString("base64url"),
    allowedHosts: ["127.0.0.1"],
    allowedOrigins: [origin],
    multiSession: true,
    productSource: productResult.value,
    workspaceSource: (identity) =>
      createDynamicWorkspacePort({
        service: workspace,
        product: productResult.value,
        clientId: identity.clientId,
        principalId: `principal.${identity.sessionId}`,
      }),
  });
  assert.equal(gatewayResult.ok, true);
  if (!gatewayResult.ok) return;
  const started = await gatewayResult.value.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  t.after(async () => {
    await gatewayResult.value.close();
    workspace.close();
    storeResult.value.close();
  });
  const firstTicket = gatewayResult.value.issuePairingTicket?.();
  const secondTicket = gatewayResult.value.issuePairingTicket?.();
  assert.equal(firstTicket?.ok, true);
  assert.equal(secondTicket?.ok, true);
  if (!firstTicket?.ok || !secondTicket?.ok) return;
  const baseUrl = `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`;
  const first = createWorkspaceHttpConnection({
    baseUrl,
    origin,
    pairingToken: firstTicket.value.ticket,
  });
  const second = createWorkspaceHttpConnection({
    baseUrl,
    origin,
    pairingToken: secondTicket.value.ticket,
  });
  assert.notEqual(first, null);
  assert.notEqual(second, null);
  if (first === null || second === null) return;
  const registered = await first.product.request({
    operation: "product.project.register.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.http.register"),
    directoryPath: directory,
    displayName: "HTTP project",
    profileId: provider.profileId,
  });
  assert.equal(registered.ok, true);
  assert.equal(projectCount(await first.workspace.read({ operation: "project.list.v1" })), 1);
  assert.equal(projectCount(await second.workspace.read({ operation: "project.list.v1" })), 1);
});

function projectCount(result: WorkspaceResult<WorkspaceReadResponse>): number {
  return result.ok && result.value.operation === "project.list.v1"
    ? result.value.projects.length
    : -1;
}

function missingAdapterError() {
  return AgentRuntimeErrorSchema.parse({
    code: "not_found",
    message: "fixture has no provider process",
    retry: "never",
  });
}
