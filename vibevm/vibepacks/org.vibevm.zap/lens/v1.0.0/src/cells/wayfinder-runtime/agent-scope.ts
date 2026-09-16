/** Persistent dynamic broker scopes. @scope spec://org.vibevm.zap/lens/PROP-005#project-context */
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import type { LensBroker } from "../broker/index.ts";
import {
  ConversationIdSchema,
  CredentialSchema,
  EnrollPrincipalInputSchema,
  WorkspaceIdSchema,
  type Credential,
} from "../protocol/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "../workspace-model/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import type { GatewayAddress } from "../http/index.ts";

export const WayfinderAgentScopeInputSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
  })
  .strict();
export type WayfinderAgentScopeInput = z.infer<typeof WayfinderAgentScopeInputSchema>;

export interface WayfinderEnsuredAgentScope {
  readonly projectId: WayfinderAgentScopeInput["projectId"];
  readonly contextId: WayfinderAgentScopeInput["contextId"];
  readonly workspaceId: WayfinderAgentScopeInput["workspaceId"];
  readonly conversationId: WayfinderAgentScopeInput["conversationId"];
  readonly brokerUrl: string;
  readonly humanCredentialFile: string;
  readonly agentCredentialFile: string;
}

export type AgentScopeResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly message: string };

export interface ResolvedAgentScope extends WayfinderEnsuredAgentScope {
  readonly humanPrincipalToken: Credential;
  readonly agentPrincipalToken: Credential;
}

export interface AgentScopeManager {
  ensure(
    input: WayfinderAgentScopeInput,
    address: GatewayAddress | null,
  ): Promise<AgentScopeResult<WayfinderEnsuredAgentScope>>;
  resolve(workspaceId: string, conversationId: string): ResolvedAgentScope | null;
  close(): void;
}

const CatalogRowSchema = z
  .object({
    projectId: ProjectIdSchema,
    contextId: WorkContextIdSchema,
    workspaceId: WorkspaceIdSchema,
    conversationId: ConversationIdSchema,
    humanCredentialFile: z.string().min(1),
    agentCredentialFile: z.string().min(1),
  })
  .strict();
const CredentialFileSchema = z
  .object({ protocol: z.literal("lens/1"), principalToken: CredentialSchema })
  .strict();

export function openAgentScopeManager(options: {
  readonly databasePath: string;
  readonly broker: LensBroker;
  readonly store: WorkspaceStore;
  readonly configured: readonly {
    readonly workspaceId: WayfinderAgentScopeInput["workspaceId"];
    readonly conversationId: WayfinderAgentScopeInput["conversationId"];
    readonly humanPrincipalToken: Credential;
    readonly agentPrincipalToken: Credential;
  }[];
}): AgentScopeResult<AgentScopeManager> {
  try {
    const database = new DatabaseSync(options.databasePath);
    database.exec(
      "CREATE TABLE IF NOT EXISTS wayfinder_agent_scopes (" +
        "project_id TEXT NOT NULL, context_id TEXT NOT NULL, workspace_id TEXT NOT NULL, " +
        "conversation_id TEXT NOT NULL, human_credential_file TEXT NOT NULL, " +
        "agent_credential_file TEXT NOT NULL, PRIMARY KEY (project_id, context_id), " +
        "UNIQUE (workspace_id, conversation_id)) STRICT;",
    );
    const configured = new Map(
      options.configured.map((scope) => [scopeKey(scope.workspaceId, scope.conversationId), scope]),
    );
    const scopes = new Map<string, ResolvedAgentScope>();
    const rows = database
      .prepare(
        "SELECT project_id AS projectId, context_id AS contextId, workspace_id AS workspaceId, " +
          "conversation_id AS conversationId, human_credential_file AS humanCredentialFile, " +
          "agent_credential_file AS agentCredentialFile FROM wayfinder_agent_scopes " +
          "ORDER BY project_id, context_id",
      )
      .all()
      .map((row) => CatalogRowSchema.parse(row));
    for (const row of rows) {
      const loaded = loadScope(row);
      if (!loaded.ok) {
        database.close();
        return loaded;
      }
      scopes.set(scopeKey(row.workspaceId, row.conversationId), loaded.value);
    }
    let serial = Promise.resolve();
    return {
      ok: true,
      value: {
        ensure(input, address) {
          const execute = serial.then(() =>
            ensureScope(options, database, configured, scopes, input, address),
          );
          serial = execute.then(
            () => undefined,
            () => undefined,
          );
          return execute;
        },
        resolve(workspaceId, conversationId) {
          return (
            scopes.get(scopeKey(workspaceId, conversationId)) ??
            configuredScope(configured.get(scopeKey(workspaceId, conversationId)))
          );
        },
        close() {
          database.close();
        },
      },
    };
  } catch {
    return { ok: false, message: "agent scope catalog could not be opened" };
  }
}

async function ensureScope(
  options: Parameters<typeof openAgentScopeManager>[0],
  database: DatabaseSync,
  configured: ReadonlyMap<
    string,
    Parameters<typeof openAgentScopeManager>[0]["configured"][number]
  >,
  scopes: Map<string, ResolvedAgentScope>,
  raw: WayfinderAgentScopeInput,
  address: GatewayAddress | null,
): Promise<AgentScopeResult<WayfinderEnsuredAgentScope>> {
  await Promise.resolve();
  if (address === null) return { ok: false, message: "agent gateway is not started" };
  const parsed = WayfinderAgentScopeInputSchema.safeParse(raw);
  if (!parsed.success) return { ok: false, message: "agent scope input is invalid" };
  const input = parsed.data;
  const launch = options.store.resolveProjectLaunch(input.projectId, input.contextId);
  if (
    !launch.ok ||
    launch.value.agentScope === null ||
    launch.value.agentScope.workspaceId !== input.workspaceId ||
    launch.value.agentScope.conversationId !== input.conversationId
  )
    return { ok: false, message: "agent scope does not match trusted project registration" };
  const key = scopeKey(input.workspaceId, input.conversationId);
  const existing = scopes.get(key);
  if (existing !== undefined) {
    return existing.projectId === input.projectId && existing.contextId === input.contextId
      ? { ok: true, value: publicScope(existing, address) }
      : { ok: false, message: "broker scope is already bound to another project context" };
  }
  const byProject = database
    .prepare(
      "SELECT project_id AS projectId, context_id AS contextId, workspace_id AS workspaceId, " +
        "conversation_id AS conversationId, human_credential_file AS humanCredentialFile, " +
        "agent_credential_file AS agentCredentialFile FROM wayfinder_agent_scopes " +
        "WHERE project_id = ? AND context_id = ?",
    )
    .get(input.projectId, input.contextId);
  if (byProject !== undefined) {
    const row = CatalogRowSchema.parse(byProject);
    if (row.workspaceId !== input.workspaceId || row.conversationId !== input.conversationId)
      return { ok: false, message: "project context broker scope changed" };
    const loaded = loadScope(row);
    if (!loaded.ok) return loaded;
    scopes.set(key, loaded.value);
    return { ok: true, value: publicScope(loaded.value, address) };
  }
  const bootstrap = configured.get(key);
  const credentials =
    bootstrap === undefined
      ? enrollScope(options.broker, input)
      : { ok: true as const, value: bootstrap };
  if (!credentials.ok) return credentials;
  const files = writeCredentials(options.databasePath, input, credentials.value);
  if (!files.ok) return files;
  database
    .prepare(
      "INSERT INTO wayfinder_agent_scopes " +
        "(project_id, context_id, workspace_id, conversation_id, " +
        "human_credential_file, agent_credential_file) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .run(
      input.projectId,
      input.contextId,
      input.workspaceId,
      input.conversationId,
      files.value.humanCredentialFile,
      files.value.agentCredentialFile,
    );
  const resolved: ResolvedAgentScope = {
    ...input,
    ...files.value,
    brokerUrl: brokerUrl(address),
    ...credentials.value,
  };
  scopes.set(key, resolved);
  return { ok: true, value: publicScope(resolved, address) };
}

function enrollScope(
  broker: LensBroker,
  input: WayfinderAgentScopeInput,
): AgentScopeResult<{ humanPrincipalToken: Credential; agentPrincipalToken: Credential }> {
  const human = broker.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "human_responder",
      workspaceIds: [input.workspaceId],
      conversationIds: [input.conversationId],
      capabilities: ["events:read", "message:emit", "question:answer", "question:amend"],
    }),
  );
  if (!human.ok) return { ok: false, message: human.error.message };
  const agent = broker.enrollPrincipal(
    EnrollPrincipalInputSchema.parse({
      kind: "agent",
      workspaceIds: [input.workspaceId],
      conversationIds: [input.conversationId],
      capabilities: [
        "message:emit",
        "question:ask",
        "question:cancel",
        "inbox:read",
        "inbox:ack",
        "inbox:forward",
        "actor:delegate",
        "actor:expire",
        "plan:propose",
      ],
    }),
  );
  return agent.ok
    ? {
        ok: true,
        value: {
          humanPrincipalToken: human.value.principalToken,
          agentPrincipalToken: agent.value.principalToken,
        },
      }
    : { ok: false, message: agent.error.message };
}

function writeCredentials(
  databasePath: string,
  input: WayfinderAgentScopeInput,
  credentials: {
    readonly humanPrincipalToken: Credential;
    readonly agentPrincipalToken: Credential;
  },
): AgentScopeResult<{ humanCredentialFile: string; agentCredentialFile: string }> {
  try {
    const directory = databasePath + ".agent-scopes";
    const stem = digest(input.projectId + "\u0000" + input.contextId);
    const humanCredentialFile = join(directory, stem + ".human.json");
    const agentCredentialFile = join(directory, stem + ".agent.json");
    mkdirSync(directory, { recursive: true });
    writeCredential(humanCredentialFile, credentials.humanPrincipalToken);
    writeCredential(agentCredentialFile, credentials.agentPrincipalToken);
    return { ok: true, value: { humanCredentialFile, agentCredentialFile } };
  } catch {
    return { ok: false, message: "protected agent scope credentials could not be written" };
  }
}

function writeCredential(path: string, principalToken: Credential): void {
  const temporary = path + ".tmp";
  writeFileSync(temporary, JSON.stringify({ protocol: "lens/1", principalToken }), {
    encoding: "utf8",
    mode: 0o600,
  });
  renameSync(temporary, path);
}

function loadScope(row: z.infer<typeof CatalogRowSchema>): AgentScopeResult<ResolvedAgentScope> {
  try {
    const human = CredentialFileSchema.parse(
      JSON.parse(readFileSync(row.humanCredentialFile, "utf8")),
    );
    const agent = CredentialFileSchema.parse(
      JSON.parse(readFileSync(row.agentCredentialFile, "utf8")),
    );
    return {
      ok: true,
      value: {
        ...row,
        brokerUrl: "",
        humanPrincipalToken: human.principalToken,
        agentPrincipalToken: agent.principalToken,
      },
    };
  } catch {
    return { ok: false, message: "persisted agent scope credentials are unavailable" };
  }
}

function configuredScope(
  scope: Parameters<typeof openAgentScopeManager>[0]["configured"][number] | undefined,
): ResolvedAgentScope | null {
  if (scope === undefined) return null;
  return {
    projectId: ProjectIdSchema.parse("project.configured"),
    contextId: WorkContextIdSchema.parse("context.configured"),
    ...scope,
    brokerUrl: "",
    humanCredentialFile: "",
    agentCredentialFile: "",
  };
}

function publicScope(
  scope: ResolvedAgentScope,
  address: GatewayAddress,
): WayfinderEnsuredAgentScope {
  return { ...scope, brokerUrl: brokerUrl(address) };
}
function brokerUrl(address: GatewayAddress): string {
  return "http://" + address.host + ":" + String(address.port);
}
function scopeKey(workspaceId: string, conversationId: string): string {
  return workspaceId + "\u0000" + conversationId;
}
function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}
