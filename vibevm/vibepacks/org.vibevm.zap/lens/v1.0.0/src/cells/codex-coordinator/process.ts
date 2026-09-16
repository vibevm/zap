/**
 * Trusted, bounded JSONL process transport for Codex app-server.
 *
 * @scope spec://org.vibevm.zap/lens/PROP-006#supervision
 * @scope spec://org.vibevm.zap/lens/PROP-009#stop
 */
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomUUID } from "node:crypto";
import { access } from "node:fs/promises";
import { constants } from "node:fs";
import { basename, delimiter, dirname, isAbsolute, join, resolve } from "node:path";
import { z } from "zod";
import { ExecutionCatalogIdSchema } from "../execution-catalog/index.ts";
import {
  resolveProxyEnvironment,
  ProxyPolicySchema,
  type ProxyPolicy,
} from "../proxy-policy/index.ts";
import type { JsonValue } from "../protocol/index.ts";
import {
  CodexRpcIdSchema,
  CodexWireMessageSchema,
  type CodexRpcId,
  type CodexWireMessage,
} from "./protocol.ts";

const MAX_LINE_BYTES = 16 * 1024 * 1024;
const MAX_STDERR_BYTES = 64 * 1024;

export const CodexProcessErrorSchema = z
  .object({
    kind: z.enum(["spawn_failed", "transport_closed", "timeout", "protocol", "rpc"]),
    message: z.string(),
    rpcCode: z.number().int().optional(),
  })
  .strict();
export type CodexProcessError = z.infer<typeof CodexProcessErrorSchema>;
export type CodexProcessResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: CodexProcessError };

export interface CodexRpcProcess {
  readonly epoch: string;
  request(
    method: string,
    params: JsonValue,
    signal?: AbortSignal,
  ): Promise<CodexProcessResult<JsonValue>>;
  notify(method: string, params: JsonValue): CodexProcessResult<void>;
  respond(id: CodexRpcId, result: JsonValue): CodexProcessResult<void>;
  subscribe(listener: (message: CodexWireMessage) => void): () => void;
  onExit(listener: (exit: { code: number | null; diagnostic: string }) => void): () => void;
  terminate(timeoutMs?: number): Promise<CodexProcessResult<{ code: number | null }>>;
  close(): void;
}

export interface CodexProcessFactory {
  start(
    profile: CodexProcessProfile,
    launchCwd: string,
  ): Promise<CodexProcessResult<CodexRpcProcess>>;
}

export const CodexProcessProfileSchema = z
  .object({
    executablePath: z.string().min(1),
    requestTimeoutMs: z.number().int().min(1_000).max(300_000).default(30_000),
    accountBindingId: ExecutionCatalogIdSchema.optional(),
    proxy: ProxyPolicySchema.optional(),
  })
  .strict();
export type CodexProcessProfile = z.infer<typeof CodexProcessProfileSchema>;

export function createNodeCodexProcessFactory(options?: {
  readonly environment?: Readonly<Record<string, string | undefined>>;
  readonly proxyPolicy?: ProxyPolicy;
}): CodexProcessFactory {
  return {
    start: async (rawProfile, launchCwd) => {
      const parsed = CodexProcessProfileSchema.safeParse(rawProfile);
      if (!parsed.success || !isAbsolute(parsed.data.executablePath) || !isAbsolute(launchCwd)) {
        return failure("spawn_failed", "Codex executable path and launch cwd must be absolute");
      }
      const cwd = resolve(launchCwd);
      try {
        await access(parsed.data.executablePath, constants.X_OK);
        const environment = codexProcessEnvironment(parsed.data, options);
        const child = spawn(parsed.data.executablePath, ["app-server", "--listen", "stdio://"], {
          env: environment.environment,
          cwd,
          shell: false,
          windowsHide: true,
          stdio: ["pipe", "pipe", "pipe"],
        });
        return { ok: true, value: new JsonlProcess(child, parsed.data.requestTimeoutMs) };
      } catch (error: unknown) {
        return failure("spawn_failed", safeMessage(error));
      }
    },
  };
}

export function codexProcessEnvironment(
  profile: CodexProcessProfile,
  options?: {
    readonly environment?: Readonly<Record<string, string | undefined>>;
    readonly proxyPolicy?: ProxyPolicy;
  },
) {
  return resolveProxyEnvironment({
    ambient: options?.environment ?? process.env,
    ...(options?.proxyPolicy === undefined ? {} : { global: options.proxyPolicy }),
    ...(profile.proxy === undefined ? {} : { profile: profile.proxy }),
  });
}

export async function resolveInstalledCodexExecutable(input?: {
  readonly configuredPath?: string;
  readonly searchPath?: string;
  readonly platform?: NodeJS.Platform;
  readonly arch?: string;
}): Promise<CodexProcessResult<string>> {
  const platform = input?.platform ?? process.platform;
  const arch = input?.arch ?? process.arch;
  const configured = input?.configuredPath;
  if (configured !== undefined) {
    if (!isAbsolute(configured)) {
      return failure("spawn_failed", "Configured Codex executable path must be absolute");
    }
    return (await executable(configured))
      ? { ok: true, value: configured }
      : failure("spawn_failed", "Configured Codex executable is not accessible");
  }

  const directories = new Set<string>([dirname(process.execPath)]);
  for (const entry of (input?.searchPath ?? process.env["PATH"] ?? "").split(delimiter)) {
    if (entry.length > 0) directories.add(entry);
  }
  for (const directory of directories) {
    for (const candidate of executableCandidates(directory, platform, arch)) {
      if (await executable(candidate)) return { ok: true, value: candidate };
    }
  }
  return failure("spawn_failed", "Native Codex executable was not found in the configured PATH");
}

function executableCandidates(
  directory: string,
  platform: NodeJS.Platform,
  arch: string,
): string[] {
  const direct = join(directory, platform === "win32" ? "codex.exe" : "codex");
  if (platform !== "win32") return [direct];
  const target = arch === "arm64" ? "win32-arm64" : "win32-x64";
  const triplet = arch === "arm64" ? "aarch64-pc-windows-msvc" : "x86_64-pc-windows-msvc";
  return [
    direct,
    join(
      directory,
      "node_modules",
      "@openai",
      "codex",
      "node_modules",
      "@openai",
      `codex-${target}`,
      "vendor",
      triplet,
      "bin",
      "codex.exe",
    ),
  ];
}

async function executable(path: string): Promise<boolean> {
  try {
    await access(path, constants.X_OK);
    return basename(path).length > 0;
  } catch {
    return false;
  }
}

interface PendingRequest {
  readonly resolve: (result: CodexProcessResult<JsonValue>) => void;
  readonly timer: ReturnType<typeof setTimeout>;
}

class JsonlProcess implements CodexRpcProcess {
  readonly epoch = randomUUID();
  readonly #messages = new Set<(message: CodexWireMessage) => void>();
  readonly #exits = new Set<(exit: { code: number | null; diagnostic: string }) => void>();
  readonly #pending = new Map<number, PendingRequest>();
  readonly #child: ChildProcessWithoutNullStreams;
  readonly #timeoutMs: number;
  #nextId = 1;
  #buffer = Buffer.alloc(0);
  #stderr = Buffer.alloc(0);
  #closed = false;
  #exit: { code: number | null } | null = null;

  constructor(child: ChildProcessWithoutNullStreams, timeoutMs: number) {
    this.#child = child;
    this.#timeoutMs = timeoutMs;
    child.stdout.on("data", (chunk: Buffer) => {
      this.#receive(chunk);
    });
    child.stderr.on("data", (chunk: Buffer) => {
      if (this.#stderr.length >= MAX_STDERR_BYTES) return;
      this.#stderr = Buffer.concat([this.#stderr, chunk]).subarray(0, MAX_STDERR_BYTES);
    });
    child.once("error", (error) => {
      this.#finish(null, safeMessage(error));
    });
    child.once("exit", (code) => {
      this.#finish(code, this.#stderr.toString("utf8"));
    });
  }

  request(
    method: string,
    params: JsonValue,
    signal?: AbortSignal,
  ): Promise<CodexProcessResult<JsonValue>> {
    if (this.#closed) return Promise.resolve(failure("transport_closed", "Codex process closed"));
    if (signal?.aborted === true) {
      return Promise.resolve(failure("timeout", `Codex request ${method} was aborted`));
    }
    const id = this.#nextId++;
    const wrote = this.#write({ id, method, params });
    if (!wrote.ok) return Promise.resolve(wrote);
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        resolve(failure("timeout", `Codex request ${method} timed out`));
      }, this.#timeoutMs);
      this.#pending.set(id, { resolve, timer });
      if (signal !== undefined) {
        signal.addEventListener(
          "abort",
          () => {
            const pending = this.#pending.get(id);
            if (pending === undefined) return;
            clearTimeout(pending.timer);
            this.#pending.delete(id);
            resolve(failure("timeout", `Codex request ${method} was aborted`));
          },
          { once: true },
        );
      }
    });
  }

  notify(method: string, params: JsonValue): CodexProcessResult<void> {
    return this.#write({ method, params });
  }

  respond(id: CodexRpcId, result: JsonValue): CodexProcessResult<void> {
    const parsedId = CodexRpcIdSchema.safeParse(id);
    return parsedId.success
      ? this.#write({ id: parsedId.data, result })
      : failure("protocol", "Codex response id is invalid");
  }

  subscribe(listener: (message: CodexWireMessage) => void): () => void {
    this.#messages.add(listener);
    return () => this.#messages.delete(listener);
  }

  onExit(listener: (exit: { code: number | null; diagnostic: string }) => void): () => void {
    this.#exits.add(listener);
    return () => this.#exits.delete(listener);
  }

  terminate(timeoutMs = 10_000): Promise<CodexProcessResult<{ code: number | null }>> {
    if (this.#exit !== null) return Promise.resolve({ ok: true, value: this.#exit });
    if (this.#closed) {
      return Promise.resolve(failure("transport_closed", "Codex process exit is unobserved"));
    }
    return new Promise((resolveTermination) => {
      let settled = false;
      const finish = (result: CodexProcessResult<{ code: number | null }>) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        unsubscribe();
        resolveTermination(result);
      };
      const unsubscribe = this.onExit((exit) => {
        finish({ ok: true, value: { code: exit.code } });
      });
      const timer = setTimeout(() => {
        finish(failure("timeout", "Codex process exit was not observed before timeout"));
      }, timeoutMs);
      this.#child.kill();
    });
  }

  close(): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#child.kill();
    this.#rejectPending(processError("transport_closed", "Codex process closed"));
  }

  #write(value: JsonValue): CodexProcessResult<void> {
    if (this.#closed || !this.#child.stdin.writable) {
      return failure("transport_closed", "Codex process input is unavailable");
    }
    const encoded = `${JSON.stringify(value)}\n`;
    if (Buffer.byteLength(encoded) > MAX_LINE_BYTES) {
      return failure("protocol", "Codex request exceeded the JSONL line bound");
    }
    this.#child.stdin.write(encoded);
    return { ok: true, value: undefined };
  }

  #receive(chunk: Buffer): void {
    if (this.#closed) return;
    this.#buffer = Buffer.concat([this.#buffer, chunk]);
    for (;;) {
      const newline = this.#buffer.indexOf(0x0a);
      if (newline < 0) {
        if (this.#buffer.length > MAX_LINE_BYTES) {
          this.#rejectProtocol("Codex emitted an oversized JSONL line");
        }
        return;
      }
      if (newline > MAX_LINE_BYTES) {
        this.#rejectProtocol("Codex emitted an oversized JSONL line");
        return;
      }
      const line = this.#buffer.subarray(0, newline).toString("utf8").trim();
      this.#buffer = this.#buffer.subarray(newline + 1);
      if (line.length === 0) continue;
      this.#parseLine(line);
    }
  }

  #parseLine(line: string): void {
    let decoded: unknown;
    try {
      decoded = JSON.parse(line);
    } catch {
      this.#rejectProtocol("Codex emitted malformed JSONL");
      return;
    }
    const message = CodexWireMessageSchema.safeParse(decoded);
    if (!message.success) {
      this.#rejectProtocol("Codex emitted a message outside the validated protocol envelope");
      return;
    }
    if ("id" in message.data && !("method" in message.data)) {
      const numericId = typeof message.data.id === "number" ? message.data.id : undefined;
      if (numericId !== undefined) {
        const pending = this.#pending.get(numericId);
        if (pending === undefined) return;
        clearTimeout(pending.timer);
        this.#pending.delete(numericId);
        pending.resolve(
          "error" in message.data
            ? {
                ok: false,
                error: {
                  kind: "rpc",
                  message: message.data.error.message,
                  rpcCode: message.data.error.code,
                },
              }
            : "result" in message.data
              ? { ok: true, value: message.data.result }
              : failure("protocol", "Codex response omitted both result and error"),
        );
        return;
      }
    }
    for (const listener of this.#messages) listener(message.data);
  }

  #finish(code: number | null, diagnostic: string): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#exit = { code };
    this.#rejectPending(processError("transport_closed", "Codex process exited"));
    const safeDiagnostic = diagnostic.slice(0, MAX_STDERR_BYTES);
    for (const listener of this.#exits) listener({ code, diagnostic: safeDiagnostic });
  }

  #rejectProtocol(message: string): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#rejectPending(processError("protocol", message));
    this.#child.kill();
  }

  #rejectPending(error: CodexProcessError): void {
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.resolve({ ok: false, error });
    }
    this.#pending.clear();
  }
}

function failure(kind: CodexProcessError["kind"], message: string): CodexProcessResult<never> {
  return { ok: false, error: processError(kind, message) };
}

function processError(kind: CodexProcessError["kind"], message: string): CodexProcessError {
  return { kind, message };
}

function safeMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Unknown process error";
}
