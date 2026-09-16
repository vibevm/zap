import assert from "node:assert/strict";
import { createServer as createHttpsServer, request as httpsRequest } from "node:https";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { z } from "zod";
import { createPasswordVerifier } from "../web-auth/index.ts";
import {
  HistoryPageSchema,
  ProjectDescriptorSchema,
  ProjectExecutionStateSchema,
  WorkspaceCommandResponseSchema,
  WorkspaceReadResponseSchema,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";
import type { WorkspaceService } from "../workspace-service/index.ts";
import { openWayfinderWebRuntime } from "./web.ts";

const ORIGIN = "https://quicklens.test";
const PROOF = "synthetic-wayfinder-proxy-proof-123456";

test("Wayfinder web helper composes password login, two-project reads and owner lifecycle command", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "wayfinder-web-renderer-"));
  const authRoot = await mkdtemp(join(tmpdir(), "wayfinder-web-auth-"));
  context.after(() => rm(root, { recursive: true, force: true }));
  context.after(() => rm(authRoot, { recursive: true, force: true }));
  await writeFile(join(root, "index.html"), "wayfinder");
  const verifier = await createPasswordVerifier("synthetic password for web QA");
  assert.equal(verifier.ok, true);
  if (!verifier.ok) return;
  const authPath = join(authRoot, "password.json");
  await writeFile(authPath, JSON.stringify(verifier.value));
  const projectIds = ["project.web.a", "project.web.b"];
  const service = fakeService(projectIds);
  const opened = await openWayfinderWebRuntime(
    {
      enabled: true,
      host: "127.0.0.1",
      port: 0,
      rendererRoot: root,
      passwordVerifierPath: authPath,
      publicOrigin: ORIGIN,
      proxyProofToken: PROOF,
      trustedProjectIds: projectIds,
      role: "owner",
      webOperations: ["read", "invalidations"],
      workspaceActions: ["read", "events", "subscribe", "project.pause.v1"],
    },
    service,
  );
  assert.equal(opened.ok, true);
  if (!opened.ok || opened.value === undefined) return;
  const runtime = opened.value;
  context.after(() => runtime.close());
  const started = await runtime.start();
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const proxy = await createProxy(started.value.port);
  context.after(proxy.close);
  const login = await proxy.request("POST", "/api/v1/login", {
    password: "synthetic password for web QA",
  });
  assert.equal(login.status, 200);
  const cookie = login.headers["set-cookie"]?.split(";", 1)[0] ?? "";
  const csrf = login.headers["x-quicklens-csrf"] ?? "";
  const read = await proxy.request(
    "POST",
    "/api/v1/workspace/read",
    { operation: "project.list.v1" },
    cookie,
    csrf,
  );
  assert.equal(read.status, 200);
  const readBody = z
    .object({
      ok: z.literal(true),
      value: z.object({
        operation: z.literal("project.list.v1"),
        projects: z.array(z.object({ projectId: z.string() })),
      }),
    })
    .parse(read.body);
  assert.deepEqual(
    readBody.value.projects.map((project) => project.projectId),
    projectIds,
  );
  const command = await proxy.request(
    "POST",
    "/api/v1/workspace/command",
    {
      operation: "project.pause.v1",
      clientRequestId: "request.web.pause",
      projectId: projectIds[0],
      contextId: "context.web.a",
      sessionId: "session.web",
      expectedRevision: "1",
      reasonMarkdown: "QA pause",
    },
    cookie,
    csrf,
  );
  assert.equal(command.status, 200);
  assert.equal(z.object({ ok: z.literal(true) }).parse(command.body).ok, true);
  const logout = await proxy.request("POST", "/api/v1/logout", {}, cookie, csrf);
  assert.equal(logout.status, 200);
  assert.equal(
    (
      await proxy.request(
        "POST",
        "/api/v1/workspace/read",
        { operation: "project.list.v1" },
        cookie,
        csrf,
      )
    ).status,
    401,
  );
});

function fakeService(projectIds: readonly string[]): WorkspaceService {
  const projects = projectIds.map((projectId) =>
    ProjectDescriptorSchema.parse({
      projectId,
      displayName: projectId,
      repositoryRootRefs: [`repo.${projectId}`],
      defaultContextId: `context.${projectId}`,
      actions: {},
      revision: "1",
      createdAt: "2026-09-16T00:00:00.000Z",
      updatedAt: "2026-09-16T00:00:00.000Z",
    }),
  );
  const client: WorkspaceClientPort = {
    read: async () => ({
      ok: true,
      value: WorkspaceReadResponseSchema.parse({ operation: "project.list.v1", projects }),
    }),
    command: async () => ({
      ok: true,
      value: WorkspaceCommandResponseSchema.parse({
        operation: "project.pause.v1",
        execution: ProjectExecutionStateSchema.parse({
          projectId: projectIds[0],
          contextId: "context.web.a",
          state: "pausing",
          sessionId: "session.web",
          processEpoch: "epoch.web",
          pendingAction: null,
          lastAction: null,
          revision: "2",
          updatedAt: "2026-09-16T00:00:00.000Z",
        }),
      }),
    }),
    events: async () => ({
      ok: true,
      value: HistoryPageSchema.parse({
        events: [],
        resume: { scope: { kind: "all_authorized" }, afterGlobalSequence: "0" },
        next: null,
        coverage: { state: "complete" },
      }),
    }),
    subscribe: async function* () {
      /* empty QA stream */
    },
  };
  return {
    bind: () => client,
    observe: () => undefined,
    close: () => undefined,
  };
}

async function createProxy(innerPort: number) {
  const [key, cert] = await Promise.all([
    readFile(join(import.meta.dirname, "../web-gateway/fixtures/localhost-key.pem")),
    readFile(join(import.meta.dirname, "../web-gateway/fixtures/localhost-cert.pem")),
  ]);
  const server = createHttpsServer({ key, cert }, (request, response) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const headers: Record<string, string> = {
        Origin: ORIGIN,
        "X-Forwarded-Proto": "https",
        "X-Forwarded-Host": "quicklens.test",
        "X-Quicklens-Proxy-Proof": PROOF,
      };
      for (const name of ["cookie", "content-type", "x-quicklens-csrf"]) {
        const value = request.headers[name];
        if (typeof value === "string") headers[name] = value;
      }
      void fetch(`http://127.0.0.1:${String(innerPort)}${request.url ?? "/"}`, {
        method: request.method ?? "GET",
        headers,
        ...(chunks.length === 0 ? {} : { body: Buffer.concat(chunks) }),
      }).then(async (inner) => {
        inner.headers.forEach((value, name) => response.setHeader(name, value));
        response.writeHead(inner.status);
        response.end(Buffer.from(await inner.arrayBuffer()));
      });
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address !== null && typeof address !== "string");
  return {
    request: (method: string, path: string, body: unknown = {}, cookie = "", csrf = "") =>
      request(address.port, method, path, body, cookie, csrf),
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}

function request(
  port: number,
  method: string,
  path: string,
  body: unknown,
  cookie: string,
  csrf: string,
): Promise<{ status: number; headers: Record<string, string>; body: unknown }> {
  const encoded = JSON.stringify(body);
  return new Promise((resolve) => {
    const req = httpsRequest(
      {
        host: "127.0.0.1",
        port,
        path,
        method,
        rejectUnauthorized: false,
        headers: {
          Origin: ORIGIN,
          "Content-Type": "application/json",
          "Content-Length": String(Buffer.byteLength(encoded)),
          ...(cookie === "" ? {} : { Cookie: cookie }),
          ...(csrf === "" ? {} : { "X-Quicklens-CSRF": csrf }),
        },
      },
      (response) => {
        const chunks: Buffer[] = [];
        response.on("data", (chunk: Buffer) => chunks.push(chunk));
        response.on("end", () =>
          resolve({
            status: response.statusCode ?? 0,
            headers: Object.fromEntries(
              Object.entries(response.headers).map(([key, value]) => [
                key,
                Array.isArray(value) ? (value[0] ?? "") : (value ?? ""),
              ]),
            ),
            body: JSON.parse(Buffer.concat(chunks).toString("utf8")),
          }),
        );
      },
    );
    req.end(encoded);
  });
}
