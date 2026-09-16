/** Internal workspace-store state helpers. @scope spec://org.vibevm.zap/lens/PROP-005#history */
import { randomUUID } from "node:crypto";
import { z } from "zod";
import { DecimalSchema } from "../protocol/index.ts";
import {
  HistoryEventIdSchema,
  HistoryEventSchema,
  type HistoryEvent,
  type ProjectId,
  type WorkspaceEventIngest,
} from "../workspace-model/index.ts";
import { WorkspaceDatabase } from "./database.ts";

const CounterRowSchema = z.object({ value: z.bigint() });

export class WorkspaceState {
  readonly database: WorkspaceDatabase;
  readonly clock: () => Date;
  readonly idFactory: (kind: string) => string;
  closed = false;

  constructor(databasePath: string, clock?: () => Date, idFactory?: (kind: string) => string) {
    this.database = new WorkspaceDatabase(databasePath);
    this.clock = clock ?? (() => new Date());
    this.idFactory = idFactory ?? ((kind) => `${kind}.${randomUUID()}`);
  }

  now(): string {
    return this.clock().toISOString();
  }

  id(kind: string): string {
    return this.idFactory(kind);
  }

  json(value: unknown): string {
    return JSON.stringify(value);
  }

  parse<S extends z.ZodType>(text: string, schema: S): z.output<S> {
    const decoded: unknown = JSON.parse(text);
    return schema.parse(decoded);
  }

  nextConversation(projectId: string, contextId: string, conversationId: string): bigint {
    const row = this.database.get(
      `SELECT value FROM workspace_conversation_sequences
       WHERE project_id = ? AND context_id = ? AND conversation_id = ?`,
      CounterRowSchema,
      [projectId, contextId, conversationId],
    );
    const next = (row?.value ?? 0n) + 1n;
    this.database.run(
      `INSERT INTO workspace_conversation_sequences(project_id, context_id, conversation_id, value)
       VALUES(?, ?, ?, ?)
       ON CONFLICT(project_id, context_id, conversation_id) DO UPDATE SET value = excluded.value`,
      [projectId, contextId, conversationId, next],
    );
    return next;
  }

  appendHistory(input: WorkspaceEventIngest): HistoryEvent {
    const global = this.database.get(
      "SELECT value FROM workspace_global_sequence WHERE singleton = 1",
      CounterRowSchema,
    );
    const project = this.database.get(
      "SELECT value FROM workspace_project_sequences WHERE project_id = ?",
      CounterRowSchema,
      [input.projectId],
    );
    const globalSequence = (global?.value ?? 0n) + 1n;
    const projectSequence = (project?.value ?? 0n) + 1n;
    this.database.run("UPDATE workspace_global_sequence SET value = ? WHERE singleton = 1", [
      globalSequence,
    ]);
    this.database.run(
      `INSERT INTO workspace_project_sequences(project_id, value) VALUES(?, ?)
       ON CONFLICT(project_id) DO UPDATE SET value = excluded.value`,
      [input.projectId, projectSequence],
    );
    const event = HistoryEventSchema.parse({
      historyEventId: HistoryEventIdSchema.parse(this.id("history")),
      projectId: input.projectId,
      contextId: input.contextId,
      globalSequence: DecimalSchema.parse(String(globalSequence)),
      projectSequence: DecimalSchema.parse(String(projectSequence)),
      sourceSequence: input.sourceSequence,
      kind: input.kind,
      source: input.source,
      actorId: input.actorId,
      occurrenceAt: input.occurrenceAt,
      ingestedAt: this.now(),
      sourceEventId: input.sourceEventId,
      correlationId: input.correlationId,
      causationId: input.causationId,
      planProvenance: input.planProvenance,
      payload: input.payload,
    });
    this.database.run(
      `INSERT INTO workspace_history(
         global_sequence, project_id, context_id, context_key, actor_id, project_sequence,
         source, source_event_id, public_json
       ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        globalSequence,
        event.projectId,
        event.contextId,
        event.contextId ?? "<project>",
        event.actorId,
        projectSequence,
        event.source,
        event.sourceEventId,
        this.json(event),
      ],
    );
    return event;
  }

  projectExists(projectId: ProjectId): boolean {
    return (
      this.database.get(
        "SELECT COUNT(*) AS value FROM workspace_projects WHERE project_id = ?",
        CounterRowSchema,
        [projectId],
      )?.value === 1n
    );
  }
}
