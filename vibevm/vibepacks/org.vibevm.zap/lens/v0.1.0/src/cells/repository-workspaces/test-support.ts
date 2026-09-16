/** Real-Git disposable fixture support. @scope spec://org.vibevm.zap/lens/PROP-014#verification */
import { createHash } from "node:crypto";
import {
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, relative, resolve } from "node:path";
import {
  createGitArgvAdapter,
  createLocalIdleWriterGate,
  createRepositoryWorkspaceService,
  fixtureGitIdentityEnvironment,
  openRepositoryWorkspaceStore,
  type RepositoryGitPort,
  type RepositoryWorkspaceService,
} from "./index.ts";

export interface FixtureFile {
  readonly path: string;
  readonly content: string;
}
export interface GitFixture {
  readonly tempParent: string;
  readonly root: string;
  readonly projectCwd: string;
  readonly worktreeRoot: string;
  readonly databasePath: string;
  readonly git: RepositoryGitPort;
  readonly initialHead: string;
  readonly initialFiles: ReadonlyMap<string, string>;
  cleanup(): void;
}

export async function createGitFixture(input: {
  readonly seed: string;
  readonly projectRelativePath: string;
  readonly initialFiles: readonly FixtureFile[];
  readonly ignoredFiles: readonly FixtureFile[];
}): Promise<GitFixture> {
  const tempParent = realpathSync(tmpdir());
  const root = mkdtempSync(resolve(tempParent, "zap-repository-git-"));
  if (!contains(tempParent, realpathSync(root)))
    throw new Error(
      reqMessage("fixture root escaped TEMP", "create the fixture below the verified TEMP root"),
    );
  const git = createGitArgvAdapter();
  await gitRequired(git, root, ["init", "--initial-branch=main"]);
  await gitRequired(git, root, ["config", "core.autocrlf", "false"]);
  const projectCwd = safeDirectory(root, input.projectRelativePath);
  const initial = new Map<string, string>();
  for (const file of input.initialFiles) {
    safeWrite(projectCwd, file);
    initial.set(file.path, file.content);
  }
  const ignorePatterns = input.ignoredFiles.map((file) =>
    `/${input.projectRelativePath}/${file.path}`.replaceAll("//", "/"),
  );
  safeWrite(root, { path: ".gitignore", content: `${ignorePatterns.join("\n")}\n` });
  const added = [
    ".gitignore",
    ...input.initialFiles.map((file) => `${input.projectRelativePath}/${file.path}`),
  ];
  await gitRequired(git, root, ["add", "--", ...added]);
  await gitRequired(git, root, ["commit", "-m", `Fixture ${input.seed}`], fixtureEnvironment());
  for (const file of input.ignoredFiles) safeWrite(projectCwd, file);
  const initialHead = await gitRequired(git, root, ["rev-parse", "--verify", "HEAD"]);
  const worktreeRoot = resolve(root, "..", `${root.split(/[\\/]/).at(-1) ?? "fixture"}-worktrees`);
  mkdirSync(worktreeRoot, { recursive: true });
  const databasePath = resolve(root, "..", `${root.split(/[\\/]/).at(-1) ?? "fixture"}.sqlite`);
  return {
    tempParent,
    root,
    projectCwd,
    worktreeRoot,
    databasePath,
    git,
    initialHead,
    initialFiles: initial,
    cleanup() {
      for (const target of [
        root,
        worktreeRoot,
        databasePath,
        `${databasePath}-shm`,
        `${databasePath}-wal`,
      ]) {
        const absolute = resolve(target);
        if (!contains(tempParent, absolute) || absolute === tempParent)
          throw new Error(
            reqMessage("fixture cleanup escaped TEMP", "remove only the owned fixture root"),
          );
        rmSync(absolute, { recursive: true, force: true });
      }
    },
  };
}

export function openFixtureService(
  fixture: GitFixture,
  seed: string,
  git: RepositoryGitPort = fixture.git,
): RepositoryWorkspaceService {
  const store = openRepositoryWorkspaceStore(fixture.databasePath);
  let counter = 0;
  const service = createRepositoryWorkspaceService({
    executionHostId: "host.fixture",
    trustedWorktreeRoot: fixture.worktreeRoot,
    store,
    git,
    writerGate: createLocalIdleWriterGate({
      observeActiveWriters: () => Promise.resolve(0),
      createLeaseId: () => `lease.${seed}.${(++counter).toString()}`,
    }),
    testRunner: {
      async run(input) {
        const status = await git.run({
          cwd: input.projectDirectory,
          args: ["status", "--porcelain=v1", "--untracked-files=all"],
        });
        return status.exitCode === 0
          ? {
              ok: true,
              value: {
                passed: status.stdout.trim().length === 0,
                summary:
                  status.stdout.trim().length === 0
                    ? "fixture workspace is clean"
                    : "fixture workspace is dirty",
                runnerId: "repository.fixture",
                runnerAuthorityId: "principal.fixture-runner",
              },
            }
          : { ok: false, error: { code: "unavailable", message: "fixture test runner failed" } };
      },
    },
    idFactory: {
      create(kind) {
        counter += 1;
        return `${kind}.${createHash("sha256").update(`${seed}:${counter.toString()}`).digest("hex").slice(0, 20)}`;
      },
    },
    mergeIdentity: { name: "Zap Fixture", email: "zap-fixture@invalid.local" },
    now: () => "2026-01-01T00:00:00.000Z",
  });
  if (!service.ok) throw new Error(service.error.message);
  return service.value;
}

export async function commitFixture(
  git: RepositoryGitPort,
  projectCwd: string,
  files: readonly FixtureFile[],
  message: string,
): Promise<string> {
  for (const file of files) safeWrite(projectCwd, file);
  await gitRequired(git, projectCwd, ["add", "--", ...files.map((file) => file.path)]);
  await gitRequired(git, projectCwd, ["commit", "-m", message], fixtureEnvironment());
  return gitRequired(git, projectCwd, ["rev-parse", "--verify", "HEAD"]);
}

export function writeFixture(projectCwd: string, files: readonly FixtureFile[]): void {
  for (const file of files) safeWrite(projectCwd, file);
}

export function readFixtureFile(projectCwd: string, path: string): string {
  const file = safeTarget(projectCwd, path);
  return readFileSync(file, "utf8");
}

function safeWrite(root: string, file: FixtureFile): void {
  const target = safeTarget(root, file.path);
  ensureSafeParents(root, dirname(target));
  try {
    if (lstatSync(target).isSymbolicLink())
      throw new Error(
        reqMessage("fixture target is a symbolic link", "use a regular fixture file"),
      );
  } catch (error) {
    if (!missing(error)) throw error;
  }
  writeFileSync(target, file.content, "utf8");
}

function safeDirectory(root: string, relativePath: string): string {
  const target = safeTarget(root, relativePath);
  ensureSafeParents(root, target);
  return realpathSync(target);
}

function safeTarget(root: string, path: string): string {
  const segments = path.split("/");
  if (
    path.includes("\\") ||
    path.startsWith("/") ||
    /^[A-Za-z]:/.test(path) ||
    segments.some(
      (segment) =>
        segment === "" || segment === "." || segment === ".." || segment.toLowerCase() === ".git",
    )
  )
    throw new Error(
      reqMessage("fixture path is unsafe", "use a relative path without .git or traversal"),
    );
  const target = resolve(root, ...segments);
  if (!contains(realpathSync(root), target))
    throw new Error(
      reqMessage("fixture path escaped root", "keep the file below the fixture workspace"),
    );
  return target;
}

function ensureSafeParents(root: string, target: string): void {
  const canonicalRoot = realpathSync(root);
  const child = relative(canonicalRoot, target);
  let current = canonicalRoot;
  for (const segment of child.split(/[\\/]/).filter((value) => value.length > 0)) {
    current = resolve(current, segment);
    try {
      if (lstatSync(current).isSymbolicLink())
        throw new Error(
          reqMessage("fixture parent is a symbolic link", "use regular fixture directories"),
        );
    } catch (error) {
      if (!missing(error)) throw error;
      mkdirSync(current);
    }
    if (!contains(canonicalRoot, realpathSync(current)))
      throw new Error(
        reqMessage("fixture parent escaped root", "keep fixture parents below the workspace"),
      );
  }
}

function contains(root: string, candidate: string): boolean {
  const child = relative(root, candidate);
  return child === "" || (!child.startsWith("..") && !isAbsolute(child));
}

async function gitRequired(
  git: RepositoryGitPort,
  cwd: string,
  args: readonly string[],
  environment?: Readonly<Record<string, string>>,
): Promise<string> {
  const result = await git.run(
    environment === undefined ? { cwd, args } : { cwd, args, environment },
  );
  if (result.exitCode !== 0)
    throw new Error(result.stderr || result.stdout || "fixture Git command failed");
  return result.stdout.trim();
}

function fixtureEnvironment(): Readonly<Record<string, string>> {
  return {
    ...fixtureGitIdentityEnvironment(),
    GIT_AUTHOR_DATE: "2026-01-01T00:00:00Z",
    GIT_COMMITTER_DATE: "2026-01-01T00:00:00Z",
  };
}

function missing(error: unknown): boolean {
  return typeof error === "object" && error !== null && Reflect.get(error, "code") === "ENOENT";
}

function reqMessage(why: string, fix: string): string {
  return `spec://org.vibevm.zap/lens/PROP-014#verification: ${why}; fix: ${fix}`;
}
