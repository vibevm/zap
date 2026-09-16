/** Owned local sidecar process utilities. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import { spawn } from "node:child_process";
import { createServer } from "node:net";

export interface ManagedControlProcess {
  readonly pid: number;
  onExit(listener: (code: number | null) => void): () => void;
  kill(): void;
}

export interface ManagedControlProcessFactory {
  spawn(input: {
    readonly executable: string;
    readonly args: readonly string[];
    readonly cwd: string;
    readonly env: Readonly<Record<string, string>>;
  }): ManagedControlProcess;
}

export function nodeManagedControlProcessFactory(): ManagedControlProcessFactory {
  return {
    spawn(input) {
      const child = spawn(input.executable, [...input.args], {
        cwd: input.cwd,
        env: input.env,
        shell: false,
        windowsHide: true,
        stdio: "ignore",
      });
      const listeners = new Set<(code: number | null) => void>();
      child.once("exit", (code) => {
        for (const listener of listeners) listener(code);
      });
      return {
        pid: child.pid ?? 0,
        onExit(listener) {
          listeners.add(listener);
          return () => listeners.delete(listener);
        },
        kill() {
          child.kill();
        },
      };
    },
  };
}

export function reserveLoopbackPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (address === null || typeof address === "string") {
        server.close();
        reject(new Error());
        return;
      }
      server.close((error) => {
        if (error !== undefined) reject(error);
        else resolve(address.port);
      });
    });
  });
}
