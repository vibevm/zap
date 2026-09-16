/** Authenticated loopback lifecycle hooks for owned managed sessions.
 * @scope spec://org.vibevm.zap/lens/PROP-012#managed-control
 */
import { randomBytes, randomUUID } from "node:crypto";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { z } from "zod";

export const ManagedHookEventSchema = z.looseObject({
  hook_event_name: z.string().min(1).max(160),
  session_id: z.string().min(1).max(512).optional(),
  notification_type: z.string().min(1).max(160).optional(),
  prompt_id: z.string().min(1).max(512).optional(),
  tool_use_id: z.string().min(1).max(512).optional(),
});
export type ManagedHookEvent = z.infer<typeof ManagedHookEventSchema>;

export interface ManagedHookRegistration {
  readonly url: string;
  readonly authorization: string;
  close(): void;
}

export interface ManagedHookServer {
  register(listener: (event: ManagedHookEvent) => void): ManagedHookRegistration;
  close(): Promise<void>;
}

export async function openManagedHookServer(): Promise<ManagedHookServer> {
  const registrations: HookRegistrations = new Map();
  const server = createServer((request, response) => {
    void handleRequest(registrations, request, response);
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      resolve();
    });
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    await closeServer(server);
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-012#managed-control: managed hook server did not bind loopback; fix surface: reserve an available loopback listener",
    );
  }
  return {
    register(listener) {
      const path = `/managed-hook/${randomUUID()}`;
      const authorization = `Bearer ${randomBytes(32).toString("base64url")}`;
      registrations.set(path, { authorization, listener });
      return {
        url: `http://127.0.0.1:${String(address.port)}${path}`,
        authorization,
        close: () => {
          registrations.delete(path);
        },
      };
    },
    async close() {
      registrations.clear();
      await closeServer(server);
    },
  };
}

type HookRegistrations = Map<
  string,
  { readonly authorization: string; readonly listener: (event: ManagedHookEvent) => void }
>;

async function handleRequest(
  registrations: HookRegistrations,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  const path = request.url === undefined ? "" : new URL(request.url, "http://127.0.0.1").pathname;
  const registration = registrations.get(path);
  if (
    request.method !== "POST" ||
    registration === undefined ||
    request.headers.authorization !== registration.authorization
  ) {
    response.writeHead(404).end();
    return;
  }
  const raw = await body(request);
  const event = raw === null ? null : ManagedHookEventSchema.safeParse(raw);
  if (event === null || !event.success) {
    response.writeHead(400).end();
    return;
  }
  registration.listener(event.data);
  response.writeHead(200, { "content-type": "application/json" }).end("{}");
}

async function body(request: IncomingMessage) {
  const chunks: Buffer[] = [];
  let length = 0;
  for await (const chunk of request) {
    if (!Buffer.isBuffer(chunk)) return null;
    length += chunk.length;
    if (length > 1_000_000) return null;
    chunks.push(chunk);
  }
  try {
    const parsed: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    return parsed;
  } catch {
    return null;
  }
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolve) => {
    server.close(() => {
      resolve();
    });
  });
}
