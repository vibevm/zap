/** Bounded renderer assets. @scope spec://org.vibevm.zap/lens/PROP-003#channel-separation */
import { lstat, readFile, realpath, stat } from "node:fs/promises";
import type { IncomingMessage, ServerResponse } from "node:http";
import { extname, relative, resolve, sep } from "node:path";

export async function serveStaticAsset(
  request: IncomingMessage,
  response: ServerResponse,
  path: string,
  root: string,
): Promise<void> {
  try {
    const relativePath = path === "/" ? "index.html" : decodeURIComponent(path.slice(1));
    const resolvedRoot = await realpath(root);
    const candidate = resolve(resolvedRoot, relativePath);
    const file = await realpath(candidate);
    const suffix = relative(resolvedRoot, file);
    const entry = await lstat(candidate);
    if (suffix === ".." || suffix.startsWith(`..${sep}`) || entry.isSymbolicLink()) {
      sendNotFound(response);
      return;
    }
    const info = await stat(file);
    if (!info.isFile() || info.size > 20 * 1024 * 1024) {
      sendNotFound(response);
      return;
    }
    const content = request.method === "HEAD" ? Buffer.alloc(0) : await readFile(file);
    response.writeHead(200, {
      "Content-Type": contentType(file),
      "Content-Length": String(info.size),
    });
    response.end(content);
  } catch {
    sendNotFound(response);
  }
}

function sendNotFound(response: ServerResponse): void {
  response.writeHead(404, { "Content-Type": "text/plain; charset=utf-8" });
  response.end("Not found");
}

function contentType(path: string): string {
  const extension = extname(path);
  if (extension === ".html") return "text/html; charset=utf-8";
  if (extension === ".js") return "text/javascript; charset=utf-8";
  if (extension === ".css") return "text/css; charset=utf-8";
  if (extension === ".json" || extension === ".map") return "application/json; charset=utf-8";
  return "application/octet-stream";
}
