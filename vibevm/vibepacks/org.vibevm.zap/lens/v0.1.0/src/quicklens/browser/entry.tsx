/** Browser/Electron renderer composition root. */
import { noSerialize, render } from "@qwik.dev/core";

import { createQuicklensDemoDataSource } from "../../cells/quicklens-demo/index.ts";
import { QuicklensRefSchema, unavailableDataSource } from "../../cells/quicklens-model/index.ts";
import { QuicklensApp } from "../../cells/quicklens-ui/app.tsx";
import { createWorkspaceDemoPort, WORKSPACE_DEMO_LABEL } from "../../cells/workspace-demo/index.ts";
import { createWorkspaceHttpClient } from "../../cells/workspace-client/index.ts";
import { WorkspaceApp } from "../../cells/workspace-ui/index.tsx";
import "../../cells/quicklens-ui/styles.css";
import { createBrowserGatewayBridge } from "./gateway-bridge.ts";
import { createHostBridgeDataSource } from "./host-bridge.ts";
import { createQuicklensWebClient } from "./web-gateway.ts";
import { QuicklensWebRoot } from "./web-root.tsx";
import { createWorkspaceIpcClient } from "./workspace-ipc.ts";

const parameters = new URL(window.location.href).searchParams;
const container = document.getElementById("quicklens-root") ?? document.body;
const web = parameters.get("web") === "1" || window.location.protocol === "https:";
const demo = parameters.get("demo") === "1";
const workspaceDemo = parameters.get("workspace-demo") === "1";
const requestedTheme = parameters.get("theme");
if (requestedTheme === "light" || requestedTheme === "dark") {
  window.localStorage.setItem("quicklens.theme", requestedTheme);
}
const workspaceGateway = parameters.get("workspace-gateway");
const workspacePair = new URLSearchParams(window.location.hash.slice(1)).get("workspace-pair");
if (workspacePair !== null) {
  window.history.replaceState({}, "", `${window.location.pathname}${window.location.search}`);
}
const workspaceTransport =
  workspaceGateway === null
    ? undefined
    : createWorkspaceHttpClient({
        baseUrl: workspaceGateway,
        origin: window.location.origin,
        ...(workspacePair === null ? {} : { pairingToken: workspacePair }),
      });
const workspacePort = workspaceDemo
  ? createWorkspaceDemoPort()
  : window.lensWorkspace === undefined
    ? (workspaceTransport ?? undefined)
    : createWorkspaceIpcClient(window.lensWorkspace);
if (workspacePort !== undefined) {
  await render(
    container,
    <WorkspaceApp
      port={noSerialize(workspacePort)}
      demoLabel={workspaceDemo ? WORKSPACE_DEMO_LABEL : undefined}
    />,
  );
} else if (web) {
  const client = createQuicklensWebClient();
  const source = createHostBridgeDataSource(client);
  await render(
    container,
    <QuicklensWebRoot client={noSerialize(client)} source={noSerialize(source)} />,
  );
} else {
  const gatewayBridge = createBrowserGatewayBridge(window.location, window.history);
  const source = demo
    ? createQuicklensDemoDataSource()
    : window.quicklensHost !== undefined
      ? createHostBridgeDataSource(window.quicklensHost)
      : gatewayBridge !== null
        ? createHostBridgeDataSource(gatewayBridge)
        : unavailableDataSource("No Quicklens data source is configured for this browser session.");
  const initialSelectedRef = demo ? QuicklensRefSchema.parse("task.renderer") : undefined;
  await render(
    container,
    <QuicklensApp source={noSerialize(source)} initialSelectedRef={initialSelectedRef} />,
  );
}
