#!/usr/bin/env node
/** Ordinary Zap Quick Lens launcher. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { z } from "zod";
import { CredentialSchema } from "./cells/protocol/index.ts";
import { ExecutionHostIdSchema } from "./cells/workspace-model/index.ts";
import { createNativeManagedProviderControlAdapters } from "./cells/managed-work/index.ts";
import {
  discoverLocalProductProviders,
  createProtectedEnvironmentResolver,
  loadProductLocalSettings,
  startLocalProductUi,
  type LocalProductUi,
  type ProductLocalSettings,
} from "./cells/product-app/index.ts";
import {
  acquireWayfinderOwner,
  createWayfinderRuntime,
  loadWayfinderConfig,
  requestRunningOwnerTicket,
  type OwnerResult,
  type WayfinderOwnerLease,
  type WayfinderRuntime,
  type WayfinderRuntimeConfig,
} from "./cells/wayfinder-runtime/index.ts";

export interface ZapProductLauncherInput {
  readonly args?: readonly string[];
  readonly commandName?: "zap-quicklens" | "zap-server";
  readonly forceNoOpen?: boolean;
  readonly serverMode?: boolean;
}

export async function runZapProductLauncher(input: ZapProductLauncherInput = {}): Promise<void> {
  const args = [...(input.args ?? process.argv.slice(2))];
  const mode = {
    commandName: input.commandName ?? ("zap-quicklens" as const),
    forceNoOpen: input.forceNoOpen ?? false,
    serverMode: input.serverMode ?? false,
  };
  if (args.includes("--help") || args.includes("-h")) {
    printHelp(mode);
    return;
  }
  if (
    mode.serverMode &&
    args.some((argument) => argument === "--electron" || argument.startsWith("--electron="))
  ) {
    fail("zap-server does not open a presentation; use zap-quicklens --electron", 2);
    return;
  }
  await main(args, mode);
}

async function main(
  args: readonly string[],
  mode: Required<Pick<ZapProductLauncherInput, "commandName" | "forceNoOpen" | "serverMode">>,
): Promise<void> {
  const advancedPath = option(args, "--config");
  const stateDirectory = resolve(option(args, "--state-dir") ?? join(homedir(), ".vibe", "zap"));
  const presentation = args.includes("--electron") ? "electron" : "browser";
  const shouldOpen = !mode.forceNoOpen && !args.includes("--no-open");
  const settings = await loadProductLocalSettings(join(stateDirectory, "settings.json"));
  if (!settings.ok) {
    fail(settings.message, 2);
    return;
  }
  const uiOrigin = `http://127.0.0.1:${String(settings.value.uiPort)}`;
  const advanced = advancedPath === undefined ? null : await loadWayfinderConfig(advancedPath);
  if (advanced !== null && !advanced.ok) {
    fail(advanced.error.message, 2);
    return;
  }
  const config = advanced?.ok === true ? advanced.value : undefined;
  const databasePath = resolve(
    config?.state.databasePath ?? join(stateDirectory, "workspace.sqlite"),
  );
  const existing = await requestRunningOwnerTicket(databasePath);
  if (existing.ok) {
    const url = attachUrl(existing.value.gateway, existing.value.ticket, uiOrigin);
    emit(mode, receipt(mode, { url, reusedOwner: true, databasePath, presentation }));
    if (shouldOpen)
      openPresentation(presentation, url, existing.value.gateway, existing.value.ticket);
    return;
  }
  if (existing.error.code !== "not_found") {
    fail(existing.error.message, 1);
    return;
  }
  await startNewOwner(
    databasePath,
    stateDirectory,
    config,
    settings.value,
    presentation,
    shouldOpen,
    mode,
  );
}

function printHelp(
  mode: Required<Pick<ZapProductLauncherInput, "commandName" | "forceNoOpen" | "serverMode">>,
): void {
  if (mode.serverMode) {
    console.log(
      [
        "Zap Server",
        "",
        "Usage: zap-server [options]",
        "",
        "Starts the normal Wayfinder and Quick Lens HTTP stack without opening a viewer.",
        "It does not start a coordinator, worker, child agent, or model turn.",
        "",
        "Options:",
        "  --state-dir <path>  Store settings and workspace state under this directory",
        "  --config <path>     Load an advanced Wayfinder runtime configuration",
        "  -h, --help          Show this help and exit",
      ].join("\n"),
    );
    return;
  }
  console.log(
    [
      "Zap Quick Lens",
      "",
      "Usage: zap-quicklens [options]",
      "Legacy alias: zap-quick-lens",
      "",
      "Options:",
      "  --state-dir <path>  Store settings and workspace state under this directory",
      "  --config <path>     Load an advanced Wayfinder runtime configuration",
      "  --electron          Open the Electron client instead of the browser client",
      "  --no-open           Start or attach without opening a client window",
      "  -h, --help          Show this help and exit",
    ].join("\n"),
  );
}

async function startNewOwner(
  database: string,
  stateRoot: string,
  configured: WayfinderRuntimeConfig | undefined,
  settings: ProductLocalSettings,
  client: "browser" | "electron",
  open: boolean,
  mode: Required<Pick<ZapProductLauncherInput, "commandName" | "forceNoOpen" | "serverMode">>,
): Promise<void> {
  const owner = acquireWayfinderOwner(database);
  if (!owner.ok) {
    fail(owner.error.message, 1);
    return;
  }
  const ui = await startLocalProductUi({
    rendererRoot: rendererRoot(),
    port: settings.uiPort,
  });
  if (!ui.ok) {
    await owner.value.close();
    fail(ui.error.message, 1);
    return;
  }
  const config =
    configured === undefined
      ? await defaultConfig(database, ui.value.origin, settings)
      : advancedConfig(configured, ui.value.origin, settings);
  const created = createWayfinderRuntime(config, {
    managedEnvironment: createProtectedEnvironmentResolver(settings.environmentFiles),
    managedControlAdapters: createNativeManagedProviderControlAdapters({
      directory: join(stateRoot, "managed-control"),
    }),
  });
  if (!created.ok) {
    await closeOwner(undefined, ui.value, owner.value);
    fail(created.error.message, 2);
    return;
  }
  const started = await created.value.start();
  if (!started.ok) {
    await closeOwner(created.value, ui.value, owner.value);
    fail(started.error.message, 1);
    return;
  }
  const gateway = {
    host: started.value.host,
    port: started.value.port,
    basePath: started.value.basePath,
  };
  const published = await owner.value.publish(gateway, () => ownerTicket(created.value));
  if (!published.ok) {
    await closeOwner(created.value, ui.value, owner.value);
    fail(published.error.message, 1);
    return;
  }
  const ticket = created.value.issuePairingTicket();
  if (!ticket.ok) {
    await closeOwner(created.value, ui.value, owner.value);
    fail(ticket.error.message, 1);
    return;
  }
  const url = attachUrl(gateway, ticket.value.ticket, ui.value.origin);
  emit(
    mode,
    receipt(mode, {
      url,
      reusedOwner: false,
      databasePath: database,
      presentation: client,
      stateRoot,
    }),
  );
  if (open) openPresentation(client, url, gateway, ticket.value.ticket);
  const close = async (): Promise<void> => {
    await closeOwner(created.value, ui.value, owner.value);
    process.exit(0);
  };
  process.once("SIGINT", () => void close());
  process.once("SIGTERM", () => void close());
  setInterval(() => undefined, 60_000);
}

async function defaultConfig(
  databasePath: string,
  origin: string,
  settings: ProductLocalSettings,
): Promise<WayfinderRuntimeConfig> {
  const providers = await discoverLocalProductProviders(settings, {
    hostId: "host.execution.local",
  });
  return {
    version: 1,
    state: { databasePath },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "zapquicklens",
      pairingToken: randomBytes(32).toString("base64url"),
      allowedHosts: ["127.0.0.1", "localhost"],
      allowedOrigins: [origin, "quicklens://app"],
    },
    agentGateway: {
      databasePath: `${databasePath}.agent`,
      host: "127.0.0.1",
      port: 0,
      allowedHosts: ["127.0.0.1", "localhost"],
      allowedOrigins: [],
      statusToken: CredentialSchema.parse(randomBytes(32).toString("base64url")),
      scopes: [],
    },
    managedTerminals: {
      enabled: true,
      databasePath: `${databasePath}.terminals`,
      profiles: [],
      outputHistoryLimit: 2_000,
    },
    managedAgents: [],
    executionHostId: ExecutionHostIdSchema.parse("host.execution.local"),
    profiles: [...providers.coordinatorProfiles],
    providerCoordinatorProfiles: [...providers.providerCoordinatorProfiles],
    productProviders: [...providers.productProviders],
    managedWorkerProfiles: [...providers.managedWorkers],
    executionBindings: mergeBindings(providers.executionBindings, settings.executionBindings),
    executionLaunchTemplates: [...providers.launchTemplates],
    repositoryWorkspaces: repositoryConfig(settings),
    proxy: settings.proxy,
    projects: [],
    modelPolicies: [],
  };
}

function advancedConfig(
  loaded: WayfinderRuntimeConfig,
  origin: string,
  settings: ProductLocalSettings,
): WayfinderRuntimeConfig {
  return {
    ...loaded,
    gateway: {
      ...loaded.gateway,
      pairingToken: randomBytes(32).toString("base64url"),
      allowedOrigins: [...new Set([...loaded.gateway.allowedOrigins, origin, "quicklens://app"])],
    },
    proxy: settings.proxy,
    executionBindings: mergeBindings(loaded.executionBindings, settings.executionBindings),
    repositoryWorkspaces: loaded.repositoryWorkspaces ?? repositoryConfig(settings),
  };
}

function repositoryConfig(
  settings: ProductLocalSettings,
): NonNullable<WayfinderRuntimeConfig["repositoryWorkspaces"]> {
  return {
    executionHostId: "host.repository.local",
    mergeIdentity: settings.repositoryMergeIdentity,
    testProfiles: ["repository.consistency"],
  };
}

function mergeBindings(
  discovered: readonly ProductLocalSettings["executionBindings"][number][],
  configured: readonly ProductLocalSettings["executionBindings"][number][],
): ProductLocalSettings["executionBindings"] {
  const merged = new Map(discovered.map((binding) => [binding.bindingId, binding]));
  for (const binding of configured) merged.set(binding.bindingId, binding);
  return [...merged.values()].sort((left, right) => left.bindingId.localeCompare(right.bindingId));
}

function rendererRoot(): string {
  const installed = resolve(import.meta.dirname, "quicklens/browser");
  return existsSync(join(installed, "index.html"))
    ? installed
    : resolve(import.meta.dirname, "../dist/quicklens/browser");
}

function ownerTicket(
  runtime: WayfinderRuntime,
): OwnerResult<{ readonly ticket: string; readonly expiresAt: string }> {
  const ticket = runtime.issuePairingTicket();
  return ticket.ok
    ? ticket
    : { ok: false, error: { code: "unavailable", message: ticket.error.message } };
}

function attachUrl(
  gateway: { readonly host: string; readonly port: number; readonly basePath: string },
  ticket: string,
  origin: string,
): string {
  const base = `http://${gateway.host}:${String(gateway.port)}${gateway.basePath}`;
  return `${origin}/?workspace-gateway=${encodeURIComponent(base)}#workspace-pair=${encodeURIComponent(ticket)}`;
}

function openPresentation(
  client: "browser" | "electron",
  url: string,
  gateway: { readonly host: string; readonly port: number; readonly basePath: string },
  ticket: string,
): void {
  if (client === "browser") {
    const opened =
      process.platform === "win32"
        ? spawn("rundll32.exe", ["url.dll,FileProtocolHandler", url], detached())
        : spawn(process.platform === "darwin" ? "open" : "xdg-open", [url], detached());
    opened.unref();
    return;
  }
  const require = createRequire(import.meta.url);
  const moduleValue: unknown = require("electron");
  const executable = z.string().safeParse(moduleValue);
  if (!executable.success) {
    fail("Electron executable is unavailable", 1);
    return;
  }
  const child = spawn(
    executable.data,
    [resolve(import.meta.dirname, "quicklens/electron/main.js")],
    {
      ...detached(),
      env: {
        ...process.env,
        QUICKLENS_WORKSPACE_GATEWAY_URL: `http://${gateway.host}:${String(gateway.port)}${gateway.basePath}`,
        QUICKLENS_WORKSPACE_PAIRING_TOKEN: ticket,
      },
    },
  );
  child.unref();
}

function detached(): {
  readonly detached: true;
  readonly stdio: "ignore";
  readonly windowsHide: false;
} {
  return { detached: true, stdio: "ignore", windowsHide: false };
}

async function closeOwner(
  runtime: WayfinderRuntime | undefined,
  ui: LocalProductUi,
  owner: WayfinderOwnerLease,
): Promise<void> {
  await runtime?.close();
  await ui.close();
  await owner.close();
}

function option(args: readonly string[], name: string): string | undefined {
  const direct = args.find((argument) => argument.startsWith(`${name}=`));
  if (direct !== undefined) return direct.slice(name.length + 1);
  const index = args.indexOf(name);
  return index < 0 ? undefined : args[index + 1];
}

function receipt(
  mode: Required<Pick<ZapProductLauncherInput, "commandName" | "forceNoOpen" | "serverMode">>,
  value: Record<string, unknown>,
): Record<string, unknown> {
  return mode.serverMode
    ? { ...value, presentation: undefined, headless: true, viewerOpened: false }
    : value;
}

function emit(
  mode: Required<Pick<ZapProductLauncherInput, "commandName" | "forceNoOpen" | "serverMode">>,
  value: unknown,
): void {
  console.log(
    JSON.stringify({
      protocol: mode.serverMode ? "zap-server/1" : "zap-quick-lens/1",
      ...object(value),
    }),
  );
}

function object(value: unknown): Record<string, unknown> {
  return z.record(z.string(), z.unknown()).parse(value);
}

function fail(message: string, exitCode: number): void {
  console.error(message);
  process.exitCode = exitCode;
}

if (
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
)
  await runZapProductLauncher();
