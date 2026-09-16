/** @scope spec://org.vibevm.zap/lens/PROP-006#managed-terminal */
/** Optional node-pty adapter. It is loaded only when managed terminals are enabled. */
import type { IPty } from "node-pty";
import type { ManagedTerminalFactory, ManagedTerminalProcess } from "./index.ts";
import { TerminalLaunchSpecSchema, type ManagedTerminalResult } from "./index.ts";

export async function createOptionalNodePtyFactory(
  enabled: boolean,
): Promise<ManagedTerminalResult<ManagedTerminalFactory>> {
  if (!enabled)
    return { ok: false, error: { code: "unsupported", message: "managed terminals are disabled" } };
  try {
    const module = await import("node-pty");
    return {
      ok: true,
      value: {
        spawn: async (rawSpec) => {
          await Promise.resolve();
          const spec = TerminalLaunchSpecSchema.safeParse(rawSpec);
          if (!spec.success)
            return {
              ok: false,
              error: { code: "invalid_input", message: "terminal launch spec is invalid" },
            };
          try {
            const process = module.spawn(spec.data.executable, spec.data.args, {
              name: "xterm-256color",
              cols: 120,
              rows: 32,
              cwd: spec.data.cwd,
              env: spec.data.env ?? processEnv(),
            });
            return { ok: true, value: adapt(process) };
          } catch {
            return {
              ok: false,
              error: {
                code: "unavailable",
                message: "node-pty could not spawn the trusted terminal",
              },
            };
          }
        },
      },
    };
  } catch {
    return { ok: false, error: { code: "unavailable", message: "node-pty is not installed" } };
  }
}

function adapt(process: IPty): ManagedTerminalProcess {
  return {
    processId: process.pid,
    onData(listener) {
      const subscription = process.onData(listener);
      return () => {
        subscription.dispose();
      };
    },
    onExit(listener) {
      const subscription = process.onExit((event) => {
        listener(event.exitCode);
      });
      return () => {
        subscription.dispose();
      };
    },
    write(data) {
      process.write(data);
    },
    resize(columns, rows) {
      process.resize(columns, rows);
    },
    interrupt() {
      process.write("\u0003");
    },
    stop() {
      process.kill();
    },
  };
}

function processEnv(): Record<string, string> {
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => entry[1] !== undefined,
    ),
  );
}
