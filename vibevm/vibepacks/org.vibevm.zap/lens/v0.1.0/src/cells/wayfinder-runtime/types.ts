/** Wayfinder runtime public assembly types. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import type { AgentHost } from "../agent-runtime/index.ts";
import type { CodexProcessFactory } from "../codex-coordinator/index.ts";
import type {
  ManagedAgentBackend,
  ProtectedEnvironmentPort,
  WorkAttachmentPort,
} from "../managed-work/index.ts";
import type { AnnotationService } from "../workspace-annotations/index.ts";
import type { WorkspaceManagedTerminalPort, WorkspaceService } from "../workspace-service/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
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
  readonly annotations?: AnnotationService;
  readonly annotationRestoreIntent?: AnnotationRestoreIntentPort;
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
  close(): Promise<void>;
}
