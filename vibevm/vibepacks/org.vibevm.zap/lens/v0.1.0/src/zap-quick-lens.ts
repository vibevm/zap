#!/usr/bin/env node
/** Ordinary Zap Quick Lens launcher. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { z } from "zod";
import { CredentialSchema } from "./cells/protocol/index.ts";
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

const arguments_ = process.argv.slice(2);
const advancedPath = option("--config");
const stateDirectory = resolve(option("--state-dir") ?? join(homedir(), ".vibe", "zap"));
const presentation = arguments_.includes("--electron") ? "electron" : "browser";
const shouldOpen = !arguments_.includes("--no-open");
await main();

async function main(): Promise<void> {
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
    emit({ url, reusedOwner: true, databasePath, presentation });
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
  );
}

async function startNewOwner(
  database: string,
  stateRoot: string,
  configured: WayfinderRuntimeConfig | undefined,
  settings: ProductLocalSettings,
  client: "browser" | "electron",
  open: boolean,
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
  emit({ url, reusedOwner: false, databasePath: database, presentation: client, stateRoot });
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
  const providers = await discoverLocalProductProviders(settings);
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
    profiles: [...providers.coordinatorProfiles],
    providerCoordinatorProfiles: [...providers.providerCoordinatorProfiles],
    productProviders: [...providers.productProviders],
    managedWorkerProfiles: [...providers.managedWorkers],
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
  };
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

function option(name: string): string | undefined {
  const direct = arguments_.find((argument) => argument.startsWith(`${name}=`));
  if (direct !== undefined) return direct.slice(name.length + 1);
  const index = arguments_.indexOf(name);
  return index < 0 ? undefined : arguments_[index + 1];
}

function emit(value: unknown): void {
  console.log(JSON.stringify({ protocol: "zap-quick-lens/1", ...object(value) }));
}

function object(value: unknown): Record<string, unknown> {
  return z.record(z.string(), z.unknown()).parse(value);
}

function fail(message: string, exitCode: number): void {
  console.error(message);
  process.exitCode = exitCode;
}
