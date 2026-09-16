/** Secure Electron shell for the built Quicklens browser renderer. */
import { join, resolve } from "node:path";
import { writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

import { app, BrowserWindow, dialog, ipcMain, net, protocol, session } from "electron";

import {
  createElectronGatewayClient,
  createElectronWorkspaceGatewayClient,
  type ElectronGatewayClient,
  type ElectronWorkspaceGatewayClient,
} from "./gateway-client.ts";
import { resolveRendererAsset } from "./security.ts";

const SCHEME = "quicklens";
const HOST = "app";
const CONTENT_SECURITY_POLICY =
  "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' http://127.0.0.1:*; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";
protocol.registerSchemesAsPrivileged([
  {
    scheme: SCHEME,
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      corsEnabled: false,
      allowServiceWorkers: false,
      bypassCSP: false,
    },
  },
]);

const browserRoot = resolve(import.meta.dirname, "../browser");

void app.whenReady().then(async () => {
  protocol.handle(SCHEME, (request) => serveRenderer(request.url));
  session.defaultSession.setPermissionRequestHandler((_contents, _permission, callback) => {
    callback(false);
  });
  const client = gatewayClient();
  const workspace = workspaceGatewayClient();
  registerQuicklensIpc(client);
  registerWorkspaceIpc(workspace);
  const capturePath = process.env["QUICKLENS_CAPTURE_PATH"];
  const window = new BrowserWindow({
    width: 1440,
    height: 940,
    minWidth: 760,
    minHeight: 620,
    backgroundColor: "#101522",
    show: false,
    webPreferences: {
      preload: join(import.meta.dirname, "preload.cjs"),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
      webSecurity: true,
    },
  });
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  const unsubscribeInvalidations = client?.subscribe((reason) => {
    window.webContents.send("quicklens:invalidation", reason);
  });
  window.once("closed", () => {
    unsubscribeInvalidations?.();
  });
  if (capturePath !== undefined) {
    window.webContents.on("console-message", (event) => {
      process.stderr.write(`[quicklens-demo-renderer] ${event.message}\n`);
    });
  }
  window.webContents.on("will-navigate", (event, target) => {
    const url = new URL(target);
    if (url.protocol !== `${SCHEME}:` || url.host !== HOST) event.preventDefault();
  });
  window.once("ready-to-show", () => {
    window.show();
  });
  const demo = process.argv.includes("--demo");
  const workspaceDemo = process.argv.includes("--workspace-demo");
  const theme = process.argv.includes("--theme=dark") ? "dark" : "light";
  const demoQuery = workspaceDemo
    ? `?workspace-demo=1&theme=${theme}`
    : demo
      ? `?demo=1&theme=${theme}`
      : "";
  await window.loadURL(`${SCHEME}://${HOST}/${demoQuery}`);
  if ((demo || workspaceDemo) && capturePath !== undefined) {
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 1_000));
    if (process.argv.includes("--capture-lower")) {
      await window.webContents.executeJavaScript(
        "window.scrollTo({top:document.body.scrollHeight,behavior:'instant'})",
      );
      await new Promise((resolveDelay) => setTimeout(resolveDelay, 300));
    }
    const renderState: unknown = await window.webContents.executeJavaScript(
      "({text:document.body.innerText,html:document.body.innerHTML,ready:document.readyState,url:location.href})",
    );
    process.stderr.write(`[quicklens-demo-state] ${JSON.stringify(renderState).slice(0, 600)}\n`);
    const image = await window.webContents.capturePage();
    await writeFile(capturePath, image.toPNG());
    app.quit();
  }
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});

async function serveRenderer(rawUrl: string): Promise<Response> {
  const resolved = resolveRendererAsset(browserRoot, rawUrl);
  if (!resolved.ok) return new Response("Not found", { status: 404 });
  const response = await net.fetch(pathToFileURL(resolved.file).toString());
  const headers = new Headers(response.headers);
  headers.set("Referrer-Policy", "no-referrer");
  headers.set("X-Content-Type-Options", "nosniff");
  if (resolved.file.endsWith("index.html")) {
    headers.set("Content-Security-Policy", CONTENT_SECURITY_POLICY);
  }
  return new Response(response.body, { status: response.status, headers });
}

function unavailable() {
  return {
    ok: false,
    error: {
      code: "unavailable",
      message: "The Electron data adapter is not connected yet.",
      recovery: "Start the configured lens/ZAP adapter or launch with --demo.",
    },
  };
}

function gatewayClient(): ElectronGatewayClient | null {
  const baseUrl = process.env["QUICKLENS_GATEWAY_URL"];
  const pairingToken = process.env["QUICKLENS_PAIRING_TOKEN"];
  return baseUrl === undefined || pairingToken === undefined
    ? null
    : createElectronGatewayClient({
        baseUrl,
        pairingToken,
        origin: `${SCHEME}://${HOST}`,
        fetcher: (input, init) => net.fetch(input, init),
      });
}

function workspaceGatewayClient(): ElectronWorkspaceGatewayClient | null {
  const baseUrl = process.env["QUICKLENS_WORKSPACE_GATEWAY_URL"];
  const pairingToken = process.env["QUICKLENS_WORKSPACE_PAIRING_TOKEN"];
  return baseUrl === undefined || pairingToken === undefined
    ? null
    : createElectronWorkspaceGatewayClient({
        baseUrl,
        pairingToken,
        origin: `${SCHEME}://${HOST}`,
        fetcher: (input, init) => net.fetch(input, init),
      });
}

function registerQuicklensIpc(client: ElectronGatewayClient | null): void {
  ipcMain.handle("quicklens:read", () => client?.read() ?? unavailable());
  ipcMain.handle(
    "quicklens:answer-question",
    (_event, input: unknown) => client?.answerQuestion(input) ?? unavailable(),
  );
  ipcMain.handle(
    "quicklens:propose-plan-intent",
    (_event, input: unknown) => client?.proposePlanIntent(input) ?? unavailable(),
  );
  ipcMain.handle(
    "quicklens:preview-plan",
    (_event, input: unknown) => client?.previewPlan(input) ?? unavailable(),
  );
  ipcMain.handle(
    "quicklens:apply-plan",
    (_event, input: unknown) => client?.applyPlan(input) ?? unavailable(),
  );
  ipcMain.handle(
    "quicklens:reconcile-plan",
    (_event, input: unknown) => client?.reconcilePlan(input) ?? unavailable(),
  );
  ipcMain.handle(
    "quicklens:decide-plan",
    (_event, input: unknown) => client?.decidePlan(input) ?? unavailable(),
  );
}

function registerWorkspaceIpc(client: ElectronWorkspaceGatewayClient | null): void {
  ipcMain.on("workspace:available", (event) => {
    event.returnValue = client !== null;
  });
  ipcMain.handle("workspace:read", (_event, request: unknown) =>
    client === null ? workspaceUnavailable() : client.read(request),
  );
  ipcMain.handle("workspace:command", (_event, request: unknown) =>
    client === null ? workspaceUnavailable() : client.command(request),
  );
  ipcMain.handle("workspace:events", (_event, request: unknown) =>
    client === null ? workspaceUnavailable() : client.events(request),
  );
  ipcMain.handle("product:request", (_event, request: unknown) =>
    client === null ? workspaceUnavailable() : client.product(request),
  );
  ipcMain.handle("product:choose-directory", async () => {
    const selected = await dialog.showOpenDialog({
      title: "Choose an existing project directory",
      properties: ["openDirectory", "createDirectory"],
    });
    return selected.canceled ? null : (selected.filePaths[0] ?? null);
  });
}

function workspaceUnavailable() {
  return {
    ok: false,
    error: {
      code: "unavailable",
      message:
        "violates REQ spec://org.vibevm.zap/lens/PROP-005#transport: Zap Wayfinder workspace transport is unavailable.",
    },
  };
}
