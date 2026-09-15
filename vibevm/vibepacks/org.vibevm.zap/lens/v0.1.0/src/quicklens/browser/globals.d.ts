import type { WorkspaceIpcBridge } from "./workspace-ipc.ts";
import type { QuicklensHostBridge } from "./host-bridge.ts";

declare global {
  interface Window {
    readonly quicklensHost?: QuicklensHostBridge;
    readonly lensWorkspace?: WorkspaceIpcBridge;
  }
}

export {};
