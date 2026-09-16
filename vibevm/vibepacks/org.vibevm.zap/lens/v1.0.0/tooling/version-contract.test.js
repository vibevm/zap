/** Standalone product version joins. @scope spec://org.vibevm.zap/lens/PROP-017#root */
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { ENGINE_COORDINATE, LENS_COORDINATE } from "./source-install/bootstrap-manifest.mjs";

const VERSION = "1.0.0";
const lensRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const groupRoot = resolve(lensRoot, "..", "..");
const engineRoot = join(groupRoot, "zap", "v1.0.0");

test("engine, Lens, application and source installer share the mutable 1.0.0 line", () => {
  assert.deepEqual(LENS_COORDINATE, {
    kind: "tool",
    group: "org.vibevm.zap",
    name: "lens",
    version: VERSION,
  });
  assert.deepEqual(ENGINE_COORDINATE, {
    kind: "flow",
    group: "org.vibevm.zap",
    name: "zap",
    version: VERSION,
  });
  const packageDocument = JSON.parse(readFileSync(join(lensRoot, "package.json"), "utf8"));
  const packageLock = JSON.parse(readFileSync(join(lensRoot, "package-lock.json"), "utf8"));
  assert.equal(packageDocument.version, VERSION);
  assert.equal(packageLock.version, VERSION);
  assert.equal(packageLock.packages[""].version, VERSION);
  const lensManifest = readFileSync(join(lensRoot, "vibe.toml"), "utf8");
  const engineManifest = readFileSync(join(engineRoot, "vibe.toml"), "utf8");
  const cargoManifest = readFileSync(join(engineRoot, "Cargo.toml"), "utf8");
  for (const manifest of [lensManifest, engineManifest]) {
    assert.match(manifest, /^version = "1\.0\.0"$/mu);
    assert.match(manifest, /^min_vibe_version = "1\.0\.0"$/mu);
  }
  assert.match(cargoManifest, /^version = "1\.0\.0"$/mu);
  assert.match(engineManifest, /^installer_package = "org\.vibevm\.zap\/lens@=1\.0\.0"$/mu);
  assert.match(engineManifest, /^repository = "vibevm\/zap"$/mu);
  assert.match(engineManifest, /^release_tag = "v1\.0\.0"$/mu);
  assert.match(engineManifest, /^index_asset = "DISTRIBUTIONS\.json"$/mu);
});

test("historical predecessor is outside the current registry", () => {
  const repositoryRoot = resolve(groupRoot, "..", "..", "..");
  assert.equal(
    existsSync(join(repositoryRoot, "vibevm", "vibepacks", "org.vibevm.world", "zap", "v1.0.0")),
    false,
  );
  assert.equal(
    existsSync(join(repositoryRoot, "archive", "legacy-zap-v1.0", "source", "vibe.toml")),
    true,
  );
});
