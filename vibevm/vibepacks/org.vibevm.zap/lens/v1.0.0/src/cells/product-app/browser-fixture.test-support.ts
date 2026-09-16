/** No-model browser fixture for ordinary product startup. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { randomBytes } from "node:crypto";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  CoordinatorCapabilitiesSchema,
  CoordinatorSessionDescriptorSchema,
  runtimeFailure,
  type AgentRuntimeResult,
  type CoordinatorAdapter,
  type CoordinatorHistory,
  type CoordinatorSessionDescriptor,
  type CoordinatorStartInput,
  type CoordinatorTurnReceipt,
} from "../agent-runtime/index.ts";
import { createQuicklensGateway } from "../quicklens-service/index.ts";
import { unavailableDataSource } from "../quicklens-model/index.ts";
import { createWorkspaceService } from "../workspace-service/index.ts";
import { openWorkspaceStore } from "../workspace-store/index.ts";
import { ExecutionHostIdSchema, NativeRefSchema } from "../workspace-model/index.ts";
import {
  createDynamicWorkspacePort,
  createProductAppService,
  openProductAppRegistry,
  startLocalProductUi,
} from "./index.ts";

class NoModelAdapter implements CoordinatorAdapter {
  readonly capabilities = CoordinatorCapabilitiesSchema.parse({
    persistentThreads: true,
    turnStart: false,
    activeTurnSteer: false,
    turnInterrupt: false,
    historyRead: false,
    nativeChildObservation: false,
    nativeChildDirectInput: false,
    structuredUserInput: false,
    commandApproval: false,
    managedTerminal: false,
  });
  starts = 0;
  start(input: CoordinatorStartInput): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    this.starts += 1;
    return Promise.resolve({
      ok: true,
      value: CoordinatorSessionDescriptorSchema.parse({
        coordinatorSessionId: input.coordinatorSessionId,
        projectId: input.projectId,
        contextId: input.contextId,
        conversationId: input.conversationId,
        coordinatorActorId: input.coordinatorActorId,
        hostId: input.hostId,
        profileId: input.profileId,
        cwd: input.cwd,
        productId: "fixture",
        role: "coordinator",
        launchOrigin: "lens",
        interactionKind: "structured",
        state: "ready",
        nativeThreadRef: NativeRefSchema.parse({
          namespace: "fixture.thread",
          value: `thread.${input.projectId}`,
          incarnation: "1",
        }),
        nativeSessionId: `fixture.${input.projectId}`,
        processEpoch: "fixture-1",
        bootstrap: "submitted",
        instructionSources: ["no-model browser fixture"],
        capabilities: this.capabilities,
      }),
    });
  }
  resume(): Promise<AgentRuntimeResult<CoordinatorSessionDescriptor>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture resume is unavailable"));
  }
  readHistory(): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture history is unavailable"));
  }
  readNativeChildHistory(): Promise<AgentRuntimeResult<CoordinatorHistory>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture child history is unavailable"));
  }
  startTurn(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture turns are unavailable"));
  }
  steer(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture steering is unavailable"));
  }
  send(): Promise<AgentRuntimeResult<CoordinatorTurnReceipt>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture messages are unavailable"));
  }
  interrupt(): Promise<AgentRuntimeResult<void>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture interrupt is unavailable"));
  }
  respondToRequest(): Promise<AgentRuntimeResult<void>> {
    return Promise.resolve(runtimeFailure("unsupported", "fixture responses are unavailable"));
  }
  restart(): Promise<AgentRuntimeResult<readonly CoordinatorSessionDescriptor[]>> {
    return Promise.resolve({ ok: true, value: [] });
  }
  subscribe(): () => void {
    return () => undefined;
  }
  close(): void {}
}

const root = await mkdtemp(join(tmpdir(), "zap-product-browser-"));
const firstDirectory = await mkdtemp(join(root, "alpha-"));
const secondDirectory = await mkdtemp(join(root, "beta-"));
const ui = await startLocalProductUi({
  rendererRoot: resolve("dist/quicklens/browser"),
  port: 0,
});
if (!ui.ok) throw new Error(ui.error.message);
const store = openWorkspaceStore({ databasePath: join(root, "workspace.sqlite") });
if (!store.ok) throw new Error(store.error.message);
const registry = openProductAppRegistry(join(root, "product-projects.json"));
if (!registry.ok) throw new Error(registry.message);
const adapter = new NoModelAdapter();
const workspace = createWorkspaceService({
  store: store.value,
  adapters: {
    resolve: () =>
      Promise.resolve({
        ok: true,
        value: { hostId: ExecutionHostIdSchema.parse("host.product.fixture"), adapter },
      }),
  },
});
const product = createProductAppService({
  registry: registry.value,
  workspaceStore: store.value,
  providers: [
    {
      profileId: "profile.fixture.safe",
      provider: "codex",
      displayName: "Fixture agent · no model",
      modelId: "no-model-fixture",
      effort: null,
      interactionKind: "structured",
      installed: true,
      configured: true,
      authenticated: "not_observed",
      launchable: true,
      evidence: ["In-process acceptance adapter; no provider process"],
    },
  ],
});
if (!product.ok) throw new Error(product.error.message);
const gateway = createQuicklensGateway({
  source: unavailableDataSource("Fixture has no ZAP source."),
  namespace: "productbrowser",
  pairingToken: randomBytes(32).toString("base64url"),
  allowedHosts: ["127.0.0.1"],
  allowedOrigins: [ui.value.origin],
  multiSession: true,
  productSource: product.value,
  workspaceSource: (identity) =>
    createDynamicWorkspacePort({
      service: workspace,
      product: product.value,
      clientId: identity.clientId,
      principalId: `principal.${identity.sessionId}`,
    }),
});
if (!gateway.ok) throw new Error(gateway.error.message);
const started = await gateway.value.start({ host: "127.0.0.1", port: 0 });
if (!started.ok) throw new Error(started.error.message);
const firstTicket = gateway.value.issuePairingTicket?.();
const secondTicket = gateway.value.issuePairingTicket?.();
if (firstTicket?.ok !== true || secondTicket?.ok !== true)
  throw new Error(
    "violates REQ spec://org.vibevm.zap/lens/PROP-005#simultaneous-clients: pairing tickets are unavailable; fix surface: issue two bounded local UI tickets",
  );
const gatewayUrl = `http://127.0.0.1:${String(started.value.port)}${started.value.basePath}`;
const attach = (ticket: string): string =>
  `${ui.value.origin}/?workspace-gateway=${encodeURIComponent(gatewayUrl)}#workspace-pair=${encodeURIComponent(ticket)}`;
process.stdout.write(
  `${JSON.stringify({
    firstUrl: attach(firstTicket.value.ticket),
    secondUrl: attach(secondTicket.value.ticket),
    firstDirectory,
    secondDirectory,
    root,
  })}\n`,
);

const close = async (): Promise<void> => {
  await gateway.value.close();
  workspace.close();
  store.value.close();
  await ui.value.close();
  process.exit(0);
};
process.once("SIGINT", () => void close());
process.once("SIGTERM", () => void close());
setInterval(() => undefined, 60_000);
