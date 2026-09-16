/** Internal Codex adapter state. @scope spec://org.vibevm.zap/lens/PROP-005#coordinator-lifecycle */
import type {
  CoordinatorResumeInput,
  CoordinatorLifecycleReceipt,
  CoordinatorSessionDescriptor,
  CoordinatorStartInput,
  PendingHostRequest,
} from "../agent-runtime/index.ts";
import type { CodexRpcProcess } from "./process.ts";
import type { CodexCoordinatorProfile } from "./profile.ts";
import type { CodexThread } from "./protocol.ts";
import type { ReasoningEffort } from "../model-policy/index.ts";

export interface SessionState {
  descriptor: CoordinatorSessionDescriptor;
  readonly start: CoordinatorStartInput | CoordinatorResumeInput;
  readonly modelId: string;
  readonly reasoningEffort: ReasoningEffort | null;
  activeTurnId: string | null;
  readonly childThreads: Map<string, CodexThread | null>;
  readonly childActiveTurns: Map<string, string>;
  readonly pauseTargets: Set<string>;
  lifecycle: "active" | "pause_requested" | "paused" | "stop_requested" | "stopped" | "uncertain";
  pauseObservation: CoordinatorLifecycleReceipt["observation"] | null;
  stopObservation: CoordinatorLifecycleReceipt["observation"] | null;
  readonly pending: Map<string, PendingHostRequest>;
  /** Requests that were written successfully and await serverRequest/resolved. */
  readonly answeredRequests: Set<string>;
  /** Request keys invalidated by a process epoch change. */
  readonly retiredRequests: Set<string>;
  serial: Promise<void>;
}

export interface WorkerState {
  readonly ownerCoordinatorSessionId: string;
  readonly profile: CodexCoordinatorProfile;
  readonly process: CodexRpcProcess;
  readonly epoch: string;
  readonly incarnation: string;
  sequence: number;
  unsubscribe: () => void;
  unsubscribeExit: () => void;
}
