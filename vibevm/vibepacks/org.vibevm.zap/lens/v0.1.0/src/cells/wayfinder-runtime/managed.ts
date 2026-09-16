/** Optional local managed-worker composition. @scope spec://org.vibevm.zap/lens/PROP-006#managed-terminal */
import { isAbsolute, resolve } from "node:path";
import { z } from "zod";
import {
  ManagedTerminalKernel,
  createOptionalNodePtyFactory,
  type ManagedTerminalResult,
  type ManagedTerminalSnapshot,
} from "../managed-terminal/index.ts";
import {
  createManagedTerminalService,
  type ManagedTerminalServicePort,
} from "../managed-terminal-service/index.ts";
import { openManagedTerminalOutputStore } from "../managed-terminal-store/index.ts";
import type { WorkspaceAccessContext } from "../workspace-model/index.ts";

const AbsolutePathSchema = z.string().min(1).max(32_000).refine(isAbsolute);

export const ManagedRuntimeProfileSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    projectId: z.string().min(3).max(160),
    contextId: z.string().min(3).max(160),
    executable: AbsolutePathSchema,
    args: z.array(z.string().max(16_384)).max(256),
    cwd: AbsolutePathSchema,
    env: z.record(z.string(), z.string()).optional(),
    label: z.string().min(1).max(256),
  })
  .strict();
export type ManagedRuntimeProfile = z.infer<typeof ManagedRuntimeProfileSchema>;

export const ManagedRuntimeConfigSchema = z
  .object({
    enabled: z.boolean(),
    databasePath: AbsolutePathSchema,
    profiles: z.array(ManagedRuntimeProfileSchema).max(256),
    outputHistoryLimit: z.number().int().min(1).max(100_000).default(2_000),
  })
  .strict()
  .superRefine((config, context) => {
    const ids = new Set<string>();
    for (const [index, profile] of config.profiles.entries()) {
      if (ids.has(profile.profileId)) {
        context.addIssue({
          code: "custom",
          path: ["profiles", index, "profileId"],
          message: "managed profile ids must be unique",
        });
      }
      ids.add(profile.profileId);
    }
  });
export type ManagedRuntimeConfig = z.infer<typeof ManagedRuntimeConfigSchema>;

export const ManagedWorkerLaunchRequestSchema = z
  .object({
    profileId: z.string().min(3).max(160),
    projectId: z.string().min(3).max(160),
    contextId: z.string().min(3).max(160),
    terminalId: z.string().min(3).max(160),
    sessionId: z.string().min(3).max(160),
    runId: z.string().min(3).max(160),
  })
  .strict();
export type ManagedWorkerLaunchRequest = z.infer<typeof ManagedWorkerLaunchRequestSchema>;

export interface ConfiguredManagedTerminalService extends ManagedTerminalServicePort {
  readonly profiles: readonly ManagedRuntimeProfile[];
  startRegistered(
    access: WorkspaceAccessContext,
    request: ManagedWorkerLaunchRequest,
  ): Promise<ManagedTerminalResult<ManagedTerminalSnapshot>>;
}

export type ManagedRuntimeOpenResult =
  | {
      readonly ok: true;
      readonly value: { readonly service: ConfiguredManagedTerminalService | undefined };
    }
  | {
      readonly ok: false;
      readonly error: { readonly code: "invalid_input" | "unavailable"; readonly message: string };
    };

export interface ManagedRuntimeController {
  readonly service: ConfiguredManagedTerminalService | undefined;
  start(): Promise<ManagedRuntimeOpenResult>;
  close(): void;
}

export function createManagedRuntimeController(raw: unknown):
  | { readonly ok: true; readonly value: ManagedRuntimeController }
  | {
      readonly ok: false;
      readonly error: { readonly code: "invalid_input"; readonly message: string };
    } {
  const parsed = ManagedRuntimeConfigSchema.safeParse(raw);
  if (!parsed.success) {
    return {
      ok: false,
      error: { code: "invalid_input", message: "managed runtime config is invalid" },
    };
  }
  if (!parsed.data.enabled) {
    return {
      ok: true,
      value: {
        service: undefined,
        start: () => Promise.resolve({ ok: true, value: { service: undefined } }),
        close: () => undefined,
      },
    };
  }
  const proxy = new ManagedTerminalProxy(parsed.data.profiles);
  return {
    ok: true,
    value: {
      service: proxy,
      async start() {
        if (proxy.attached) return { ok: true, value: { service: proxy } };
        const opened = await openService(parsed.data);
        if (!opened.ok) return opened;
        proxy.attach(opened.value);
        return { ok: true, value: { service: proxy } };
      },
      close() {
        proxy.close();
      },
    },
  };
}

export async function openConfiguredManagedRuntime(
  raw: unknown,
): Promise<ManagedRuntimeOpenResult> {
  const controller = createManagedRuntimeController(raw);
  return controller.ok ? controller.value.start() : controller;
}

async function openService(config: ManagedRuntimeConfig): Promise<
  | { readonly ok: true; readonly value: ConfiguredManagedTerminalService }
  | {
      readonly ok: false;
      readonly error: { readonly code: "unavailable"; readonly message: string };
    }
> {
  const factory = await createOptionalNodePtyFactory(true);
  if (!factory.ok) {
    return { ok: false, error: { code: "unavailable", message: factory.error.message } };
  }
  const outputStore = openManagedTerminalOutputStore(
    resolve(config.databasePath),
    config.outputHistoryLimit,
  );
  const base = createManagedTerminalService(
    new ManagedTerminalKernel(factory.value, config.outputHistoryLimit),
    [],
    outputStore,
  );
  const profiles = [...config.profiles];
  const byId = new Map(profiles.map((profile) => [profile.profileId, profile]));
  const service: ConfiguredManagedTerminalService = {
    ...base,
    profiles,
    async startRegistered(access, rawRequest) {
      const request = ManagedWorkerLaunchRequestSchema.safeParse(rawRequest);
      if (!request.success) return terminalFailure("invalid_input", "managed launch is invalid");
      const profile = byId.get(request.data.profileId);
      if (
        profile === undefined ||
        profile.projectId !== request.data.projectId ||
        profile.contextId !== request.data.contextId ||
        !access.authorizedProjectIds.some((authorized) => authorized === request.data.projectId)
      ) {
        return terminalFailure("forbidden", "managed profile is outside project context scope");
      }
      return base.start({
        accessProjectId: profile.projectId,
        accessContextId: profile.contextId,
        spec: {
          terminalId: request.data.terminalId,
          projectId: profile.projectId,
          contextId: profile.contextId,
          sessionId: request.data.sessionId,
          runId: request.data.runId,
          executable: profile.executable,
          args: profile.args,
          cwd: profile.cwd,
          ...(profile.env === undefined ? {} : { env: profile.env }),
        },
      });
    },
  };
  return { ok: true, value: service };
}

class ManagedTerminalProxy implements ConfiguredManagedTerminalService {
  readonly profiles: readonly ManagedRuntimeProfile[];
  #delegate: ConfiguredManagedTerminalService | undefined;

  constructor(profiles: readonly ManagedRuntimeProfile[]) {
    this.profiles = [...profiles];
  }

  get attached(): boolean {
    return this.#delegate !== undefined;
  }

  attach(delegate: ConfiguredManagedTerminalService): void {
    this.#delegate = delegate;
  }

  startRegistered(access: WorkspaceAccessContext, request: ManagedWorkerLaunchRequest) {
    const delegate = this.#delegate;
    return delegate === undefined
      ? Promise.resolve(unavailable())
      : delegate.startRegistered(access, request);
  }

  start(registration: Parameters<ManagedTerminalServicePort["start"]>[0]) {
    const delegate = this.#delegate;
    return delegate === undefined ? Promise.resolve(unavailable()) : delegate.start(registration);
  }

  snapshot(access: WorkspaceAccessContext, terminalId: string) {
    return this.#delegate?.snapshot(access, terminalId) ?? unavailable();
  }

  list(access: WorkspaceAccessContext, projectId: string, contextId: string) {
    return this.#delegate?.list(access, projectId, contextId) ?? unavailable();
  }

  read(access: WorkspaceAccessContext, request: Parameters<ManagedTerminalServicePort["read"]>[1]) {
    return this.#delegate?.read(access, request) ?? unavailable();
  }

  acquire(
    access: WorkspaceAccessContext,
    request: Parameters<ManagedTerminalServicePort["acquire"]>[1],
  ) {
    return this.#delegate?.acquire(access, request) ?? unavailable();
  }

  release(access: WorkspaceAccessContext, terminalId: string, leaseId: string, epoch: number) {
    return this.#delegate?.release(access, terminalId, leaseId, epoch) ?? unavailable();
  }

  input(
    access: WorkspaceAccessContext,
    terminalId: string,
    leaseId: string,
    epoch: number,
    data: string,
  ) {
    return this.#delegate?.input(access, terminalId, leaseId, epoch, data) ?? unavailable();
  }

  resize(
    access: WorkspaceAccessContext,
    terminalId: string,
    leaseId: string,
    epoch: number,
    columns: number,
    rows: number,
  ) {
    return (
      this.#delegate?.resize(access, terminalId, leaseId, epoch, columns, rows) ?? unavailable()
    );
  }

  interrupt(access: WorkspaceAccessContext, terminalId: string, leaseId: string, epoch: number) {
    return this.#delegate?.interrupt(access, terminalId, leaseId, epoch) ?? unavailable();
  }

  stop(access: WorkspaceAccessContext, terminalId: string, leaseId: string, epoch: number) {
    return this.#delegate?.stop(access, terminalId, leaseId, epoch) ?? unavailable();
  }

  stopProject(access: WorkspaceAccessContext, projectId: string, contextId: string) {
    const delegate = this.#delegate;
    return delegate === undefined
      ? Promise.resolve(unavailable())
      : delegate.stopProject(access, projectId, contextId);
  }

  close(): void {
    this.#delegate?.close();
    this.#delegate = undefined;
  }
}

function unavailable(): ManagedTerminalResult<never> {
  return { ok: false, error: { code: "unavailable", message: "managed runtime is not started" } };
}

function terminalFailure(
  code: "invalid_input" | "forbidden",
  message: string,
): ManagedTerminalResult<never> {
  return { ok: false, error: { code, message } };
}
