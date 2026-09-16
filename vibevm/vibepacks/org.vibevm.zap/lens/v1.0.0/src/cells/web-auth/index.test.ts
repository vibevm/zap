/** @verifies spec://org.vibevm.zap/lens/PROP-003#password-authentication */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir, userInfo } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  createPasswordAuthenticator,
  createPasswordVerifier,
  defaultPasswordVerifierPath,
  loadPasswordAuthenticator,
  writePasswordVerifier,
} from "./index.ts";

const TEST_COST = { N: 1_024, r: 8, p: 1, keyLength: 32, maxmem: 16 * 1024 * 1024 };

test("scrypt verifier accepts only the password and rotation changes its session version", async () => {
  const first = await createPasswordVerifier("correct horse battery", TEST_COST);
  assert.equal(first.ok, true);
  if (!first.ok) return;
  const opened = createPasswordAuthenticator(first.value, {
    maximumConcurrent: 1,
    allowWeakTestCost: true,
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const initialVersion = opened.value.version();
  assert.equal((await opened.value.verify("correct horse battery")).ok, true);
  assert.equal((await opened.value.verify("wrong horse battery")).ok, false);
  const second = await createPasswordVerifier("rotated horse battery", TEST_COST);
  assert.equal(second.ok, true);
  if (!second.ok) return;
  const inFlight = opened.value.verify("correct horse battery");
  opened.value.replace(second.value);
  const oldVerification = await inFlight;
  assert.equal(oldVerification.ok, true);
  assert.equal(oldVerification.version, initialVersion);
  assert.notEqual(opened.value.version(), initialVersion);
  assert.equal((await opened.value.verify("correct horse battery")).ok, false);
  assert.equal((await opened.value.verify("rotated horse battery")).ok, true);
});

test("KDF admission fails fast at its concurrency bound and weak records need an explicit test profile", async () => {
  const verifier = await createPasswordVerifier("bounded concurrency password", TEST_COST);
  assert.equal(verifier.ok, true);
  if (!verifier.ok) return;
  assert.equal(createPasswordAuthenticator(verifier.value).ok, false);
  const opened = createPasswordAuthenticator(verifier.value, {
    maximumConcurrent: 1,
    allowWeakTestCost: true,
  });
  assert.equal(opened.ok, true);
  if (!opened.ok) return;
  const results = await Promise.all(
    Array.from({ length: 5 }, () => opened.value.verify("bounded concurrency password")),
  );
  assert.equal(results.filter((result) => result.ok).length, 1);
});

test("init and rotation write only verifier material under an explicit temporary path", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-web-auth-"));
  context.after(() => rm(root, { recursive: true, force: true }));
  const path = join(root, "nested", "auth.json");
  const initialized = await writePasswordVerifier({
    path,
    password: "initial synthetic password",
    mode: "init",
    cost: TEST_COST,
  });
  assert.equal(initialized.ok, true);
  const text = await readFile(path, "utf8");
  assert.equal(text.includes("initial synthetic password"), false);
  assert.match(text, /"kdf": "scrypt"/);
  if (process.platform === "win32") {
    const acl = spawnSync("icacls.exe", [path], { encoding: "utf8", windowsHide: true });
    assert.equal(acl.status, 0);
    assert.match(
      acl.stdout,
      new RegExp(userInfo().username.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "i"),
    );
  }
  const conflict = await writePasswordVerifier({
    path,
    password: "other synthetic password",
    mode: "init",
    cost: TEST_COST,
  });
  assert.equal(conflict.ok, false);
  const rotated = await writePasswordVerifier({
    path,
    password: "rotated synthetic password",
    mode: "rotate",
    cost: TEST_COST,
  });
  assert.equal(rotated.ok, true);
  const loaded = await loadPasswordAuthenticator(path, {
    maximumConcurrent: 2,
    allowWeakTestCost: true,
  });
  assert.equal(loaded.ok, true);
  if (!loaded.ok) return;
  assert.equal((await loaded.value.verify("initial synthetic password")).ok, false);
  assert.equal((await loaded.value.verify("rotated synthetic password")).ok, true);
  const reloaded = await writePasswordVerifier({
    path,
    password: "runtime refreshed password",
    mode: "rotate",
    cost: TEST_COST,
  });
  assert.equal(reloaded.ok, true);
  assert.equal(await loaded.value.refresh(), true);
  assert.equal((await loaded.value.verify("rotated synthetic password")).ok, false);
  assert.equal((await loaded.value.verify("runtime refreshed password")).ok, true);
});

test("default verifier location is beneath the supplied home without writing it", () => {
  assert.equal(
    defaultPasswordVerifierPath("C:/synthetic-home").replaceAll("\\", "/"),
    "C:/synthetic-home/.vibe/zap/quicklens-web-auth.json",
  );
});

test("missing verifier refuses instead of enabling a passwordless profile", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-missing-auth-"));
  context.after(() => rm(root, { recursive: true, force: true }));
  const loaded = await loadPasswordAuthenticator(join(root, "missing.json"));
  assert.equal(loaded.ok, false);
});

test("concurrent init and rotation have one exclusive winner", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-web-auth-race-"));
  context.after(() => rm(root, { recursive: true, force: true }));
  const path = join(root, "auth.json");
  const initialized = await Promise.all([
    writePasswordVerifier({
      path,
      password: "first concurrent password",
      mode: "init",
      cost: TEST_COST,
    }),
    writePasswordVerifier({
      path,
      password: "second concurrent password",
      mode: "init",
      cost: TEST_COST,
    }),
  ]);
  assert.equal(initialized.filter((result) => result.ok).length, 1);
  const rotated = await Promise.all([
    writePasswordVerifier({
      path,
      password: "first rotated password",
      mode: "rotate",
      cost: TEST_COST,
    }),
    writePasswordVerifier({
      path,
      password: "second rotated password",
      mode: "rotate",
      cost: TEST_COST,
    }),
  ]);
  assert.equal(rotated.filter((result) => result.ok).length, 1);
});

test("short Unicode passwords are refused by code-point length before scrypt", async () => {
  assert.equal((await createPasswordVerifier("😀😀😀😀😀😀", TEST_COST)).ok, false);
  assert.equal((await createPasswordVerifier("короткийпароль", TEST_COST)).ok, false);
  assert.equal((await createPasswordVerifier("123456789012345", TEST_COST)).ok, true);
});

test("default verifier uses the production scrypt cost profile", async () => {
  const verifier = await createPasswordVerifier("production strength passphrase");
  assert.equal(verifier.ok, true);
  if (!verifier.ok) return;
  assert.deepEqual(
    { N: verifier.value.cost.N, r: verifier.value.cost.r, p: verifier.value.cost.p },
    { N: 65_536, r: 8, p: 2 },
  );
});
