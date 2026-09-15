/** @scope spec://org.vibevm.zap/lens/PROP-001#adapters */
import type { HostKind } from "../protocol/index.ts";
import type { HostCapabilities } from "./types.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-001#adapters */
export function getHostCapabilities(
  host: Exclude<HostKind, "lens" | "test">,
  platform: "win32" | "unix" | "other",
): HostCapabilities {
  switch (host) {
    case "codex":
      return {
        host,
        safePointContext: "supported",
        idleWake: "unsupported",
        activeTurnSteering: "conditional",
        existingSessionAttach: "not_verified",
        inputPaths: [
          {
            kind: "safe_point_hook",
            availability: "supported",
            idleWake: false,
            activeTurnSteering: false,
          },
          {
            kind: "managed_server",
            availability: "conditional",
            idleWake: true,
            activeTurnSteering: true,
          },
        ],
        limitations: [
          "background hook output waits for a safe point and never starts an idle turn",
          "app-server steering requires an established thread/server binding",
        ],
      };
    case "claude_code":
      return {
        host,
        safePointContext: "supported",
        idleWake: "conditional",
        activeTurnSteering: "conditional",
        existingSessionAttach: "conditional",
        inputPaths: [
          {
            kind: "safe_point_hook",
            availability: "supported",
            idleWake: false,
            activeTurnSteering: false,
          },
          {
            kind: "native_channel",
            availability: "conditional",
            idleWake: true,
            activeTurnSteering: false,
          },
        ],
        limitations: [
          "idle wake requires an opted-in channel or qualifying cross-session inbox",
          "channel transport write is not a model-consumption receipt",
        ],
      };
    case "opencode":
      return {
        host,
        safePointContext: "unsupported",
        idleWake: "conditional",
        activeTurnSteering: "unsupported",
        existingSessionAttach: "conditional",
        inputPaths: [
          {
            kind: "shared_server",
            availability: "conditional",
            idleWake: true,
            activeTurnSteering: false,
          },
        ],
        limitations: [
          "requires the exact shared server endpoint and session id",
          "busy prompt_async acceptance can persist input without scheduling a new turn",
        ],
      };
    case "qwen_code":
      return {
        host,
        safePointContext: "supported",
        idleWake: "conditional",
        activeTurnSteering: "unsupported",
        existingSessionAttach: "conditional",
        inputPaths: [
          {
            kind: "safe_point_hook",
            availability: "supported",
            idleWake: false,
            activeTurnSteering: false,
          },
          {
            kind: "regular_file_queue",
            availability: "conditional",
            idleWake: true,
            activeTurnSteering: false,
          },
          {
            kind: "native_peer",
            availability: platform === "win32" ? "unsupported" : "conditional",
            idleWake: platform !== "win32",
            activeTurnSteering: false,
          },
          {
            kind: "managed_server",
            availability: "conditional",
            idleWake: true,
            activeTurnSteering: false,
          },
        ],
        limitations: [
          platform === "win32"
            ? "native peer inbox uses Unix sockets and is unavailable on Windows"
            : "native peer inbox is opt-in and queues both now and next for a later turn",
          "installed 0.23.4 supports --input-file submit commands through a 500ms regular-file queue when enabled at launch",
          "dualOutput settings require restart and cannot attach to an existing unconfigured TUI",
          "confirmation_response is a human approval response and must never be generated automatically",
          "daemon control covers sessions it owns; ACP-driven peer inboxes refuse inbound messages",
        ],
      };
  }
}
