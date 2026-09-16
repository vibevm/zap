import { rmSync } from "node:fs";
import { basename, dirname, parse, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(fileURLToPath(new URL("../", import.meta.url)));
const dist = resolve(packageRoot, "dist");
if (basename(dist) !== "dist" || dirname(dist) !== packageRoot || dist === parse(dist).root) {
  throw new Error(
    "violates REQ spec://org.vibevm.zap/lens/PROP-001#packaging: build cleanup escaped the package dist boundary; fix surface: restore tooling/clean-dist.js to <package-root>/dist only",
  );
}
rmSync(dist, { recursive: true, force: true });
