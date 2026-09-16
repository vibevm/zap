/** Durable workspace-store composition. @scope spec://org.vibevm.zap/lens/PROP-005#server-ownership */
import { z } from "zod";
import { DecimalSchema } from "../protocol/index.ts";
import * as claims from "./claims.ts";
import * as chat from "./chat.ts";
import {
  AgentDescriptorSchema,
  AgentOutputItemSchema,
  AgentRelationshipSchema,
  CoordinatorSessionSchema,
  WorkspaceAccessContextSchema,
  WorkspaceCommandContextSchema,
  WorkspaceEventIngestSchema,
  WorkspaceEventsRequestSchema,
  WorkspaceReadRequestSchema,
  type AgentDescriptor,
  type AgentOutputItem,
  type AgentRelationship,
  type CoordinatorSession,
  type HistoryEvent,
  type ProjectId,
  type WorkContextId,
  type WorkspaceCommandContext,
  type WorkspaceCommandRequest,
  type WorkspaceCommandResponse,
  type WorkspaceEventsRequest,
  type WorkspaceReadRequest,
  type WorkspaceResult,
} from "../workspace-model/index.ts";
import { commandWorkspace } from "./commands.ts";
import * as execution from "./execution.ts";
import * as launchRead from "./launch-read.ts";
import * as registration from "./registration.ts";
import * as scope from "./scope.ts";
import { ManagedWakeStoreFacade } from "./managed-wake-facade.ts";
import { failure } from "./errors.ts";
import { eventsWorkspace, readWorkspace } from "./reads.ts";
import { WorkspaceState } from "./state.ts";
import { MaxSequenceSchema, OutputSourceSchema, ScopeRowSchema } from "./store-model.ts";
import {
  ObservedAgentOutputSchema,
  type OpenWorkspaceStoreOptions,
  type CoordinatorLaunchClaim,
  type CoordinatorLaunchClaimInput,
  type CoordinatorLaunchReceipt,
  type ChatDispatchClaim,
  type ChatDispatchSettlement,
  type ObservedChatReply,
  type ProjectLifecycleSettlement,
  type ObservedAgentOutput,
  type TrustedProjectLaunch,
  type TrustedProjectRegistration,
  type TrustedPlanContextRegistration,
  type ExistingContextPlanBinding,
  type ExistingContextPlanningBinding,
  type WorkspaceStore,
} from "./types.ts";

export class SqliteWorkspaceStore extends ManagedWakeStoreFacade implements WorkspaceStore {
  readonly #state: WorkspaceState;

  constructor(options: OpenWorkspaceStoreOptions) {
    super();
    this.#state = new WorkspaceState(options.databasePath, options.clock, options.idFactory);
  }

  protected interactionState(): WorkspaceState {
    return this.#state;
  }
  protected wakeState(): WorkspaceState {
    return this.#state;
  }
  protected wakeClosed(): boolean {
    return this.#state.closed;
  }

  registerProject(rawInput: TrustedProjectRegistration) {
    return registration.registerProject(this.#state, rawInput);
  }
  registerPlanContext(rawInput: TrustedPlanContextRegistration) {
    return registration.registerPlanContext(this.#state, rawInput);
  }
  bindExistingContextPlan(rawInput: ExistingContextPlanBinding) {
    return registration.bindExistingContextPlan(this.#state, rawInput);
  }
  bindExistingContextPlanning(rawInput: ExistingContextPlanningBinding) {
    return registration.bindExistingContextPlanning(this.#state, rawInput);
  }
  resolveProjectLaunch(
    projectId: ProjectId,
    contextId: WorkContextId,
  ): WorkspaceResult<TrustedProjectLaunch> {
    return launchRead.resolveProjectLaunch(this.#state, projectId, contextId);
  }

  claimCoordinatorLaunch(raw: CoordinatorLaunchClaimInput) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return claims.claimCoordinatorLaunch(this.#state, raw);
  }
  recordCoordinatorReceipt(raw: CoordinatorLaunchReceipt) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const recorded = claims.recordCoordinatorReceipt(this.#state, raw);
    if (!recorded.ok) return recorded;
    const activated = execution.activateProjectExecution(this.#state, raw);
    return activated.ok ? recorded : activated;
  }
  readCoordinatorClaim(projectId: ProjectId, contextId: WorkContextId) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return claims.readCoordinatorClaim(this.#state, projectId, contextId);
  }
  readProjectExecution(projectId: ProjectId, contextId: WorkContextId) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return execution.readProjectExecution(this.#state, projectId, contextId);
  }
  requestProjectLifecycle(
    context: WorkspaceCommandContext,
    request: Extract<
      WorkspaceCommandRequest,
      { operation: "project.pause.v1" | "project.stop.v1" | "project.continue.v1" }
    >,
  ) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return execution.requestProjectLifecycle(this.#state, context, request);
  }
  settleProjectLifecycle(input: ProjectLifecycleSettlement) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return execution.settleProjectLifecycle(this.#state, input);
  }
  queueChat(messageId: Parameters<WorkspaceStore["queueChat"]>[0]) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return chat.queueChat(this.#state, messageId);
  }
  nextQueuedChat(projectId: ProjectId, contextId: WorkContextId) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return chat.nextQueuedChat(this.#state, projectId, contextId);
  }
  claimChatDispatch(input: ChatDispatchClaim) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return chat.claimChatDispatch(this.#state, input);
  }
  releaseChatDispatch(input: ChatDispatchClaim) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return chat.releaseChatDispatch(this.#state, input);
  }
  settleChatDispatch(input: ChatDispatchSettlement) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return chat.settleChatDispatch(this.#state, input);
  }
  appendObservedChatReply(input: ObservedChatReply) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return chat.appendObservedChatReply(this.#state, input);
  }
  markCoordinatorClaimState(
    projectId: ProjectId,
    contextId: WorkContextId,
    claimId: CoordinatorLaunchClaim["claimId"],
    state: CoordinatorLaunchClaim["state"],
    updatedAt: string,
  ) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    return claims.markCoordinatorClaimState(
      this.#state,
      projectId,
      contextId,
      claimId,
      state,
      updatedAt,
    );
  }
  read(access: Parameters<WorkspaceStore["read"]>[0], rawRequest: WorkspaceReadRequest) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const context = WorkspaceAccessContextSchema.safeParse(access);
    const request = WorkspaceReadRequestSchema.safeParse(rawRequest);
    if (!context.success || !request.success)
      return failure("invalid_input", "workspace read is malformed");
    try {
      return readWorkspace(this.#state, context.data, request.data);
    } catch {
      return failure("storage_failure", "workspace read failed");
    }
  }

  command(
    rawContext: WorkspaceCommandContext,
    request: WorkspaceCommandRequest,
  ): WorkspaceResult<WorkspaceCommandResponse> {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const context = WorkspaceCommandContextSchema.safeParse(rawContext);
    return context.success
      ? commandWorkspace(this.#state, context.data, request)
      : failure("invalid_input", "workspace command context is malformed");
  }

  events(access: Parameters<WorkspaceStore["events"]>[0], rawRequest: WorkspaceEventsRequest) {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const context = WorkspaceAccessContextSchema.safeParse(access);
    const request = WorkspaceEventsRequestSchema.safeParse(rawRequest);
    if (!context.success || !request.success)
      return failure("invalid_input", "history request is malformed");
    try {
      return eventsWorkspace(this.#state, context.data, request.data);
    } catch {
      return failure("storage_failure", "workspace history read failed");
    }
  }

  upsertCoordinatorSession(raw: CoordinatorSession): WorkspaceResult<CoordinatorSession> {
    return this.#upsertScoped("workspace_sessions", "session_id", CoordinatorSessionSchema, raw);
  }

  upsertAgent(raw: AgentDescriptor): WorkspaceResult<AgentDescriptor> {
    return this.#upsertScoped("workspace_agents", "actor_id", AgentDescriptorSchema, raw);
  }

  recordAgentRelationship(raw: AgentRelationship): WorkspaceResult<AgentRelationship> {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const parsed = AgentRelationshipSchema.safeParse(raw);
    if (
      !parsed.success ||
      scope.context(this.#state, parsed.data.projectId, parsed.data.contextId) === null ||
      !scope.actorExists(
        this.#state,
        parsed.data.projectId,
        parsed.data.contextId,
        parsed.data.fromActorId,
      ) ||
      !scope.actorExists(
        this.#state,
        parsed.data.projectId,
        parsed.data.contextId,
        parsed.data.toActorId,
      )
    ) {
      return failure("invalid_input", "agent relationship scope is invalid");
    }
    try {
      const sourceKey = parsed.data.sourceEventId ?? "<none>";
      this.#state.database.run(
        `INSERT OR IGNORE INTO workspace_agent_relationships(
           project_id, context_id, from_actor_id, to_actor_id, kind,
           provenance, source_event_key, public_json
         ) VALUES(?, ?, ?, ?, ?, ?, ?, ?)`,
        [
          parsed.data.projectId,
          parsed.data.contextId,
          parsed.data.fromActorId,
          parsed.data.toActorId,
          parsed.data.kind,
          parsed.data.provenance,
          sourceKey,
          this.#state.json(parsed.data),
        ],
      );
      return { ok: true, value: parsed.data };
    } catch {
      return failure("storage_failure", "agent relationship write failed");
    }
  }

  appendAgentOutput(raw: AgentOutputItem): WorkspaceResult<AgentOutputItem> {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const parsed = AgentOutputItemSchema.safeParse(raw);
    if (
      !parsed.success ||
      scope.context(this.#state, parsed.data.projectId, parsed.data.contextId) === null ||
      !scope.actorExists(
        this.#state,
        parsed.data.projectId,
        parsed.data.contextId,
        parsed.data.actorId,
      )
    ) {
      return failure("invalid_input", "agent output scope is invalid");
    }
    try {
      this.#state.database.run(
        `INSERT INTO workspace_agent_output(actor_id, sequence, project_id, context_id, public_json)
         VALUES(?, ?, ?, ?, ?)`,
        [
          parsed.data.actorId,
          BigInt(parsed.data.sequence),
          parsed.data.projectId,
          parsed.data.contextId,
          this.#state.json(parsed.data),
        ],
      );
      return { ok: true, value: parsed.data };
    } catch {
      return failure("storage_failure", "agent output write failed");
    }
  }

  appendObservedAgentOutput(raw: ObservedAgentOutput): WorkspaceResult<AgentOutputItem | null> {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const parsed = ObservedAgentOutputSchema.safeParse(raw);
    if (!parsed.success) return failure("invalid_input", "observed agent output is malformed");
    if (
      scope.context(this.#state, parsed.data.output.projectId, parsed.data.output.contextId) ===
        null ||
      !scope.actorExists(
        this.#state,
        parsed.data.output.projectId,
        parsed.data.output.contextId,
        parsed.data.output.actorId,
      )
    ) {
      return failure("invalid_input", "observed agent output scope is invalid");
    }
    try {
      return this.#state.database.transaction(() => {
        const source = this.#state.database.get(
          `SELECT source_event_id FROM workspace_agent_output_sources
           WHERE actor_id = ? AND project_id = ? AND context_id = ? AND source_event_id = ?`,
          OutputSourceSchema,
          [
            parsed.data.output.actorId,
            parsed.data.output.projectId,
            parsed.data.output.contextId,
            parsed.data.sourceEventId,
          ],
        );
        if (source !== null) return { ok: true, value: null };
        const max = this.#state.database.get(
          "SELECT MAX(sequence) AS value FROM workspace_agent_output WHERE actor_id = ?",
          MaxSequenceSchema,
          [parsed.data.output.actorId],
        );
        const output = AgentOutputItemSchema.parse({
          ...parsed.data.output,
          sequence: DecimalSchema.parse(String((max?.value ?? 0n) + 1n)),
        });
        this.#state.database.run(
          `INSERT INTO workspace_agent_output(actor_id, sequence, project_id, context_id, public_json) VALUES(?, ?, ?, ?, ?)`,
          [
            output.actorId,
            BigInt(output.sequence),
            output.projectId,
            output.contextId,
            this.#state.json(output),
          ],
        );
        this.#state.database.run(
          `INSERT INTO workspace_agent_output_sources(actor_id, project_id, context_id, source_event_id, sequence) VALUES(?, ?, ?, ?, ?)`,
          [
            output.actorId,
            output.projectId,
            output.contextId,
            parsed.data.sourceEventId,
            BigInt(output.sequence),
          ],
        );
        return { ok: true, value: output };
      });
    } catch {
      return failure("storage_failure", "observed agent output write failed");
    }
  }

  ingestObservedEvent(
    raw: Parameters<WorkspaceStore["ingestObservedEvent"]>[0],
  ): WorkspaceResult<HistoryEvent | null> {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const parsed = WorkspaceEventIngestSchema.safeParse(raw);
    if (!parsed.success || !this.#state.projectExists(parsed.data.projectId)) {
      return failure("invalid_input", "history event scope is invalid");
    }
    try {
      return this.#state.database.transaction(() => {
        if (parsed.data.sourceEventId !== null) {
          const contextKey = parsed.data.contextId ?? "<project>";
          const found = this.#state.database.get(
            `SELECT COUNT(*) AS value FROM workspace_history
             WHERE project_id = ? AND context_key = ? AND source = ? AND source_event_id = ?`,
            z.object({ value: z.bigint() }),
            [parsed.data.projectId, contextKey, parsed.data.source, parsed.data.sourceEventId],
          );
          if (found?.value === 1n) {
            const duplicate: WorkspaceResult<HistoryEvent | null> = { ok: true, value: null };
            return duplicate;
          }
        }
        const recorded: WorkspaceResult<HistoryEvent | null> = {
          ok: true,
          value: this.#state.appendHistory(parsed.data),
        };
        return recorded;
      });
    } catch {
      return failure("storage_failure", "history event ingestion failed");
    }
  }

  ingestEvent(raw: Parameters<WorkspaceStore["ingestEvent"]>[0]): WorkspaceResult<null> {
    const result = this.ingestObservedEvent(raw);
    return result.ok ? { ok: true, value: null } : result;
  }

  close(): WorkspaceResult<null> {
    if (this.#state.closed) return { ok: true, value: null };
    try {
      this.#state.database.close();
      this.#state.closed = true;
      return { ok: true, value: null };
    } catch {
      return failure("storage_failure", "workspace store close failed");
    }
  }

  #upsertScoped<T extends { projectId: ProjectId; contextId: WorkContextId }>(
    table: "workspace_sessions" | "workspace_agents",
    keyColumn: "session_id" | "actor_id",
    schema: z.ZodType<T>,
    raw: T,
  ): WorkspaceResult<T> {
    if (this.#state.closed) return failure("closed", "workspace store is closed");
    const parsed = schema.safeParse(raw);
    if (
      !parsed.success ||
      scope.context(this.#state, parsed.data.projectId, parsed.data.contextId) === null
    ) {
      return failure("invalid_input", "workspace entity scope is invalid");
    }
    const key =
      keyColumn === "session_id"
        ? CoordinatorSessionSchema.parse(raw).sessionId
        : AgentDescriptorSchema.parse(raw).actorId;
    try {
      const existing = this.#state.database.get(
        `SELECT project_id, context_id FROM ${table} WHERE ${keyColumn} = ?`,
        ScopeRowSchema,
        [key],
      );
      if (
        existing !== null &&
        (existing.project_id !== parsed.data.projectId ||
          existing.context_id !== parsed.data.contextId)
      ) {
        return failure("conflict", "workspace entity identity cannot move across project scope");
      }
      this.#state.database.run(
        `INSERT INTO ${table}(${keyColumn}, project_id, context_id, public_json) VALUES(?, ?, ?, ?)
         ON CONFLICT(${keyColumn}) DO UPDATE SET public_json = excluded.public_json`,
        [key, parsed.data.projectId, parsed.data.contextId, this.#state.json(parsed.data)],
      );
      return { ok: true, value: parsed.data };
    } catch {
      return failure("storage_failure", "workspace entity write failed");
    }
  }
}
