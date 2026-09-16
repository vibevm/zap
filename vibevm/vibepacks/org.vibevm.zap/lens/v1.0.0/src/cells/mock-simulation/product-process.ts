/** Owned product-test subprocess lifecycle. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import { spawn } from "node:child_process";
import { join } from "node:path";

export interface OwnedProductProcessResult {
  readonly exitCode: number | null;
  readonly output: string;
  readonly timedOut: boolean;
  readonly exactTreeTerminated: boolean;
  readonly startError: boolean;
  readonly readyObserved: boolean;
}

export function runOwnedProductProcess(input: {
  readonly root: string;
  readonly testPath: string;
  readonly environment: Readonly<Record<string, string>>;
  readonly timeoutMs: number;
  readonly readyMarker?: string;
  readonly startupTimeoutMs?: number;
}): Promise<OwnedProductProcessResult> {
  return new Promise((resolveResult) => {
    const child = spawn(process.execPath, ["--test", input.testPath], {
      cwd: input.root,
      env: input.environment,
      windowsHide: true,
      detached: process.platform !== "win32",
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    let settled = false;
    let timedOut = false;
    let startError = false;
    let readyObserved = input.readyMarker === undefined;
    let termination: Promise<boolean> | null = null;
    const append = (chunk: Buffer | string) => {
      if (output.length < 1_000_000) output += String(chunk).slice(0, 1_000_000 - output.length);
      if (!readyObserved && output.includes(input.readyMarker ?? "")) {
        readyObserved = true;
        clearTimeout(startupTimer);
        armTimeout();
      }
    };
    child.stdout.on("data", append);
    child.stderr.on("data", append);
    const finish = (exitCode: number | null, exactTreeTerminated: boolean) => {
      if (settled) return;
      settled = true;
      if (timer !== null) clearTimeout(timer);
      clearTimeout(startupTimer);
      resolveResult({
        exitCode,
        output,
        timedOut,
        exactTreeTerminated,
        startError,
        readyObserved,
      });
    };
    let timer: ReturnType<typeof setTimeout> | null = null;
    const expire = () => {
      timedOut = true;
      termination = terminateOwnedProcessTree(child.pid, input.environment);
      void termination.then((terminated) => {
        if (!terminated) child.kill("SIGKILL");
      });
    };
    const armTimeout = () => {
      timer ??= setTimeout(expire, input.timeoutMs);
    };
    const startupTimer = setTimeout(expire, input.startupTimeoutMs ?? input.timeoutMs);
    if (readyObserved) {
      clearTimeout(startupTimer);
      armTimeout();
    }
    child.once("error", () => {
      startError = true;
      finish(null, false);
    });
    child.once("close", (code) => {
      if (termination === null) finish(code, false);
      else
        void termination.then((exactTreeTerminated) => {
          finish(code, exactTreeTerminated);
        });
    });
  });
}

async function terminateOwnedProcessTree(
  pid: number | undefined,
  environment: Readonly<Record<string, string>>,
): Promise<boolean> {
  if (pid === undefined) return false;
  if (process.platform !== "win32") {
    try {
      process.kill(-pid, "SIGKILL");
      return true;
    } catch {
      return false;
    }
  }
  const systemRoot = environment["SystemRoot"] ?? environment["WINDIR"];
  if (systemRoot === undefined) return false;
  return new Promise((resolveTermination) => {
    const taskkill = spawn(
      join(systemRoot, "System32", "taskkill.exe"),
      ["/PID", String(pid), "/T", "/F"],
      {
        windowsHide: true,
        stdio: "ignore",
        env: environment,
      },
    );
    taskkill.once("error", () => {
      resolveTermination(false);
    });
    taskkill.once("close", (code) => {
      resolveTermination(code === 0);
    });
  });
}
