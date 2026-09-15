/**
 * Bounded project specification digest and advisory change watcher.
 * @scope spec://org.vibevm.zap/lens/PROP-002#specification-drift
 */
import { createHash } from "node:crypto";
import { lstat, open, opendir, realpath } from "node:fs/promises";
import { watch, type FSWatcher } from "node:fs";
import { relative, resolve, sep } from "node:path";
import { z } from "zod";

export const SpecificationBasisSchema = z
  .object({ digest: z.string().regex(/^[0-9a-f]{64}$/), fileCount: z.number().int().min(0) })
  .strict();
export type SpecificationBasis = z.infer<typeof SpecificationBasisSchema>;

export interface SpecificationRoot {
  readonly root: string;
  readonly include: readonly string[];
}

export interface SpecificationWatchOptions {
  readonly roots: readonly SpecificationRoot[];
  readonly debounceMilliseconds?: number;
  readonly maximumFiles?: number;
  readonly maximumDirectories?: number;
  readonly maximumTotalBytes?: number;
}

export type SpecificationWatchResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code: "invalid_configuration" | "unavailable" | "limit_exceeded";
        readonly message: string;
      };
    };

export interface SpecificationChangeSignal {
  readonly previous: SpecificationBasis;
  readonly current: SpecificationBasis;
}

export interface SpecificationWatch {
  capture(): Promise<SpecificationWatchResult<SpecificationBasis>>;
  verify(expectedDigest: string): Promise<SpecificationWatchResult<SpecificationBasis>>;
  subscribe(listener: (signal: SpecificationChangeSignal) => void): () => void;
  close(): void;
}

const IGNORED = new Set([".git", ".vibe", "cache", "dist", "node_modules", "target", "vibedeps"]);

export async function createSpecificationWatch(
  options: SpecificationWatchOptions,
): Promise<SpecificationWatchResult<SpecificationWatch>> {
  const debounce = options.debounceMilliseconds ?? 250;
  const maximumFiles = options.maximumFiles ?? 5_000;
  const maximumDirectories = options.maximumDirectories ?? 5_000;
  const maximumTotalBytes = options.maximumTotalBytes ?? 32 * 1024 * 1024;
  if (
    options.roots.length === 0 ||
    options.roots.length > 32 ||
    debounce < 25 ||
    debounce > 60_000 ||
    maximumFiles < 1 ||
    maximumFiles > 100_000 ||
    maximumDirectories < 1 ||
    maximumDirectories > 100_000 ||
    maximumTotalBytes < 1 ||
    maximumTotalBytes > 512 * 1024 * 1024 ||
    options.roots.some(
      (entry) =>
        entry.include.length === 0 ||
        entry.include.some(
          (pattern) => !/^[A-Za-z0-9_.*?/-]{1,256}$/.test(pattern) || pattern.includes(".."),
        ),
    )
  ) {
    return fail("invalid_configuration", "specification watch bounds are invalid");
  }
  try {
    const roots = await Promise.all(
      options.roots.map(async (entry) => ({
        root: await realpath(resolve(entry.root)),
        matchers: entry.include.map(globMatcher),
      })),
    );
    if (roots.some((entry) => entry.matchers.length === 0)) {
      return fail("invalid_configuration", "each specification root needs an include glob");
    }
    const state = new WatchState(
      roots,
      debounce,
      maximumFiles,
      maximumDirectories,
      maximumTotalBytes,
    );
    const initial = await state.capture();
    if (!initial.ok) return initial;
    const started = state.start(initial.value);
    if (!started.ok) return started;
    return { ok: true, value: state };
  } catch {
    return fail("unavailable", "configured specification roots could not be opened");
  }
}

class WatchState implements SpecificationWatch {
  readonly #roots: readonly ResolvedRoot[];
  readonly #debounce: number;
  readonly #maximumFiles: number;
  readonly #maximumDirectories: number;
  readonly #maximumTotalBytes: number;
  readonly #listeners = new Set<(signal: SpecificationChangeSignal) => void>();
  readonly #watchers: FSWatcher[] = [];
  #basis: SpecificationBasis | undefined;
  #timer: NodeJS.Timeout | undefined;
  #closed = false;
  #generation = 0;

  constructor(
    roots: readonly ResolvedRoot[],
    debounce: number,
    files: number,
    directories: number,
    bytes: number,
  ) {
    this.#roots = roots;
    this.#debounce = debounce;
    this.#maximumFiles = files;
    this.#maximumDirectories = directories;
    this.#maximumTotalBytes = bytes;
  }

  async capture(): Promise<SpecificationWatchResult<SpecificationBasis>> {
    try {
      const files: { path: string; absolutePath: string }[] = [];
      let directories = 0;
      let totalBytes = 0;
      for (const [rootIndex, root] of this.#roots.entries()) {
        const collected = await collect(
          root.root,
          root,
          this.#maximumFiles - files.length,
          this.#maximumDirectories - directories,
        );
        if (!collected.ok) return collected;
        directories += collected.value.directories;
        for (const path of collected.value.paths) {
          files.push({
            path: `${String(rootIndex)}\0${relative(root.root, path).split(sep).join("/")}`,
            absolutePath: path,
          });
        }
      }
      if (files.length > this.#maximumFiles) {
        return fail("limit_exceeded", "specification file count exceeds the configured bound");
      }
      files.sort((left, right) => Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)));
      const hash = createHash("sha256");
      for (const file of files) {
        const digested = await digestFile(file.absolutePath, this.#maximumTotalBytes - totalBytes);
        if (!digested.ok) return digested;
        totalBytes += digested.value.byteLength;
        const pathBytes = Buffer.from(file.path);
        hash.update(unsignedLength(pathBytes.length));
        hash.update(pathBytes);
        hash.update(unsignedLength(digested.value.byteLength));
        hash.update(digested.value.digest);
      }
      return { ok: true, value: { digest: hash.digest("hex"), fileCount: files.length } };
    } catch {
      return fail("unavailable", "specification files changed during the bounded read");
    }
  }

  async verify(expectedDigest: string): Promise<SpecificationWatchResult<SpecificationBasis>> {
    if (!/^[0-9a-f]{64}$/.test(expectedDigest)) {
      return fail("invalid_configuration", "expected specification digest is invalid");
    }
    return this.capture();
  }

  subscribe(listener: (signal: SpecificationChangeSignal) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  start(initial: SpecificationBasis): SpecificationWatchResult<null> {
    this.#basis = initial;
    try {
      for (const root of this.#roots) {
        const watcher = watch(root.root, { recursive: true }, () => {
          this.schedule();
        });
        watcher.on("error", () => {
          this.schedule();
        });
        this.#watchers.push(watcher);
      }
      return { ok: true, value: null };
    } catch {
      this.#closed = true;
      this.#generation += 1;
      for (const watcher of this.#watchers) watcher.close();
      this.#watchers.length = 0;
      return fail("unavailable", "configured specification roots could not be watched");
    }
  }

  close(): void {
    this.#closed = true;
    this.#generation += 1;
    if (this.#timer !== undefined) clearTimeout(this.#timer);
    for (const watcher of this.#watchers) watcher.close();
    this.#listeners.clear();
  }

  private schedule(): void {
    if (this.#closed) return;
    if (this.#timer !== undefined) clearTimeout(this.#timer);
    const generation = this.#generation;
    this.#timer = setTimeout(() => void this.refresh(generation), this.#debounce);
  }

  private async refresh(generation: number): Promise<void> {
    this.#timer = undefined;
    const prior = this.#basis;
    const current = await this.capture();
    if (
      this.#closed ||
      generation !== this.#generation ||
      !current.ok ||
      prior === undefined ||
      current.value.digest === prior.digest
    )
      return;
    this.#basis = current.value;
    for (const listener of this.#listeners) listener({ previous: prior, current: current.value });
  }
}

interface ResolvedRoot {
  readonly root: string;
  readonly matchers: readonly RegExp[];
}

async function collect(
  root: string,
  config: ResolvedRoot,
  remainingFiles: number,
  remainingDirectories: number,
): Promise<SpecificationWatchResult<{ readonly paths: string[]; readonly directories: number }>> {
  const found: string[] = [];
  const directories = [root];
  let visitedDirectories = 0;
  while (directories.length > 0) {
    const directory = directories.pop();
    if (directory === undefined) break;
    visitedDirectories += 1;
    if (visitedDirectories > remainingDirectories) {
      return fail("limit_exceeded", "specification directory scan exceeds the configured bound");
    }
    const handle = await opendir(directory);
    for await (const entry of handle) {
      if (IGNORED.has(entry.name)) continue;
      const path = resolve(directory, entry.name);
      const stat = await lstat(path);
      if (stat.isSymbolicLink()) continue;
      const resolvedPath = await realpath(path);
      if (!withinRoot(config.root, resolvedPath)) continue;
      if (stat.isDirectory()) {
        directories.push(resolvedPath);
      } else if (stat.isFile()) {
        const relativePath = relative(config.root, resolvedPath).split(sep).join("/");
        if (
          !relativePath.startsWith("../") &&
          config.matchers.some((rule) => rule.test(relativePath))
        ) {
          found.push(resolvedPath);
          if (found.length > remainingFiles) {
            return fail("limit_exceeded", "specification file count exceeds the configured bound");
          }
        }
      }
    }
  }
  return { ok: true, value: { paths: found, directories: visitedDirectories } };
}

async function digestFile(
  path: string,
  remainingBytes: number,
): Promise<SpecificationWatchResult<{ readonly byteLength: number; readonly digest: Buffer }>> {
  if (remainingBytes < 0) {
    return fail("limit_exceeded", "specification bytes exceed the configured bound");
  }
  const file = await open(path, "r");
  try {
    const stat = await file.stat();
    if (!stat.isFile() || stat.size > remainingBytes) {
      return fail("limit_exceeded", "specification bytes exceed the configured bound");
    }
    const hash = createHash("sha256");
    const chunk = Buffer.allocUnsafe(Math.min(64 * 1024, Math.max(remainingBytes + 1, 1)));
    let byteLength = 0;
    let bytesRead: number;
    do {
      const read = await file.read(chunk, 0, chunk.length, null);
      bytesRead = read.bytesRead;
      byteLength += read.bytesRead;
      if (byteLength > remainingBytes) {
        return fail("limit_exceeded", "specification bytes exceed the configured bound");
      }
      hash.update(chunk.subarray(0, read.bytesRead));
    } while (bytesRead > 0);
    return { ok: true, value: { byteLength, digest: hash.digest() } };
  } finally {
    await file.close();
  }
}

function unsignedLength(value: number): Buffer {
  const encoded = Buffer.allocUnsafe(8);
  encoded.writeBigUInt64BE(BigInt(value));
  return encoded;
}

function globMatcher(pattern: string): RegExp {
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  const source = escaped
    .replaceAll("**/", "__DOUBLE_STAR_SLASH__")
    .replaceAll("*", "[^/]*")
    .replaceAll("?", "[^/]")
    .replaceAll("__DOUBLE_STAR_SLASH__", "(?:.*/)?");
  return new RegExp(`^${source}$`, "u");
}

function withinRoot(root: string, path: string): boolean {
  const suffix = relative(root, path);
  return suffix === "" || (!suffix.startsWith(`..${sep}`) && suffix !== "..");
}

function fail(
  code: "invalid_configuration" | "unavailable" | "limit_exceeded",
  message: string,
): SpecificationWatchResult<never> {
  return { ok: false, error: { code, message } };
}
