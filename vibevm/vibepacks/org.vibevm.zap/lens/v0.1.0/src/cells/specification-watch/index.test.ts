/** @verifies spec://org.vibevm.zap/lens/PROP-002#specification-drift */
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { createSpecificationWatch } from "./index.ts";

test("added, changed and deleted specs change the bounded source basis", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-specs-"));
  await writeFile(join(root, "one.xml"), "<spec>one</spec>");
  await mkdir(join(root, "node_modules"));
  await writeFile(join(root, "node_modules", "ignored.xml"), "ignored");
  const opened = await createSpecificationWatch({
    roots: [{ root, include: ["**/*.xml"] }],
    debounceMilliseconds: 25,
    maximumFiles: 10,
    maximumTotalBytes: 10_000,
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  context.after(() => opened.value.close());
  const first = await opened.value.capture();
  assert.equal(first.ok, true);
  if (!first.ok) return;
  assert.equal(first.value.fileCount, 1);
  const unchanged = await opened.value.verify(first.value.digest);
  assert.equal(unchanged.ok && unchanged.value.digest, first.value.digest);

  await writeFile(join(root, "two.xml"), "<spec>two</spec>");
  const added = await opened.value.capture();
  assert.equal(added.ok && added.value.fileCount, 2);
  assert.notEqual(added.ok ? added.value.digest : "", first.value.digest);
  await writeFile(join(root, "one.xml"), "<spec>changed</spec>");
  const changed = await opened.value.capture();
  assert.notEqual(changed.ok ? changed.value.digest : "", added.ok ? added.value.digest : "");
  await rm(join(root, "two.xml"));
  const deleted = await opened.value.capture();
  assert.equal(deleted.ok && deleted.value.fileCount, 1);
  opened.value.close();
});

test("debounced notifications are advisory and fresh capture catches a missed event", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-watch-"));
  const file = join(root, "plan.md");
  await writeFile(file, "first");
  const opened = await createSpecificationWatch({
    roots: [{ root, include: ["**/*.md"] }],
    debounceMilliseconds: 25,
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  context.after(() => opened.value.close());
  const initial = await opened.value.capture();
  assert.equal(initial.ok, true);
  if (!initial.ok) return;
  const signal = new Promise<string>((resolve) => {
    const unsubscribe = opened.value.subscribe((change) => {
      unsubscribe();
      resolve(change.current.digest);
    });
  });
  await writeFile(file, "second");
  const signaled = await Promise.race([
    signal,
    new Promise<string>((resolve) => setTimeout(() => resolve("timeout"), 2_000)),
  ]);
  assert.notEqual(signaled, "timeout");

  opened.value.close();
  await writeFile(file, "third");
  const fresh = await opened.value.verify(initial.value.digest);
  assert.equal(fresh.ok, true);
  assert.notEqual(fresh.ok ? fresh.value.digest : "", initial.value.digest);
});

test("parent-traversing include patterns are rejected", async () => {
  const invalid = await createSpecificationWatch({
    roots: [{ root: tmpdir(), include: ["../*.xml"] }],
  });
  assert.equal(invalid.ok, false);
  if (!invalid.ok) assert.equal(invalid.error.code, "invalid_configuration");
});

test("a single oversized specification is refused before an unbounded read", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-large-spec-"));
  await writeFile(join(root, "large.xml"), "x".repeat(20_000));
  const opened = await createSpecificationWatch({
    roots: [{ root, include: ["**/*.xml"] }],
    maximumTotalBytes: 1_024,
  });
  context.after(() => rm(root, { recursive: true, force: true }));
  assert.equal(opened.ok, false);
  if (!opened.ok) assert.equal(opened.error.code, "limit_exceeded");
});

test("nonmatching directory traversal obeys its independent scan bound", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-directory-bound-"));
  let current = root;
  for (let index = 0; index < 6; index += 1) {
    current = join(current, `directory-${String(index)}`);
    await mkdir(current);
  }
  const opened = await createSpecificationWatch({
    roots: [{ root, include: ["**/*.xml"] }],
    maximumDirectories: 3,
  });
  context.after(() => rm(root, { recursive: true, force: true }));
  assert.equal(opened.ok, false);
  if (!opened.ok) assert.equal(opened.error.code, "limit_exceeded");
});
