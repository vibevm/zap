/** Narrow Electron bridge: fixed Quicklens operations, no filesystem or shell surface. */
import { contextBridge, ipcRenderer, type IpcRendererEvent } from "electron";

const channels = {
  read: "quicklens:read",
  answerQuestion: "quicklens:answer-question",
  proposePlanIntent: "quicklens:propose-plan-intent",
  previewPlan: "quicklens:preview-plan",
  applyPlan: "quicklens:apply-plan",
  reconcilePlan: "quicklens:reconcile-plan",
  decidePlan: "quicklens:decide-plan",
  invalidation: "quicklens:invalidation",
  workspaceRead: "workspace:read",
  workspaceCommand: "workspace:command",
  workspaceEvents: "workspace:events",
  productRequest: "product:request",
  productChooseDirectory: "product:choose-directory",
};

contextBridge.exposeInMainWorld("quicklensHost", {
  read: () => ipcRenderer.invoke(channels.read),
  answerQuestion: (input: unknown) => ipcRenderer.invoke(channels.answerQuestion, input),
  proposePlanIntent: (input: unknown) => ipcRenderer.invoke(channels.proposePlanIntent, input),
  previewPlan: (input: unknown) => ipcRenderer.invoke(channels.previewPlan, input),
  applyPlan: (input: unknown) => ipcRenderer.invoke(channels.applyPlan, input),
  reconcilePlan: (input: unknown) => ipcRenderer.invoke(channels.reconcilePlan, input),
  decidePlan: (input: unknown) => ipcRenderer.invoke(channels.decidePlan, input),
  subscribe: (listener: (reason: string) => void) => {
    const receive = (_event: IpcRendererEvent, reason: unknown): void => {
      if (
        reason === "events" ||
        reason === "questions" ||
        reason === "plan" ||
        reason === "reconnect"
      ) {
        listener(reason);
      }
    };
    ipcRenderer.on(channels.invalidation, receive);
    return () => {
      ipcRenderer.removeListener(channels.invalidation, receive);
    };
  },
});

const workspaceAvailable = ipcRenderer.sendSync("workspace:available") === true;

if (workspaceAvailable)
  contextBridge.exposeInMainWorld("lensWorkspace", {
    read: (request: unknown) => ipcRenderer.invoke(channels.workspaceRead, request),
    command: (request: unknown) => ipcRenderer.invoke(channels.workspaceCommand, request),
    events: (request: unknown) => ipcRenderer.invoke(channels.workspaceEvents, request),
  });

if (workspaceAvailable)
  contextBridge.exposeInMainWorld("lensProduct", {
    request: (request: unknown) => ipcRenderer.invoke(channels.productRequest, request),
    chooseDirectory: () => ipcRenderer.invoke(channels.productChooseDirectory),
  });
