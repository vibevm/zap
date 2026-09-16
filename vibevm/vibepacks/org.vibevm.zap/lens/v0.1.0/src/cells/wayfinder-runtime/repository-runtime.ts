/** Trusted local repository-workspace runtime. @scope spec://org.vibevm.zap/lens/PROP-014#multi-user-hosts */
import { randomUUID } from "node:crypto";
import { dirname, resolve } from "node:path";
import { z } from "zod";
import {
  createGitArgvAdapter,
  createLocalIdleWriterGate,
  createRepositoryWorkspaceService,
  openRepositoryWorkspaceStore,
  type RepositoryIntegrationTestRunner,
  type RepositoryWorkspaceService,
} from "../repository-workspaces/index.ts";
import type { WorkspaceStore } from "../workspace-store/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "../workspace-model/index.ts";
import type { RuntimeRepositoryWriterActivity } from "./repository-activity.ts";

export const RuntimeRepositoryWorkspaceConfigSchema = z
  .object({
    executionHostId: z.string().min(3).max(160),
    mergeIdentity: z
      .object({ name: z.string().min(1).max(200), email: z.email().max(320) })
      .strict()
      .nullable()
      .default(null),
    testProfiles: z.array(z.literal("repository.consistency")).max(1).default([]),
  })
  .strict();
export type RuntimeRepositoryWorkspaceConfig = z.infer<
  typeof RuntimeRepositoryWorkspaceConfigSchema
>;

export interface RuntimeRepositoryWorkspaces {
  readonly service: RepositoryWorkspaceService;
  readonly executionHostId: string;
  readonly testProfiles: readonly {
    readonly profileId: string;
    readonly displayName: string;
    readonly descriptionMarkdown: string;
  }[];
  close(): void;
}

export function openRuntimeRepositoryWorkspaces(input: {
  readonly config: RuntimeRepositoryWorkspaceConfig;
  readonly workspaceDatabasePath: string;
  readonly workspaceStore: WorkspaceStore;
  readonly writerActivity?: RuntimeRepositoryWriterActivity;
}):
  | { readonly ok: true; readonly value: RuntimeRepositoryWorkspaces }
  | {
      readonly ok: false;
      readonly message: string;
    } {
  const config = RuntimeRepositoryWorkspaceConfigSchema.safeParse(input.config);
  if (!config.success) return { ok: false, message: "repository workspace config is invalid" };
  const repositoryStore = openRepositoryWorkspaceStore(
    `${input.workspaceDatabasePath}.repository-workspaces`,
  );
  const git = createGitArgvAdapter();
  const writerGate = createLocalIdleWriterGate({
    observeActiveWriters: async (target) => {
      await Promise.resolve();
      const worktree = repositoryStore.getWorktree(target.targetWorktreeId);
      if (worktree === null) return 1;
      const execution = input.workspaceStore.readProjectExecution(
        ProjectIdSchema.parse(worktree.record.projectId),
        WorkContextIdSchema.parse(worktree.record.contextId),
      );
      if (!execution.ok) return 1;
      if (!["paused", "stopped", "uninitialized"].includes(execution.value.state)) return 1;
      return input.writerActivity?.observe(worktree.record.projectId, worktree.record.contextId) ===
        "idle"
        ? 0
        : 1;
    },
    createLeaseId: () => `repository-writer.${randomUUID()}`,
  });
  const testRunner = consistencyTestRunner(git, config.data.testProfiles);
  const opened = createRepositoryWorkspaceService({
    executionHostId: config.data.executionHostId,
    trustedWorktreeRoot: resolve(dirname(input.workspaceDatabasePath), "repository-worktrees"),
    store: repositoryStore,
    git,
    writerGate,
    testRunner,
    idFactory: { create: (kind) => `${kind}.${randomUUID()}` },
    mergeIdentity: config.data.mergeIdentity,
    now: () => new Date().toISOString(),
  });
  if (!opened.ok) {
    repositoryStore.close();
    return { ok: false, message: opened.error.message };
  }
  const profiles = config.data.testProfiles.map(() => ({
    profileId: "repository.consistency",
    displayName: "Repository consistency",
    descriptionMarkdown:
      "Checks only that the prepared integration worktree has no uncommitted Git changes.",
  }));
  return {
    ok: true,
    value: {
      service: opened.value,
      executionHostId: config.data.executionHostId,
      testProfiles: profiles,
      close: () => {
        opened.value.close();
      },
    },
  };
}

function consistencyTestRunner(
  git: ReturnType<typeof createGitArgvAdapter>,
  profiles: readonly "repository.consistency"[],
): RepositoryIntegrationTestRunner {
  return {
    async run(input) {
      if (
        input.profileId !== "repository.consistency" ||
        !profiles.includes("repository.consistency")
      )
        return {
          ok: false,
          error: { code: "denied", message: "repository test profile is not registered" },
        };
      const status = await git.run({
        cwd: input.projectDirectory,
        args: ["status", "--porcelain"],
      });
      return status.exitCode === 0
        ? {
            ok: true,
            value: {
              passed: status.stdout.trim() === "",
              summary:
                status.stdout.trim() === ""
                  ? "Integration worktree is clean. Project tests were not run."
                  : "Integration worktree contains uncommitted changes.",
              runnerId: "repository.consistency",
              runnerAuthorityId: "wayfinder.local-repository-runtime",
            },
          }
        : {
            ok: false,
            error: { code: "unavailable", message: "repository consistency check failed" },
          };
    },
  };
}
