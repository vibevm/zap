/** Wayfinder runtime public assembly types. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type { AgentHost } from "../agent-runtime/index.ts";
import type { CodexProcessFactory } from "../codex-coordinator/index.ts";
import type {
  ManagedAgentBackend,
  ManagedProviderControlAdapter,
  ProtectedEnvironmentPort,
  WorkAttachmentPort,
} from "../managed-work/index.ts";
import type { AnnotationService } from "../workspace-annotations/index.ts";
import type { WorkspaceManagedTerminalPort, WorkspaceService } from "../workspace-service/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { AlgorithmBinding } from "../repository-model/index.ts";
import type { WorkspacePlanningAttachment } from "../workspace-planning/index.ts";
import type { AnnotationRestoreIntentPort } from "./annotations.ts";

export type WayfinderResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: { readonly code: string; readonly message: string } };

export interface WayfinderRuntimeOptions {
  readonly store?: WorkspaceStore;
  readonly hosts?: readonly AgentHost[];
  readonly processFactory?: CodexProcessFactory;
  readonly terminals?: WorkspaceManagedTerminalPort;
  readonly managedWork?: ManagedAgentBackend;
  readonly managedEnvironment?: ProtectedEnvironmentPort;
  readonly managedAttachments?: WorkAttachmentPort;
  readonly managedControlAdapters?: readonly ManagedProviderControlAdapter[];
  readonly annotations?: AnnotationService;
  readonly annotationRestoreIntent?: AnnotationRestoreIntentPort;
  readonly observeGatewayRequest?: (event: {
    readonly method: string;
    readonly path: string;
    readonly status: number;
    readonly durationMs: number;
  }) => void;
}

export interface WayfinderReceipt {
  readonly host: string;
  readonly port: number;
  readonly basePath: string;
  readonly databasePath: string;
  readonly projectIds: readonly string[];
  readonly agentGateway: { readonly host: string; readonly port: number } | null;
  readonly web?: { readonly host: string; readonly port: number } | null;
}

export interface WayfinderRuntime {
  readonly service: WorkspaceService;
  readonly store: WorkspaceStore;
  readonly receipt: WayfinderReceipt | null;
  start(): Promise<WayfinderResult<WayfinderReceipt>>;
  issuePairingTicket(): WayfinderResult<{ readonly ticket: string; readonly expiresAt: string }>;
  attachPlanningSource(
    input: WorkspacePlanningAttachment & {
      readonly planId: string;
      readonly principalId: string;
    },
  ): Promise<WayfinderResult<AlgorithmBinding>>;
  close(): Promise<void>;
}
