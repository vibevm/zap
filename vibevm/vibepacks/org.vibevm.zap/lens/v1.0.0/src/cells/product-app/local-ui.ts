/** Local built-renderer server for ordinary startup. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer, type ServerResponse } from "node:http";
import { extname, isAbsolute, relative, resolve } from "node:path";

export interface LocalProductUi {
  readonly origin: string;
  close(): Promise<void>;
}

export async function startLocalProductUi(options: {
  readonly rendererRoot: string;
  readonly port?: number;
}): Promise<ProductUiResult<LocalProductUi>> {
  const root = resolve(options.rendererRoot);
  if (!isAbsolute(options.rendererRoot)) return failure("renderer root must be absolute");
  const server = createServer((request, response) => {
    void serve(root, request.url, response);
  });
  const started = await listen(server, options.port ?? 4174);
  if (!started.ok) return started;
  return {
    ok: true,
    value: {
      origin: `http://127.0.0.1:${String(started.value)}`,
      close: () => closeServer(server),
    },
  };
}

export type ProductUiResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: { readonly message: string } };

async function serve(
  root: string,
  rawUrl: string | undefined,
  response: ServerResponse,
): Promise<void> {
  try {
    const url = new URL(rawUrl ?? "/", "http://127.0.0.1");
    const requested =
      url.pathname === "/" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
    const file = resolve(root, requested);
    const pathFromRoot = relative(root, file);
    if (pathFromRoot.startsWith("..") || isAbsolute(pathFromRoot)) {
      send(response, 404, "Not found");
      return;
    }
    const information = await stat(file);
    if (!information.isFile()) {
      send(response, 404, "Not found");
      return;
    }
    response.writeHead(200, securityHeaders(file));
    createReadStream(file).pipe(response);
  } catch {
    send(response, 404, "Not found");
  }
}

function securityHeaders(file: string): Record<string, string> {
  return {
    "Cache-Control": "no-store",
    "Content-Security-Policy":
      "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' http://127.0.0.1:*; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
    "Content-Type": contentType(extname(file).toLowerCase()),
    "Referrer-Policy": "no-referrer",
    "X-Content-Type-Options": "nosniff",
  };
}

function contentType(extension: string): string {
  if (extension === ".html") return "text/html; charset=utf-8";
  if (extension === ".js") return "text/javascript; charset=utf-8";
  if (extension === ".css") return "text/css; charset=utf-8";
  if (extension === ".svg") return "image/svg+xml";
  if (extension === ".png") return "image/png";
  return "application/octet-stream";
}

function send(response: ServerResponse, status: number, body: string): void {
  response.writeHead(status, {
    "Cache-Control": "no-store",
    "Content-Type": "text/plain; charset=utf-8",
    "X-Content-Type-Options": "nosniff",
  });
  response.end(body);
}

function listen(
  server: ReturnType<typeof createServer>,
  port: number,
): Promise<ProductUiResult<number>> {
  return new Promise((complete) => {
    const failed = (): void => {
      complete(failure("Zap Quick Lens UI could not bind"));
    };
    server.once("error", failed);
    server.listen(port, "127.0.0.1", () => {
      server.off("error", failed);
      const address = server.address();
      complete(
        typeof address === "object" && address !== null
          ? { ok: true, value: address.port }
          : failure("Zap Quick Lens UI address is unavailable"),
      );
    });
  });
}

function closeServer(server: ReturnType<typeof createServer>): Promise<void> {
  return new Promise((complete) => {
    server.close(() => {
      complete();
    });
    server.closeAllConnections();
  });
}

function failure(message: string): ProductUiResult<never> {
  return { ok: false, error: { message } };
}
