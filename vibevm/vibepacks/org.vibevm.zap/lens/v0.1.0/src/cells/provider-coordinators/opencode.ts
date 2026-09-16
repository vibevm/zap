/** Lens-owned OpenCode server transport. @scope spec://org.vibevm.zap/lens/PROP-010#provider-support */
import { spawn, type ChildProcess } from "node:child_process";
import { randomBytes, randomUUID } from "node:crypto";
import { createServer } from "node:net";
import type {
  CoordinatorResumeInput,
  CoordinatorStartInput,
  HostRequestAnswer,
} from "../agent-runtime/index.ts";
import { resolveProxyEnvironment, type ProxyPolicy } from "../proxy-policy/index.ts";
import type {
  ProviderCoordinatorProfile,
  ProviderCoordinatorSession,
  ProviderCoordinatorTransport,
  ProviderCoordinatorTransportFactory,
  ProviderLaunchPreparationPort,
} from "./index.ts";
import { openCodeHttpTransport } from "./http.ts";

export interface OpenCodeDaemonProcess {
  readonly pid: number;
  onExit(listener: (code: number | null) => void): () => void;
  kill(): void;
}

export interface OpenCodeDaemonFactory {
  spawn(input: {
    readonly executablePath: string;
    readonly args: readonly string[];
    readonly cwd: string;
    readonly environment: Readonly<Record<string, string>>;
  }): OpenCodeDaemonProcess;
}

export function createOpenCodeOwnedTransportFactory(
  options: {
    readonly processFactory?: OpenCodeDaemonFactory;
    readonly fetchImpl?: typeof fetch;
    readonly proxyPolicy?: ProxyPolicy;
    readonly startupTimeoutMs?: number;
    readonly reservePort?: () => Promise<number>;
    readonly prepareLaunch?: ProviderLaunchPreparationPort;
  } = {},
): ProviderCoordinatorTransportFactory {
  return {
    open: (profile) =>
      Promise.resolve({
        ok: true as const,
        value: new OpenCodeOwnedTransport(profile, {
          processFactory: options.processFactory ?? nodeDaemonFactory(),
          fetchImpl: options.fetchImpl ?? fetch,
          startupTimeoutMs: options.startupTimeoutMs ?? 10_000,
          reservePort: options.reservePort ?? reserveLoopbackPort,
          ...(options.proxyPolicy === undefined ? {} : { proxyPolicy: options.proxyPolicy }),
          ...(options.prepareLaunch === undefined ? {} : { prepareLaunch: options.prepareLaunch }),
        }),
      }),
  };
}

class OpenCodeOwnedTransport implements ProviderCoordinatorTransport {
  readonly #profile: ProviderCoordinatorProfile;
  readonly #options: {
    readonly processFactory: OpenCodeDaemonFactory;
    readonly fetchImpl: typeof fetch;
    readonly proxyPolicy?: ProxyPolicy;
    readonly startupTimeoutMs: number;
    readonly reservePort: () => Promise<number>;
    readonly prepareLaunch?: ProviderLaunchPreparationPort;
  };
  readonly #listeners = new Set<(raw: unknown) => void>();
  #daemon: OpenCodeDaemonProcess | undefined;
  #inner: ProviderCoordinatorTransport | undefined;
  #unsubscribeInner: (() => void) | undefined;
  #unsubscribeExit: (() => void) | undefined;

  constructor(
    profile: ProviderCoordinatorProfile,
    options: {
      readonly processFactory: OpenCodeDaemonFactory;
      readonly fetchImpl: typeof fetch;
      readonly proxyPolicy?: ProxyPolicy;
      readonly startupTimeoutMs: number;
      readonly reservePort: () => Promise<number>;
      readonly prepareLaunch?: ProviderLaunchPreparationPort;
    },
  ) {
    this.#profile = profile;
    this.#options = options;
  }

  async start(input: { readonly scope: CoordinatorStartInput }) {
    const ready = await this.#ensureDaemon(input.scope);
    if (!ready.ok) return ready;
    if (this.#inner === undefined) return unavailable();
    return this.#inner.start(input);
  }

  async resume(input: { readonly scope: CoordinatorResumeInput }) {
    const ready = await this.#ensureDaemon(input.scope);
    if (!ready.ok) return ready;
    if (this.#inner === undefined) return unavailable();
    return this.#inner.resume(input);
  }

  history(session: ProviderCoordinatorSession) {
    if (this.#inner === undefined) return Promise.resolve(unavailable());
    return this.#inner.history(session);
  }

  send(input: {
    readonly session: ProviderCoordinatorSession;
    readonly text: string;
    readonly clientMessageId: string;
  }) {
    if (this.#inner === undefined) return Promise.resolve(unavailable());
    return this.#inner.send(input);
  }

  interrupt(input: {
    readonly session: ProviderCoordinatorSession;
    readonly nativeTurnId: string;
  }) {
    if (this.#inner === undefined) return Promise.resolve(unavailable());
    return this.#inner.interrupt(input);
  }

  respond(input: {
    readonly session: ProviderCoordinatorSession;
    readonly answer: HostRequestAnswer;
  }) {
    if (this.#inner === undefined) return Promise.resolve(unavailable());
    return this.#inner.respond(input);
  }

  pause(input: { readonly session: ProviderCoordinatorSession }) {
    if (this.#inner === undefined) return Promise.resolve(unavailable());
    return this.#inner.pause(input);
  }

  async stop(input: { readonly session: ProviderCoordinatorSession }) {
    const stopped = this.#inner === undefined ? unavailable() : await this.#inner.stop(input);
    this.#inner?.close();
    this.#inner = undefined;
    this.#daemon?.kill();
    return stopped;
  }

  subscribe(listener: (raw: unknown) => void) {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  close() {
    this.#inner?.close();
    this.#inner = undefined;
    this.#disposeDaemon();
    this.#listeners.clear();
  }

  async #ensureDaemon(scope: CoordinatorStartInput | CoordinatorResumeInput) {
    if (this.#inner !== undefined) return { ok: true as const, value: null };
    const prepared = await this.#options.prepareLaunch?.prepare({
      profile: this.#profile,
      scope,
    });
    if (prepared !== undefined && !prepared.ok) return prepared;
    const port = await this.#options.reservePort();
    const username = "zap";
    const password = randomBytes(32).toString("base64url");
    const proxy = resolveProxyEnvironment({
      ambient: { ...process.env, ...(prepared?.value.environment ?? {}) },
      ...(this.#options.proxyPolicy === undefined ? {} : { global: this.#options.proxyPolicy }),
      ...(this.#profile.proxy === undefined ? {} : { profile: this.#profile.proxy }),
    }).environment;
    const environment = Object.fromEntries(
      Object.entries(proxy).filter((entry): entry is [string, string] => entry[1] !== undefined),
    );
    const daemon = this.#options.processFactory.spawn({
      executablePath: this.#profile.executablePath,
      args: [
        ...(this.#profile.argumentPrefix ?? []),
        "serve",
        "--pure",
        "--hostname",
        "127.0.0.1",
        "--port",
        String(port),
      ],
      cwd: scope.cwd,
      environment: {
        ...environment,
        OPENCODE_SERVER_USERNAME: username,
        OPENCODE_SERVER_PASSWORD: password,
        ...((prepared?.value.mcpConfigPath ?? this.#profile.mcpConfigPath) === undefined ||
        (prepared?.value.mcpConfigPath ?? this.#profile.mcpConfigPath) === null
          ? {}
          : {
              OPENCODE_CONFIG: prepared?.value.mcpConfigPath ?? this.#profile.mcpConfigPath ?? "",
            }),
      },
    });
    this.#daemon = daemon;
    this.#unsubscribeExit = daemon.onExit(() => {
      this.#daemon = undefined;
      this.#unsubscribeExit?.();
      this.#unsubscribeExit = undefined;
      this.#inner?.close();
      this.#inner = undefined;
      for (const listener of this.#listeners) listener({ type: "process_exited", exitCode: null });
    });
    const endpoint = `http://127.0.0.1:${String(port)}`;
    const authorized = `Basic ${Buffer.from(`${username}:${password}`).toString("base64")}`;
    const deadline = Date.now() + this.#options.startupTimeoutMs;
    while (Date.now() < deadline) {
      try {
        const response = await this.#options.fetchImpl(`${endpoint}/doc`, {
          headers: { authorization: authorized },
        });
        if (response.ok) {
          this.#inner = openCodeHttpTransport(
            { ...this.#profile, endpoint },
            { fetchImpl: this.#options.fetchImpl, basicAuth: { username, password } },
            `opencode:${String(daemon.pid)}:${randomUUID()}`,
          );
          this.#unsubscribeInner = this.#inner.subscribe((raw) => {
            for (const listener of this.#listeners) listener(raw);
          });
          return { ok: true as const, value: null };
        }
      } catch {
        // The owned loopback server has not bound yet.
      }
      await delay(50);
    }
    this.#disposeDaemon();
    return failure("transport_lost", "owned OpenCode server did not become ready");
  }

  #disposeDaemon(): void {
    this.#unsubscribeInner?.();
    this.#unsubscribeInner = undefined;
    this.#unsubscribeExit?.();
    this.#unsubscribeExit = undefined;
    this.#daemon?.kill();
    this.#daemon = undefined;
  }
}

function reserveLoopbackPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = typeof address === "object" && address !== null ? address.port : 0;
      server.close((error) => {
        if (error === undefined) resolve(port);
        else reject(error);
      });
    });
  });
}

function nodeDaemonFactory(): OpenCodeDaemonFactory {
  return {
    spawn(input) {
      const child = spawn(input.executablePath, [...input.args], {
        cwd: input.cwd,
        env: input.environment,
        stdio: "ignore",
        windowsHide: true,
      });
      return ownedDaemon(child);
    },
  };
}

function ownedDaemon(child: ChildProcess): OpenCodeDaemonProcess {
  const listeners = new Set<(code: number | null) => void>();
  child.on("exit", (code) => {
    for (const listener of listeners) listener(code);
  });
  return {
    pid: child.pid ?? 0,
    onExit(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    kill: () => void child.kill(),
  };
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function unavailable() {
  return failure("transport_lost", "owned OpenCode server is unavailable");
}

function failure(code: "transport_lost", message: string) {
  return { ok: false as const, error: { code, message, retry: "after_reconcile" as const } };
}
