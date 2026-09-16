/** Trusted repository-workspace runtime dependencies. @scope spec://org.vibevm.zap/lens/PROP-014#multi-user-hosts */
import { createHash } from "node:crypto";
import { mkdirSync, realpathSync } from "node:fs";
import { isAbsolute, relative, resolve } from "node:path";
import type { RepositoryWorkspaceResult } from "./contracts.ts";
import type { RepositoryGitPort } from "./git.ts";
import type { RepositoryWriterGate } from "./gate.ts";
import type { RepositoryWorkspaceStore } from "./store.ts";

export interface IntegrationTestRunInput {
  readonly integrationId: string;
  readonly repositoryId: string;
  readonly planId: string;
  readonly commit: string;
  readonly profileId: string;
  readonly projectDirectory: string;
}
export interface IntegrationTestRunResult {
  readonly passed: boolean;
  readonly summary: string;
  readonly runnerId: string;
  readonly runnerAuthorityId: string;
}
export interface RepositoryIntegrationTestRunner {
  run(input: IntegrationTestRunInput): Promise<RepositoryWorkspaceResult<IntegrationTestRunResult>>;
}
export interface RepositoryWorkspaceIdFactory {
  create(kind: "repository" | "worktree" | "integration" | "assignment"): string;
}
export interface RepositoryWorkspaceRuntimeOptions {
  readonly executionHostId: string;
  readonly trustedWorktreeRoot: string;
  readonly store: RepositoryWorkspaceStore;
  readonly git: RepositoryGitPort;
  readonly writerGate: RepositoryWriterGate;
  readonly testRunner: RepositoryIntegrationTestRunner;
  readonly idFactory: RepositoryWorkspaceIdFactory;
  readonly mergeIdentity: { readonly name: string; readonly email: string } | null;
  readonly now: () => string;
}

export interface RepositoryWorkspaceRuntime {
  readonly executionHostId: string;
  readonly worktreeRoot: string;
  readonly store: RepositoryWorkspaceStore;
  readonly git: RepositoryGitPort;
  readonly writerGate: RepositoryWriterGate;
  readonly testRunner: RepositoryIntegrationTestRunner;
  readonly idFactory: RepositoryWorkspaceIdFactory;
  readonly mergeIdentity: { readonly name: string; readonly email: string } | null;
  readonly now: () => string;
}

export function mergeIdentityEnvironment(
  runtime: RepositoryWorkspaceRuntime,
): RepositoryWorkspaceResult<Readonly<Record<string, string>>> {
  if (runtime.mergeIdentity === null)
    return fail("unavailable", "a trusted Git merge identity is not configured");
  return {
    ok: true,
    value: {
      GIT_AUTHOR_NAME: runtime.mergeIdentity.name,
      GIT_AUTHOR_EMAIL: runtime.mergeIdentity.email,
      GIT_COMMITTER_NAME: runtime.mergeIdentity.name,
      GIT_COMMITTER_EMAIL: runtime.mergeIdentity.email,
    },
  };
}

export function prepareRuntime(
  options: RepositoryWorkspaceRuntimeOptions,
): RepositoryWorkspaceResult<RepositoryWorkspaceRuntime> {
  try {
    mkdirSync(options.trustedWorktreeRoot, { recursive: true });
    const worktreeRoot = realpathSync(options.trustedWorktreeRoot);
    return { ok: true, value: { ...options, worktreeRoot } };
  } catch {
    return fail("unavailable", "trusted worktree root is unavailable");
  }
}

export function ownedPath(runtime: RepositoryWorkspaceRuntime, segment: string): string | null {
  const candidate = resolve(runtime.worktreeRoot, segment);
  return contains(runtime.worktreeRoot, candidate) ? candidate : null;
}

export function contains(root: string, candidate: string): boolean {
  const child = relative(root, candidate);
  return child === "" || (!child.startsWith("..") && !isAbsolute(child));
}

export function requestDigest(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

export function branchSegment(value: string): string {
  const readable = value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 32);
  const digest = createHash("sha256").update(value).digest("hex").slice(0, 12);
  return `${readable || "workspace"}-${digest}`;
}

export function nextRevision(revision: string): string {
  return (BigInt(revision) + 1n).toString();
}

export function fail(
  code:
    | "invalid_input"
    | "conflict"
    | "unavailable"
    | "host_unavailable"
    | "dirty"
    | "stale"
    | "conflicted"
    | "not_ready"
    | "denied",
  message: string,
): RepositoryWorkspaceResult<never> {
  return { ok: false, error: { code, message } };
}

export function hostAvailable(
  runtime: RepositoryWorkspaceRuntime,
  requestedHostId: string,
): RepositoryWorkspaceResult<null> {
  return requestedHostId === runtime.executionHostId
    ? { ok: true, value: null }
    : fail("host_unavailable", "repository operation is not available on the requested host");
}

export async function gitText(
  runtime: RepositoryWorkspaceRuntime,
  cwd: string,
  args: readonly string[],
  environment?: Readonly<Record<string, string>>,
): Promise<RepositoryWorkspaceResult<string>> {
  const result = await runtime.git.run(
    environment === undefined ? { cwd, args } : { cwd, args, environment },
  );
  if (result.exitCode !== 0)
    return fail("unavailable", boundedGitMessage(result.stderr || result.stdout));
  return { ok: true, value: result.stdout.trim() };
}

function boundedGitMessage(value: string): string {
  const normalized = value.trim().replace(/\s+/g, " ");
  return normalized.length === 0 ? "Git operation failed" : normalized.slice(0, 1_000);
}
