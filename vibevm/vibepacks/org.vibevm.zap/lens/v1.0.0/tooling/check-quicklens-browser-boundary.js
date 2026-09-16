import { readdirSync, readFileSync } from "node:fs";
import { extname, join, relative, resolve } from "node:path";

const packageRoot = resolve(import.meta.dirname, "..");
const roots = [
  "src/cells/quicklens-model",
  "src/cells/quicklens-graph",
  "src/cells/quicklens-demo",
  "src/cells/quicklens-ui",
  "src/quicklens/browser",
].map((path) => resolve(packageRoot, path));
const forbidden = [
  "node:",
  "electron",
  "zap-client",
  "/broker/",
  "/http/",
  "/mcp/",
  "/session-vault/",
  "/transport/",
];
const findings = [];

for (const root of roots) {
  for (const file of walk(root)) {
    if (![".ts", ".tsx"].includes(extname(file)) || file.endsWith(".test.ts")) continue;
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(/(?:from\s+|import\s*\()["']([^"']+)["']/g)) {
      const specifier = match[1] ?? "";
      if (forbidden.some((token) => specifier === token || specifier.includes(token))) {
        findings.push(`${relative(packageRoot, file)} -> ${specifier}`);
      }
    }
  }
}

if (findings.length > 0) {
  process.stderr.write(`${findings.join("\n")}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write("quicklens browser boundary: clean\n");
}

function walk(root) {
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}
