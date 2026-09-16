/** Shared multi-project Lens workspace renderer. @scope spec://org.vibevm.zap/lens/PROP-005#shared-code */
import { $, component$, useSignal, useStore, useVisibleTask$ } from "@qwik.dev/core";
import {
  initialHistoryCursor,
  listWorkspaceProjects,
  readAgentOutput,
  readProjectWorkspace,
  readWorkspaceChat,
  readWorkspaceHistory,
  workspaceRequestId,
  type ProjectWorkspaceView,
} from "../workspace-client/index.ts";
import {
  WorkContextIdSchema,
  type ProjectId,
  type WorkContextId,
} from "../workspace-model/index.ts";
import type * as WorkspaceModel from "../workspace-model/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import { ActivityPanel, type ActivityScope } from "./activity-panel.tsx";
import { AgentPanel } from "./agent-panel.tsx";
import { ChatPanel } from "./chat-panel.tsx";
import { CoordinatorControls } from "./coordinator-controls.tsx";
import { ProjectRail } from "./project-navigation.tsx";
import { ProductSetup } from "./product-setup.tsx";
import { ExecutionSettingsPanel } from "./execution-settings-panel.tsx";
import { ManagedTerminalWorkspace } from "./managed-terminal-workspace.tsx";
import { ManagedWorkPanel } from "./managed-work-panel.tsx";
import { WorkspaceHeader } from "./workspace-header.tsx";
import type { WorkspaceAppProps } from "./workspace-app.tsx";
import { WorkspacePortfolio } from "./workspace-portfolio.tsx";
import { WorkspaceQuestions } from "./workspace-questions.tsx";
import { WorkspacePlan } from "./workspace-plan.tsx";
import { RepositoryWorkspacePanel } from "./repository-workspace-panel.tsx";
import { PlanUnavailable, WorkspaceTabs, type WorkspaceTab } from "./workspace-tabs.tsx";
import { createProjectEventRefresh } from "./workspace-event-refresh.ts";
import {
  agentNames,
  eventMatchesScope,
  historyCoverageLabel,
  historyScope,
  observeEvents,
  projectNames,
  readProjectBoardViews,
  preservesInspection,
  selectedAgent,
  uniqueEvents,
} from "./workspace-helpers.ts";
import "./styles.css";
export const WorkspaceApp = component$<WorkspaceAppProps>((props) => {
  const projects = useSignal<readonly WorkspaceModel.ProjectDescriptor[]>([]);
  const boardViews = useStore<Record<string, ProjectWorkspaceView | undefined>>({});
  const selectedProjectId = useSignal<ProjectId | null>(null);
  const selectedContextId = useSignal<WorkContextId | null>(null);
  const projectView = useSignal<ProjectWorkspaceView | null>(null);
  const selectedActorId = useSignal<WorkspaceModel.AgentDescriptor["actorId"] | null>(null);
  const agentOutput = useSignal<readonly WorkspaceModel.AgentOutputItem[]>([]);
  const agentOutputLoading = useSignal(false);
  const agentOutputError = useSignal<string | null>(null);
  const chat = useSignal<readonly WorkspaceModel.ChatMessage[]>([]);
  const chatLoading = useSignal(false);
  const chatError = useSignal<string | null>(null);
  const activity = useSignal<readonly WorkspaceModel.HistoryEvent[]>([]);
  const activityCursor = useSignal<WorkspaceModel.HistoryCursor | null>(null);
  const activityCoverage = useSignal<string | null>(null);
  const activityLoading = useSignal(false);
  const activityError = useSignal<string | null>(null);
  const activityScope = useSignal<ActivityScope>("all");
  const projectLoading = useSignal(false),
    globalError = useSignal<string | null>(null);
  const tab = useSignal<WorkspaceTab>("agents");
  const theme = useSignal<"light" | "dark">("light");
  const coordinatorMessage = useSignal<string | null>(null);
  const coordinatorStarting = useSignal(false);
  const focusEpoch = useSignal(0),
    canvasRefreshEpoch = useSignal(0),
    setupOpen = useSignal(false);
  const loadActivity = $(
    async (scope: ActivityScope, cursor: WorkspaceModel.HistoryCursor | null = null) => {
      const port = props.port;
      if (port === undefined) return;
      const eventScope = historyScope(
        scope,
        selectedProjectId.value,
        selectedContextId.value,
        selectedActorId.value,
      );
      if (eventScope === null) return;
      activityLoading.value = true;
      const result = await readWorkspaceHistory(port, cursor ?? initialHistoryCursor(eventScope));
      activityLoading.value = false;
      if (!result.ok) {
        activityError.value = result.error.message;
        return;
      }
      activityError.value = null;
      activity.value =
        cursor === null ? result.value.events : [...activity.value, ...result.value.events];
      activityCursor.value = result.value.next;
      activityCoverage.value = historyCoverageLabel(result.value.coverage);
    },
  );
  const loadAgent = $(async (actorId: WorkspaceModel.AgentDescriptor["actorId"]) => {
    const port = props.port;
    const view = projectView.value;
    if (port === undefined || view === null || selectedContextId.value === null) return;
    const epoch = focusEpoch.value;
    const projectId = view.project.projectId;
    const contextId = selectedContextId.value;
    selectedActorId.value = actorId;
    agentOutputLoading.value = true;
    const result = await readAgentOutput(port, {
      projectId,
      contextId,
      actorId,
      afterSequence: DecimalSchema.parse("0"),
    });
    if (
      focusEpoch.value !== epoch ||
      selectedProjectId.value !== projectId ||
      selectedContextId.value !== contextId ||
      selectedActorId.value !== actorId
    )
      return;
    agentOutputLoading.value = false;
    if (result.ok) {
      agentOutput.value = result.value.items;
      agentOutputError.value = null;
    } else {
      agentOutputError.value = result.error.message;
    }
  });
  const loadChat = $(
    async (view: ProjectWorkspaceView, contextId: WorkContextId, epoch: number) => {
      const port = props.port;
      const coordinator = view.coordinator;
      if (port === undefined || coordinator === null) {
        chat.value = [];
        return;
      }
      chatLoading.value = true;
      const result = await readWorkspaceChat(port, {
        projectId: view.project.projectId,
        contextId,
        conversationId: coordinator.conversationId,
        afterSequence: DecimalSchema.parse("0"),
      });
      if (
        focusEpoch.value !== epoch ||
        selectedProjectId.value !== view.project.projectId ||
        selectedContextId.value !== contextId
      )
        return;
      chatLoading.value = false;
      if (result.ok) {
        chat.value = result.value.messages;
        chatError.value = null;
      } else chatError.value = result.error.message;
    },
  );
  const loadProject = $(
    async (projectId: ProjectId, preferred?: WorkContextId, preserveInspection = false) => {
      const port = props.port;
      if (port === undefined) return;
      const background = preservesInspection(
        preserveInspection,
        selectedProjectId.value === projectId,
        preferred === undefined || selectedContextId.value === preferred,
      );
      if (preserveInspection && !background) return;
      const epoch = ++focusEpoch.value;
      const previousActorId = selectedActorId.value;
      if (!background && preferred !== undefined) selectedContextId.value = preferred;
      if (!background) projectLoading.value = true;
      const result = await readProjectWorkspace(port, projectId, preferred);
      if (
        focusEpoch.value !== epoch ||
        selectedProjectId.value !== projectId ||
        (preferred !== undefined && selectedContextId.value !== preferred)
      )
        return;
      if (!background) projectLoading.value = false;
      if (!result.ok) {
        globalError.value = result.error.message;
        return;
      }
      const contextId = preferred ?? result.value.project.defaultContextId;
      const preserve =
        preserveInspection &&
        selectedProjectId.value === projectId &&
        selectedContextId.value === contextId;
      projectView.value = result.value;
      boardViews[projectId] = result.value;
      selectedContextId.value = contextId;
      if (
        !preserve ||
        !result.value.network.agents.some((agent) => agent.actorId === previousActorId)
      ) {
        selectedActorId.value = null;
        agentOutput.value = [];
      } else if (previousActorId !== null) {
        await loadAgent(previousActorId);
      }
      globalError.value = null;
      await loadChat(result.value, contextId, epoch);
    },
  );
  const loadProjects = $(async () => {
    const port = props.port;
    if (port === undefined) return;
    const result = await listWorkspaceProjects(port);
    if (!result.ok) {
      globalError.value = result.error.message;
      return;
    }
    projects.value = result.value;
    Object.assign(boardViews, await readProjectBoardViews(port, result.value));
  });
  const openPortfolioProject = $(
    async (projectId: ProjectId, nextTab: WorkspaceTab, contextId?: WorkContextId) => {
      selectedProjectId.value = projectId;
      activityScope.value = "project";
      tab.value = nextTab;
      await Promise.all([loadProject(projectId, contextId), loadActivity("project")]);
    },
  );
  const refreshCurrentProject = $(async () => {
    canvasRefreshEpoch.value += 1;
    const projectId = selectedProjectId.value;
    const contextId = selectedContextId.value;
    if (projectId !== null && contextId !== null) await loadProject(projectId, contextId, true);
  });
  useVisibleTask$(({ cleanup }) => {
    theme.value = window.localStorage.getItem("quicklens.theme") === "dark" ? "dark" : "light";
    void loadProjects();
    void loadActivity("all");
    const port = props.port;
    if (port === undefined) return;
    const controller = new AbortController();
    const refresh = createProjectEventRefresh(loadProject, () => (canvasRefreshEpoch.value += 1));
    void observeEvents(port, controller.signal, (event) => {
      if (
        eventMatchesScope(
          event,
          activityScope.value,
          selectedProjectId.value,
          selectedActorId.value,
        )
      ) {
        activity.value = uniqueEvents([...activity.value, event]);
      }
      refresh.accept(event, selectedProjectId.value, selectedContextId.value);
    });
    cleanup(() => {
      controller.abort();
      refresh.close();
    });
  });
  if (
    tab.value === "plan" &&
    projectView.value?.snapshot.state === "ready" &&
    selectedProjectId.value !== null &&
    selectedContextId.value !== null &&
    props.port !== undefined
  ) {
    return (
      <WorkspacePlan
        port={props.port}
        projectId={selectedProjectId.value}
        contextId={selectedContextId.value}
        onBack$={$(() => (tab.value = "agents"))}
      />
    );
  }
  return (
    <div class={`workspace-shell theme-${theme.value}`}>
      <WorkspaceHeader
        demoLabel={props.demoLabel}
        headerActionLabel={props.headerActionLabel}
        onHeaderAction$={props.onHeaderAction$}
        projectName={projectView.value?.project.displayName}
        contexts={projectView.value?.contexts ?? []}
        selectedContextId={selectedContextId.value}
        theme={theme.value}
        onContext$={$((value) => {
          if (selectedProjectId.value !== null)
            void loadProject(selectedProjectId.value, WorkContextIdSchema.parse(value));
        })}
        onTheme$={$(() => {
          theme.value = theme.value === "light" ? "dark" : "light";
          window.localStorage.setItem("quicklens.theme", theme.value);
        })}
      />
      <div
        class={
          "workspace-body" +
          (props.product !== undefined && (projects.value.length === 0 || setupOpen.value)
            ? " setup-open"
            : "")
        }
      >
        <ProjectRail
          projects={projects.value}
          selectedProjectId={selectedProjectId.value}
          onSelectAll$={$(() => {
            focusEpoch.value += 1;
            selectedProjectId.value = null;
            selectedContextId.value = null;
            projectView.value = null;
            selectedActorId.value = null;
            activityScope.value = "all";
            void loadActivity("all");
          })}
          onSelectProject$={$(async (projectId) => {
            selectedProjectId.value = projectId;
            activityScope.value = "project";
            tab.value = "agents";
            await loadProject(projectId);
            await loadActivity("project");
          })}
          onAddProject$={
            props.product === undefined ? undefined : $(() => (setupOpen.value = true))
          }
        />
        <main class="workspace-main">
          {globalError.value !== null ? <p class="workspace-notice">{globalError.value}</p> : null}
          {props.product !== undefined && (projects.value.length === 0 || setupOpen.value) ? (
            <ProductSetup
              product={props.product}
              workspace={props.port}
              directoryPicker={props.directoryPicker}
              canClose={projects.value.length > 0}
              onClose$={$(() => (setupOpen.value = false))}
              onRegistered$={$(() => undefined)}
              onOpen$={$(async (project) => {
                await loadProjects();
                setupOpen.value = false;
                await openPortfolioProject(project.projectId, "agents");
              })}
            />
          ) : selectedProjectId.value === null ? (
            <WorkspacePortfolio
              port={props.port}
              projects={projects.value}
              views={boardViews}
              theme={theme.value}
              refreshEpoch={canvasRefreshEpoch.value}
              onOpenProject$={$((projectId, contextId) =>
                openPortfolioProject(projectId, "agents", contextId),
              )}
              onOpenQuestions$={$((projectId, contextId) =>
                openPortfolioProject(projectId, "questions", contextId),
              )}
            />
          ) : projectLoading.value ||
            projectView.value === null ||
            selectedContextId.value === null ? (
            <div class="workspace-panel workspace-loading">Loading project workspace…</div>
          ) : (
            <>
              <WorkspaceTabs selected={tab.value} onSelect$={$((next) => (tab.value = next))} />
              {tab.value === "agents" ? (
                <>
                  <CoordinatorControls
                    coordinator={projectView.value.coordinator}
                    sessions={projectView.value.sessions}
                    launchOptions={projectView.value.coordinatorLaunchOptions}
                    execution={projectView.value.execution}
                    starting={coordinatorStarting.value}
                    message={coordinatorMessage.value}
                    onStart$={$(async (option) => {
                      const port = props.port;
                      if (
                        port === undefined ||
                        selectedProjectId.value === null ||
                        selectedContextId.value === null
                      )
                        return;
                      coordinatorStarting.value = true;
                      const result = await port.command({
                        operation: "session.start.v1",
                        clientRequestId: workspaceRequestId("start"),
                        projectId: selectedProjectId.value,
                        contextId: selectedContextId.value,
                        profileId: option.profileId,
                        interactionKind: option.interactionKind,
                      });
                      coordinatorStarting.value = false;
                      coordinatorMessage.value = result.ok
                        ? "Coordinator start requested and durably pending."
                        : result.error.message;
                      await loadProject(selectedProjectId.value, selectedContextId.value, true);
                    })}
                    onLifecycle$={$(async (action) => {
                      const port = props.port;
                      const projectId = selectedProjectId.value;
                      const contextId = selectedContextId.value;
                      const currentView = projectView.value;
                      const sessionId = currentView?.execution.sessionId;
                      if (
                        port === undefined ||
                        projectId === null ||
                        contextId === null ||
                        sessionId === null ||
                        sessionId === undefined ||
                        currentView === null
                      )
                        return;
                      const operation =
                        action === "pause"
                          ? "project.pause.v1"
                          : action === "stop"
                            ? "project.stop.v1"
                            : "project.continue.v1";
                      const result = await port.command({
                        operation,
                        clientRequestId: workspaceRequestId(`project-${action}`),
                        projectId,
                        contextId,
                        sessionId,
                        expectedRevision: currentView.execution.revision,
                        reasonMarkdown: `Requested ${action} from Zap Quick Lens.`,
                      });
                      coordinatorMessage.value = result.ok
                        ? `Project ${action} requested.`
                        : result.error.message;
                      await loadProject(projectId, contextId, true);
                    })}
                  />
                  <AgentPanel
                    network={projectView.value.network}
                    selectedActorId={selectedActorId.value}
                    output={agentOutput.value}
                    outputLoading={agentOutputLoading.value}
                    outputError={agentOutputError.value}
                    onSelect$={$(async (actorId) => {
                      await loadAgent(actorId);
                      activityScope.value = "agent";
                      await loadActivity("agent");
                    })}
                  />
                  <ManagedWorkPanel
                    key={`managed-work:${selectedProjectId.value}:${selectedContextId.value}`}
                    port={props.port}
                    projectId={selectedProjectId.value}
                    contextId={selectedContextId.value}
                    onSelectActor$={$(async (actorId) => {
                      await loadAgent(actorId);
                      activityScope.value = "agent";
                    })}
                    onChanged$={refreshCurrentProject}
                  />
                  <ManagedTerminalWorkspace
                    key={`${selectedProjectId.value}:${selectedContextId.value}`}
                    port={props.port}
                    projectId={selectedProjectId.value}
                    contextId={selectedContextId.value}
                    view={projectView.value}
                    selectedActorId={selectedActorId.value}
                  />
                  <ExecutionSettingsPanel
                    key={
                      "execution-settings:" +
                      selectedProjectId.value +
                      ":" +
                      selectedContextId.value
                    }
                    port={props.port}
                    projectId={selectedProjectId.value}
                    contextId={selectedContextId.value}
                  />
                </>
              ) : tab.value === "chat" ? (
                <ChatPanel
                  coordinator={projectView.value.coordinator}
                  messages={chat.value}
                  loading={chatLoading.value}
                  error={chatError.value}
                  onSend$={$(async (bodyMarkdown) => {
                    const port = props.port;
                    const coordinator = projectView.value?.coordinator;
                    if (
                      port === undefined ||
                      coordinator === undefined ||
                      coordinator === null ||
                      selectedProjectId.value === null ||
                      selectedContextId.value === null
                    )
                      return false;
                    const result = await port.command({
                      operation: "chat.post.v1",
                      clientRequestId: workspaceRequestId("chat"),
                      projectId: selectedProjectId.value,
                      contextId: selectedContextId.value,
                      conversationId: coordinator.conversationId,
                      bodyMarkdown,
                      artifactRefs: [],
                      correlationId: null,
                      causationMessageId: null,
                    });
                    if (!result.ok) {
                      chatError.value = result.error.message;
                      return false;
                    }
                    if (result.value.operation === "chat.post.v1")
                      chat.value = [...chat.value, result.value.message];
                    return true;
                  })}
                />
              ) : tab.value === "questions" ? (
                <WorkspaceQuestions
                  port={props.port}
                  view={projectView.value}
                  contextId={selectedContextId.value}
                  onRefresh$={$(async () => {
                    if (selectedProjectId.value !== null)
                      await loadProject(
                        selectedProjectId.value,
                        selectedContextId.value ?? undefined,
                        true,
                      );
                  })}
                />
              ) : tab.value === "workspaces" ? (
                <RepositoryWorkspacePanel
                  key={`repository:${selectedProjectId.value}:${selectedContextId.value}`}
                  port={props.port}
                  projectId={selectedProjectId.value}
                  contextId={selectedContextId.value}
                  onOpenContext$={$(async (projectId, contextId) => {
                    selectedProjectId.value = projectId;
                    await loadProject(projectId, contextId);
                  })}
                  onChanged$={refreshCurrentProject}
                />
              ) : projectView.value.snapshot.state === "unavailable" ? (
                <PlanUnavailable reason={projectView.value.snapshot.reason} />
              ) : null}
            </>
          )}
        </main>
        <ActivityPanel
          scope={activityScope.value}
          projectLabel={projectView.value?.project.displayName ?? null}
          agentLabel={selectedAgent(projectView.value, selectedActorId.value)?.displayName ?? null}
          projectNames={projectNames(projects.value)}
          actorNames={agentNames(projectView.value)}
          events={activity.value}
          loading={activityLoading.value}
          error={activityError.value}
          hasMore={activityCursor.value !== null}
          coverage={activityCoverage.value}
          onScope$={$(async (scope) => {
            activityScope.value = scope;
            await loadActivity(scope);
          })}
          onLoadMore$={$(() => {
            if (activityCursor.value !== null)
              void loadActivity(activityScope.value, activityCursor.value);
          })}
        />
      </div>
    </div>
  );
});
