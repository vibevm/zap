/** No-model UI fixture over the real annotation runtime. @scope spec://org.vibevm.zap/lens/PROP-011#shared-implementation */
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { startLocalProductUi } from "../product-app/index.ts";
import { runtimeConfig } from "./annotations.test-support.ts";
import { createWayfinderRuntime } from "./index.ts";

const root = await mkdtemp(join(tmpdir(), "zap-annotations-browser-"));
const projectDirectory = await mkdtemp(join(root, "project-"));
const ui = await startLocalProductUi({
  rendererRoot: resolve("dist/quicklens/browser"),
  port: 4174,
});
if (!ui.ok) throw new Error(ui.error.message);
const base = runtimeConfig(root);
const created = createWayfinderRuntime({
  ...base,
  gateway: { ...base.gateway, allowedOrigins: [ui.value.origin] },
});
if (!created.ok) throw new Error(created.error.message);
const started = await created.value.start();
if (!started.ok) throw new Error(started.error.message);
const ticket = created.value.issuePairingTicket();
if (!ticket.ok) throw new Error(ticket.error.message);
const gateway = `http://${started.value.host}:${String(started.value.port)}${started.value.basePath}`;
const url = `${ui.value.origin}/?workspace-gateway=${encodeURIComponent(gateway)}#workspace-pair=${encodeURIComponent(ticket.value.ticket)}`;
process.stdout.write(`${JSON.stringify({ url, projectDirectory, root })}\n`);

const close = async (): Promise<void> => {
  await created.value.close();
  await ui.value.close();
  process.exit(0);
};
process.once("SIGINT", () => void close());
process.once("SIGTERM", () => void close());
setInterval(() => undefined, 60_000);
