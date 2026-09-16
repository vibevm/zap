/** Clean committed source witness for Zap distributions. @scope spec://org.vibevm.zap/lens/PROP-017#binary */
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { isAbsolute, join, resolve } from "node:path";

const EXCLUDES = new Set([".git", ".vibe", ".vibeignore", "node_modules", "target"]);

export async function verifySourceWitness(options, ports = {}) {
  const observed = await captureSourceWitness(options, ports);
  if (observed.sourceCommit !== options.sourceCommit)
    return cleanupFailure(observed, "sourceCommit differs from clean repository HEAD");
  if (observed.sourceTree !== options.sourceTree)
    return cleanupFailure(
      observed,
      "sourceTree differs from the committed Vibe portable content hash",
    );
  return observed;
}

export async function inspectSourceWitness(input, ports = {}) {
  const captured = await captureSourceWitness(input, ports);
  try {
    return { sourceCommit: captured.sourceCommit, sourceTree: captured.sourceTree };
  } finally {
    await captured.cleanup();
  }
}

export async function captureSourceWitness(input, ports = {}) {
  if (typeof input.sourceRoot !== "string" || !isAbsolute(input.sourceRoot))
    failure("sourceRoot must be absolute");
  const options = {
    sourceRoot: resolve(input.sourceRoot),
    gitExecutable: input.gitExecutable ?? "git",
    tarExecutable: input.tarExecutable ?? "tar.exe",
  };
  const runner = ports.runner ?? createRunner();
  const gitBase = ["-c", "core.longpaths=true", "-C", options.sourceRoot];
  const head = (
    await runChecked(
      runner,
      command(options.gitExecutable, [...gitBase, "rev-parse", "HEAD"], options.sourceRoot),
      "source commit observation",
    )
  ).stdout.trim();
  const status = await runChecked(
    runner,
    command(
      options.gitExecutable,
      [...gitBase, "status", "--porcelain=v1", "--untracked-files=all"],
      options.sourceRoot,
    ),
    "source cleanliness observation",
  );
  if (status.stdout !== "") failure("source repository is not clean");
  const workingTree = await portableContentHash(options.sourceRoot);
  const snapshot = await (ports.sourceSnapshot ?? createGitSnapshot)(
    { ...options, sourceCommit: head },
    runner,
  );
  try {
    const archivedTree = await portableContentHash(snapshot.root);
    if (archivedTree !== workingTree)
      failure("clean checkout bytes differ from the committed Git archive snapshot");
    return {
      sourceCommit: head,
      sourceTree: archivedTree,
      root: snapshot.root,
      cleanup: snapshot.cleanup,
    };
  } catch (error) {
    await snapshot.cleanup();
    throw error;
  }
}

async function cleanupFailure(captured, message) {
  await captured.cleanup();
  failure(message);
}

async function createGitSnapshot(options, runner) {
  const temporary = await mkdtemp(join(tmpdir(), "zap source snapshot "));
  const archive = join(temporary, "source.tar");
  const root = join(temporary, "checkout");
  await mkdir(root);
  try {
    await runChecked(
      runner,
      command(
        options.gitExecutable,
        [
          "-c",
          "core.longpaths=true",
          "-C",
          options.sourceRoot,
          "archive",
          "--format=tar",
          "-o",
          archive,
          options.sourceCommit,
        ],
        options.sourceRoot,
      ),
      "committed source archive",
    );
    await runChecked(
      runner,
      command(options.tarExecutable, ["-xf", archive, "-C", root], temporary),
      "committed source extraction",
    );
    return { root, cleanup: () => rm(temporary, { recursive: true, force: true }) };
  } catch (error) {
    await rm(temporary, { recursive: true, force: true });
    throw error;
  }
}

export async function portableContentHash(root) {
  const files = [];
  await collectPortableFiles(resolve(root), "", files);
  files.sort((left, right) => Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)));
  const digest = createHash("sha256");
  for (const file of files) {
    digest.update(Buffer.from(file.path, "utf8"));
    digest.update(Buffer.from([0]));
    digest.update(await readFile(file.absolute));
    digest.update(Buffer.from([0]));
  }
  return `sha256-tree/1:${digest.digest("hex")}`;
}

async function collectPortableFiles(root, prefix, files) {
  const current = prefix === "" ? root : join(root, ...prefix.split("/"));
  const entries = await readdir(current, { withFileTypes: true });
  for (const entry of entries) {
    if (EXCLUDES.has(entry.name)) continue;
    const path = prefix === "" ? entry.name : `${prefix}/${entry.name}`;
    if (entry.isDirectory()) await collectPortableFiles(root, path, files);
    else if (entry.isFile())
      files.push({ path: path.replaceAll("\\", "/"), absolute: join(current, entry.name) });
  }
}

function createRunner() {
  return {
    async run(request) {
      const { spawn } = await import("node:child_process");
      return new Promise((accept, reject) => {
        const child = spawn(request.executable, request.args, {
          cwd: request.cwd,
          env: process.env,
          windowsHide: true,
          stdio: ["ignore", "pipe", "pipe"],
        });
        let stdout = "";
        let stderr = "";
        child.stdout.setEncoding("utf8").on("data", (chunk) => (stdout += chunk));
        child.stderr.setEncoding("utf8").on("data", (chunk) => (stderr += chunk));
        child.once("error", reject);
        child.once("close", (code) => accept({ code: code ?? -1, stdout, stderr }));
      });
    },
  };
}

function command(executable, args, cwd) {
  return { executable, args, cwd };
}

async function runChecked(runner, request, label) {
  const result = await runner.run(request);
  if (result.code !== 0) failure(`${label} failed with exit ${result.code}`);
  return result;
}

function failure(message) {
  throw new Error(
    `violates REQ spec://org.vibevm.zap/lens/PROP-017#binary: ${message}; fix surface: bind the distribution to a clean committed Zap snapshot`,
  );
}
