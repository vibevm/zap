/** Public repository product fixture. @scope spec://org.vibevm.zap/lens/PROP-014#verification */
import { randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { relative, resolve } from "node:path";
import { createGitArgvAdapter } from "../repository-workspaces/index.ts";
import { createWorkspaceHttpClient } from "../workspace-client/index.ts";
import { TrustedProjectRegistrationSchema } from "../workspace-store/index.ts";
import {
  ProjectIdSchema,
  WorkContextIdSchema,
  type ProjectId,
  type WorkContextId,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
import { createWayfinderRuntime, type WayfinderRuntime } from "./index.ts";

export interface RepositoryProductFixture {
  readonly root: string;
  readonly repositoryRoot: string;
  readonly worktreeRoot: string;
  readonly projectId: ProjectId;
  readonly contextId: WorkContextId;
  readonly client: WorkspaceClientPort;
  readonly runtime: WayfinderRuntime;
  readonly baseUrl: string;
  close(): Promise<void>;
}

export async function openRepositoryProductFixture(
  options: { readonly origin?: string } = {},
): Promise<RepositoryProductFixture> {
  const tempParent = realpathSync(tmpdir());
  const root = mkdtempSync(resolve(tempParent, "zap-repository-product-"));
  const repositoryRoot = resolve(root, "repository");
  mkdirSync(repositoryRoot, { recursive: true });
  writeFileSync(resolve(repositoryRoot, "README.md"), "repository product fixture\n", "utf8");
  const git = createGitArgvAdapter();
  await gitRequired(git, repositoryRoot, ["init", "--initial-branch=main"]);
  await gitRequired(git, repositoryRoot, ["config", "core.autocrlf", "false"]);
  await gitRequired(git, repositoryRoot, ["add", "--", "README.md"]);
  await gitRequired(git, repositoryRoot, ["commit", "-m", "Fixture"], fixtureIdentity());
  const databasePath = resolve(root, "state", "workspace.sqlite");
  const worktreeRoot = resolve(root, "state", "repository-worktrees");
  const projectId = ProjectIdSchema.parse("project.repository-product");
  const contextId = WorkContextIdSchema.parse("context.repository-product");
  const registration = TrustedProjectRegistrationSchema.parse({
    registrationId: "registration.repository-product",
    projectId,
    displayName: "Repository product fixture",
    repositoryRootRefs: ["repository.repository-product"],
    actions: {
      startCoordinator: {
        state: "unavailable",
        code: "not_configured",
        reason: "No model in fixture",
      },
    },
    context: {
      contextId,
      displayName: "Registered checkout",
      workspaceRef: "workspace.repository-product",
      branchLabel: "main",
      revisionBinding: "fixture.initial",
      planning: { state: "unavailable", reason: "No planning source in fixture" },
      coordinatorConversationId: "conversation.repository-product",
    },
    coordinatorLaunchOptions: [
      {
        profileId: "profile.zap-mock.fixture",
        label: "ZapMockAgent fixture",
        interactionKind: "structured",
        availability: {
          state: "unavailable",
          code: "not_configured",
          reason: "No coordinator process in this repository-only fixture",
        },
      },
    ],
    protected: { cwd: repositoryRoot, launchProfileRef: "profile.zap-mock.fixture" },
  });
  const origin = options.origin ?? "http://repository-product.test";
  const created = createWayfinderRuntime({
    version: 1,
    state: { databasePath },
    gateway: {
      host: "127.0.0.1",
      port: 0,
      namespace: "repositoryproduct",
      pairingToken: randomBytes(32).toString("base64url"),
      allowedHosts: ["127.0.0.1"],
      allowedOrigins: [origin],
    },
    repositoryWorkspaces: {
      executionHostId: "host.repository.fixture",
      mergeIdentity: { name: "Zap Fixture", email: "fixture@invalid.local" },
      testProfiles: ["repository.consistency"],
    },
    projects: [registration],
  });
  if (!created.ok) throw new Error(created.error.message);
  const started = await created.value.start();
  if (!started.ok) {
    await created.value.close();
    throw new Error(started.error.message);
  }
  const ticket = created.value.issuePairingTicket();
  if (!ticket.ok) {
    await created.value.close();
    throw new Error(ticket.error.message);
  }
  const baseUrl = `http://${started.value.host}:${String(started.value.port)}${started.value.basePath}`;
  const client = createWorkspaceHttpClient({
    baseUrl,
    pairingToken: ticket.value.ticket,
    origin,
  });
  if (client === null) {
    await created.value.close();
    throw new Error(
      reqMessage(
        "repository product client could not be created",
        "repair the paired workspace HTTP fixture",
      ),
    );
  }
  return {
    root,
    repositoryRoot,
    worktreeRoot,
    projectId,
    contextId,
    client,
    runtime: created.value,
    baseUrl,
    async close() {
      await created.value.close();
      cleanup(tempParent, root);
    },
  };
}

export async function commitRepositoryProductFixture(
  worktreeDirectory: string,
  relativePath: string,
  content: string,
): Promise<string> {
  const path = resolve(worktreeDirectory, relativePath);
  mkdirSync(resolve(path, ".."), { recursive: true });
  writeFileSync(path, content, "utf8");
  const git = createGitArgvAdapter();
  await gitRequired(git, worktreeDirectory, ["add", "--", relativePath]);
  await gitRequired(git, worktreeDirectory, ["commit", "-m", "Fixture change"], fixtureIdentity());
  const head = await git.run({ cwd: worktreeDirectory, args: ["rev-parse", "--verify", "HEAD"] });
  if (head.exitCode !== 0) throw new Error(head.stderr || "fixture HEAD could not be read");
  return head.stdout.trim();
}

async function gitRequired(
  git: ReturnType<typeof createGitArgvAdapter>,
  cwd: string,
  args: readonly string[],
  environment?: Readonly<Record<string, string>>,
): Promise<void> {
  const result = await git.run(
    environment === undefined ? { cwd, args } : { cwd, args, environment },
  );
  if (result.exitCode !== 0) throw new Error(result.stderr || "fixture Git command failed");
}

function fixtureIdentity(): Readonly<Record<string, string>> {
  return {
    GIT_AUTHOR_NAME: "Zap Fixture",
    GIT_AUTHOR_EMAIL: "fixture@invalid.local",
    GIT_COMMITTER_NAME: "Zap Fixture",
    GIT_COMMITTER_EMAIL: "fixture@invalid.local",
    GIT_AUTHOR_DATE: "2026-01-01T00:00:00Z",
    GIT_COMMITTER_DATE: "2026-01-01T00:00:00Z",
  };
}

function cleanup(parent: string, root: string): void {
  const child = relative(parent, resolve(root));
  if (child === "" || child.startsWith(".."))
    throw new Error(
      reqMessage("repository product fixture cleanup escaped TEMP", "remove only its owned root"),
    );
  rmSync(root, { recursive: true, force: true });
}

function reqMessage(why: string, fix: string): string {
  return `spec://org.vibevm.zap/lens/PROP-014#verification: ${why}; fix: ${fix}`;
}
