/** Retained application management closure. @scope spec://org.vibevm.zap/lens/PROP-017#adapter */
import { copyFile, mkdir } from "node:fs/promises";
import { join } from "node:path";

export const MANAGEMENT_TOOL_FILES = Object.freeze([
  "application.mjs",
  "bootstrap.mjs",
  "bootstrap-manifest.mjs",
  "bootstrap-snapshot.mjs",
  "bootstrap-status.mjs",
  "bootstrap-system.mjs",
  "contract.mjs",
]);

export async function stageManagementTools(lensRoot, runtimeRoot) {
  const destination = join(runtimeRoot, "tooling", "source-install");
  await mkdir(destination, { recursive: true });
  for (const file of MANAGEMENT_TOOL_FILES)
    await copyFile(join(lensRoot, "tooling", "source-install", file), join(destination, file));
}
