import type { ProductIpcBridge, WorkspaceIpcBridge } from "./workspace-ipc.ts";
import type { QuicklensHostBridge } from "./host-bridge.ts";

declare global {
  interface Window {
    readonly quicklensHost?: QuicklensHostBridge;
    readonly lensWorkspace?: WorkspaceIpcBridge;
    readonly lensProduct?: ProductIpcBridge;
  }
}

export {};
