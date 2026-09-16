/** @verifies spec://org.vibevm.zap/lens/PROP-003#verification */
import assert from "node:assert/strict";
import { createServer as createHttpsServer, request as httpsRequest } from "node:https";
import type { IncomingHttpHeaders } from "node:http";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { DecimalSchema } from "../protocol/index.ts";

import {
  ExactDecimalSchema,
  QuicklensRefSchema,
  QuicklensSnapshotSchema,
  type QuicklensDataSource,
} from "../quicklens-model/index.ts";
import {
  createPasswordAuthenticator,
  createPasswordVerifier,
  loadPasswordAuthenticator,
  writePasswordVerifier,
} from "../web-auth/index.ts";
import { createQuicklensWebGateway, type WebOperation } from "./index.ts";
import {
  NativeApprovalRequestSchema,
  WorkspaceCommandResponseSchema,
  type WorkspaceClientPort,
} from "../workspace-model/index.ts";

const ORIGIN = "https://quicklens.test";
const PROOF = "synthetic-proxy-proof-token-123456789";
const PASSWORD = "synthetic web password";
const ROTATED_PASSWORD = "rotated synthetic web password";
const ALL_OPERATIONS: WebOperation[] = [
  "read",
  "answer",
  "intent",
  "preview",
  "apply",
  "reconcile",
  "decide",
  "invalidations",
];

test("HTTPS proxy profile enforces password, session, CSRF, origin, scope and logout", async (context) => {
  const rendererRoot = await mkdtemp(join(tmpdir(), "quicklens-web-renderer-"));
  const authRoot = await mkdtemp(join(tmpdir(), "quicklens-web-running-auth-"));
  await writeFile(join(rendererRoot, "index.html"), "<!doctype html><title>Quicklens test</title>");
  context.after(() => rm(rendererRoot, { recursive: true, force: true }));
  context.after(() => rm(authRoot, { recursive: true, force: true }));
  const authPath = join(authRoot, "auth.json");
  const initialized = await writePasswordVerifier({
    path: authPath,
    password: PASSWORD,
    mode: "init",
    cost: testCost(),
  });
  assert.equal(initialized.ok, true);
  const authenticator = await loadPasswordAuthenticator(authPath, {
    maximumConcurrent: 1,
    allowWeakTestCost: true,
  });
  assert.equal(authenticator.ok, true);
  if (!authenticator.ok) return;
  let now = Date.parse("2026-09-15T00:00:00.000Z");
  const calls: string[] = [];
  const gateway = createQuicklensWebGateway({
    source: fixtureSource(calls),
    authenticator: authenticator.value,
    rendererRoot,
    publicOrigin: ORIGIN,
    proxyProofToken: PROOF,
    role: "owner",
    allowedOperations: ALL_OPERATIONS,
    sessionTtlMilliseconds: 60_000,
    maximumLoginAttempts: 2,
    loginWindowMilliseconds: 60_000,
    clock: () => now,
  });
  assert.equal(gateway.ok, true);
  if (!gateway.ok) return;
  context.after(() => gateway.value.close());
  const started = await gateway.value.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  assert.equal(started.value.address, "127.0.0.1");

  const direct = await fetch(`http://127.0.0.1:${String(started.value.port)}/?web=1`);
  assert.equal(direct.status, 403);

  const proxy = await startProxy(started.value.port);
  context.after(proxy.close);
  assert.equal((await proxy.call("GET", "/?web=1")).status, 200);
  assert.equal((await proxy.call("GET", "/mcp")).status, 404);
  assert.equal((await proxy.call("POST", "/api/v1/read", {})).status, 401);
  assert.equal(
    (await proxy.call("POST", "/api/v1/login", { password: "wrong password value" })).status,
    401,
  );

  const login = await proxy.call("POST", "/api/v1/login", { password: PASSWORD });
  assert.equal(login.status, 200);
  const cookie = cookiePair(login.headers["set-cookie"]);
  const csrf = String(login.headers["x-quicklens-csrf"] ?? "");
  assert.match(
    String(login.headers["set-cookie"]),
    /__Host-qls=.*Secure; HttpOnly; SameSite=Strict; Path=\//,
  );
  assert.equal(login.body.includes(PASSWORD), false);
  assert.equal((await proxy.call("POST", "/api/v1/read", {}, { Cookie: cookie })).status, 403);
  assert.equal(
    (
      await proxy.call(
        "POST",
        "/api/v1/read",
        {},
        {
          Cookie: cookie,
          "X-Quicklens-CSRF": csrf,
          Origin: "https://wrong.example",
        },
      )
    ).status,
    403,
  );
  const read = await proxy.call("POST", "/api/v1/read", {}, authHeaders(cookie, csrf));
  assert.equal(read.status, 200);
  assert.match(read.body, /"sourceMode":"live"/);
  const future = await proxy.call(
    "POST",
    "/api/v1/invalidations",
    { after: 99 },
    authHeaders(cookie, csrf),
  );
  assert.match(future.body, /"reason":"reconnect"/);
  assert.match(future.body, /"next":0/);
  const answer = await proxy.call(
    "POST",
    "/api/v1/answer",
    { questionRef: "question.web", expectedRevision: "1", answer: "Proceed" },
    authHeaders(cookie, csrf),
  );
  assert.equal(answer.status, 200);
  assert.deepEqual(calls, ["read", "answer"]);

  const tampered = await proxy.call(
    "POST",
    "/api/v1/read",
    {},
    authHeaders(`${cookie}tampered`, csrf),
  );
  assert.equal(tampered.status, 401);
  now += 61_000;
  assert.equal(
    (await proxy.call("POST", "/api/v1/read", {}, authHeaders(cookie, csrf))).status,
    401,
  );

  const relogin = await proxy.call("POST", "/api/v1/login", { password: PASSWORD });
  const secondCookie = cookiePair(relogin.headers["set-cookie"]);
  const secondCsrf = String(relogin.headers["x-quicklens-csrf"] ?? "");
  const logout = await proxy.call(
    "POST",
    "/api/v1/logout",
    {},
    authHeaders(secondCookie, secondCsrf),
  );
  assert.equal(logout.status, 200);
  assert.match(String(logout.headers["set-cookie"]), /Max-Age=0/);
  assert.equal(
    (await proxy.call("POST", "/api/v1/read", {}, authHeaders(secondCookie, secondCsrf))).status,
    401,
  );

  const thirdLogin = await proxy.call("POST", "/api/v1/login", { password: PASSWORD });
  const thirdCookie = cookiePair(thirdLogin.headers["set-cookie"]);
  const thirdCsrf = String(thirdLogin.headers["x-quicklens-csrf"] ?? "");
  const rotated = await writePasswordVerifier({
    path: authPath,
    password: ROTATED_PASSWORD,
    mode: "rotate",
    cost: testCost(),
  });
  assert.equal(rotated.ok, true);
  assert.equal(
    (await proxy.call("POST", "/api/v1/read", {}, authHeaders(thirdCookie, thirdCsrf))).status,
    401,
  );
  now += 61_000;
  assert.equal((await proxy.call("POST", "/api/v1/login", { password: PASSWORD })).status, 401);
  assert.equal(
    (await proxy.call("POST", "/api/v1/login", { password: ROTATED_PASSWORD })).status,
    200,
  );
  assert.equal(
    (await proxy.call("POST", "/api/v1/login", { password: "another wrong password" })).status,
    429,
  );
});

test("viewer role cannot invoke action or raw backend routes", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-web-viewer-"));
  await writeFile(join(root, "index.html"), "viewer");
  context.after(() => rm(root, { recursive: true, force: true }));
  const verifier = await createPasswordVerifier(PASSWORD, testCost());
  assert.equal(verifier.ok, true);
  if (!verifier.ok) return;
  const auth = createPasswordAuthenticator(verifier.value, { allowWeakTestCost: true });
  assert.equal(auth.ok, true);
  if (!auth.ok) return;
  const gateway = createQuicklensWebGateway({
    source: fixtureSource([]),
    authenticator: auth.value,
    rendererRoot: root,
    publicOrigin: ORIGIN,
    proxyProofToken: PROOF,
    role: "viewer",
    allowedOperations: ["read", "invalidations"],
    workspaceClientFactory: ({ role }) => fakeWorkspaceClient(role),
  });
  assert.equal(gateway.ok, true);
  if (!gateway.ok) return;
  context.after(() => gateway.value.close());
  const started = await gateway.value.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const proxy = await startProxy(started.value.port);
  context.after(proxy.close);
  const login = await proxy.call("POST", "/api/v1/login", { password: PASSWORD });
  const headers = authHeaders(
    cookiePair(login.headers["set-cookie"]),
    String(login.headers["x-quicklens-csrf"] ?? ""),
  );
  assert.equal((await proxy.call("POST", "/api/v1/read", {}, headers)).status, 200);
  const workspaceMutation = await proxy.call(
    "POST",
    "/api/v1/workspace/command",
    {
      operation: "chat.post.v1",
      clientRequestId: "request.viewer.chat",
      projectId: "project.web",
      contextId: "context.web",
      conversationId: "conversation.web",
      bodyMarkdown: "Denied",
      artifactRefs: [],
      correlationId: null,
      causationMessageId: null,
    },
    headers,
  );
  assert.equal(workspaceMutation.status, 200);
  assert.match(workspaceMutation.body, /forbidden/);
  const viewerRead = await proxy.call("POST", "/api/v1/read", {}, headers);
  assert.match(viewerRead.body, /"questionAnswer":\{"enabled":false/);
  assert.equal(
    (
      await proxy.call(
        "POST",
        "/api/v1/answer",
        { questionRef: "question.web", expectedRevision: "1", answer: "No" },
        headers,
      )
    ).status,
    403,
  );
  assert.equal((await proxy.call("POST", "/api/v1/mcp", {}, headers)).status, 403);
  assert.equal((await proxy.call("POST", "/v1/zap/query", {}, headers)).status, 404);
});

test("workspace routes reuse the password session and trusted client scope", async (context) => {
  const root = await mkdtemp(join(tmpdir(), "quicklens-web-workspace-"));
  await writeFile(join(root, "index.html"), "workspace");
  context.after(() => rm(root, { recursive: true, force: true }));
  const verifier = await createPasswordVerifier(PASSWORD, testCost());
  assert.equal(verifier.ok, true);
  if (!verifier.ok) return;
  const auth = createPasswordAuthenticator(verifier.value, { allowWeakTestCost: true });
  assert.equal(auth.ok, true);
  if (!auth.ok) return;
  const workspaceClientFactory = ({
    role,
  }: {
    readonly role: "viewer" | "operator" | "owner";
  }): WorkspaceClientPort => fakeWorkspaceClient(role);
  const gateway = createQuicklensWebGateway({
    source: fixtureSource([]),
    authenticator: auth.value,
    rendererRoot: root,
    publicOrigin: ORIGIN,
    proxyProofToken: PROOF,
    role: "owner",
    allowedOperations: ALL_OPERATIONS,
    workspaceClientFactory,
  });
  assert.equal(gateway.ok, true);
  if (!gateway.ok) return;
  context.after(() => gateway.value.close());
  const started = await gateway.value.start({ host: "127.0.0.1", port: 0 });
  assert.equal(started.ok, true);
  if (!started.ok) return;
  const proxy = await startProxy(started.value.port);
  context.after(proxy.close);
  assert.equal(
    (await proxy.call("POST", "/api/v1/workspace/read", { operation: "project.list.v1" })).status,
    401,
  );
  const login = await proxy.call("POST", "/api/v1/login", { password: PASSWORD });
  const cookie = cookiePair(login.headers["set-cookie"]);
  const csrf = String(login.headers["x-quicklens-csrf"] ?? "");
  assert.equal(
    (
      await proxy.call(
        "POST",
        "/api/v1/workspace/read",
        { operation: "project.list.v1" },
        authHeaders(cookie, csrf),
      )
    ).status,
    200,
  );
  const ownerControl = await proxy.call(
    "POST",
    "/api/v1/workspace/command",
    {
      operation: "native-approval.respond.v1",
      clientRequestId: "request.web.approval",
      projectId: "project.web",
      contextId: "context.web",
      nativeApprovalId: "approval.web",
      expectedRevision: "1",
      response: { decision: "accept" },
    },
    authHeaders(cookie, csrf),
  );
  assert.equal(ownerControl.status, 200);
  assert.match(ownerControl.body, /approval\.web/);
  assert.equal(
    (
      await proxy.call(
        "POST",
        "/api/v1/workspace/command",
        {
          operation: "project.pause.v1",
          clientRequestId: "request.web.pause",
          projectId: "project.web",
          contextId: "context.web",
          sessionId: "session.web",
          expectedRevision: "1",
          reasonMarkdown: "Pause from web",
        },
        authHeaders(cookie, "bad-csrf"),
      )
    ).status,
    403,
  );
  assert.equal(
    (
      await proxy.call(
        "POST",
        "/api/v1/workspace/command",
        {
          operation: "chat.post.v1",
          clientRequestId: "request.web.chat",
          projectId: "project.web",
          contextId: "context.web",
          conversationId: "conversation.web",
          bodyMarkdown: "Hello from web",
          artifactRefs: [],
          correlationId: null,
          causationMessageId: null,
        },
        authHeaders(cookie, csrf),
      )
    ).status,
    200,
  );
  assert.equal(
    (await proxy.call("POST", "/api/v1/mcp", {}, authHeaders(cookie, csrf))).status,
    403,
  );
});

function fakeWorkspaceClient(role: "viewer" | "operator" | "owner"): WorkspaceClientPort {
  const forbidden = {
    ok: false as const,
    error: {
      code: "forbidden" as const,
      message: "violates REQ spec://org.vibevm.zap/lens/PROP-003#web-authority: role cannot mutate",
    },
  };
  void role;
  return {
    read: async () => ({ ok: true, value: { operation: "project.list.v1", projects: [] } }),
    command: async (input) => {
      if (role !== "owner" || input.operation !== "native-approval.respond.v1") return forbidden;
      return {
        ok: true,
        value: WorkspaceCommandResponseSchema.parse({
          operation: "native-approval.respond.v1",
          approval: NativeApprovalRequestSchema.parse({
            nativeApprovalId: "approval.web",
            projectId: "project.web",
            contextId: "context.web",
            coordinatorSessionId: "session.web",
            originActorId: "actor.web",
            kind: "command_approval",
            title: "Synthetic approval",
            descriptionMarkdown: "Synthetic owner control",
            details: {},
            state: "resolved",
            responseAvailability: { enabled: false, reason: "resolved" },
            revision: "2",
            createdAt: "2026-09-15T00:00:00.000Z",
            updatedAt: "2026-09-15T00:00:00.000Z",
          }),
        }),
      };
    },
    events: async () => ({
      ok: true,
      value: {
        events: [],
        resume: {
          scope: { kind: "all_authorized" },
          afterGlobalSequence: DecimalSchema.parse("0"),
        },
        next: null,
        coverage: { state: "complete" },
      },
    }),
    subscribe: async function* () {
      /* synthetic empty stream */
    },
  };
}

function fixtureSource(calls: string[]): QuicklensDataSource {
  const basis = {
    storeRef: QuicklensRefSchema.parse("store.web"),
    baseRef: QuicklensRefSchema.parse("base.web"),
    revision: ExactDecimalSchema.parse("1"),
    sourceBasisRef: QuicklensRefSchema.parse(`source-basis:${"a".repeat(64)}`),
  };
  const operation = {
    operationRef: QuicklensRefSchema.parse("operation.web"),
    previewRef: null,
    preview: null,
    state: "completed" as const,
    message: "Completed web fixture",
    nextBasis: basis,
  };
  return {
    read: async () => {
      calls.push("read");
      return {
        ok: true,
        value: QuicklensSnapshotSchema.parse({
          sourceMode: "live",
          sourceLabel: "Authenticated web fixture",
          phase: "ready",
          phaseDetail: null,
          capturedAt: "2026-09-15T00:00:00.000Z",
          revision: "1",
          objects: [],
          relationships: [],
          questions: [],
          agentTargets: [],
          plan: null,
        }),
      };
    },
    answerQuestion: async (input) => {
      calls.push("answer");
      return {
        ok: true,
        value: {
          ref: input.questionRef,
          addressedActorLabel: "Web fixture",
          prompt: "Proceed?",
          state: "answered",
          revision: input.expectedRevision,
          answerMode: "free_text",
          choices: [],
          answer: input.answer,
          amendmentCount: ExactDecimalSchema.parse("0"),
        },
      };
    },
    proposePlanIntent: async () => ({ ok: true, value: operation }),
    previewPlan: async () => ({ ok: true, value: operation }),
    applyPlan: async () => ({ ok: true, value: operation }),
    reconcilePlan: async () => ({ ok: true, value: operation }),
    decidePlan: async () => ({ ok: true, value: operation }),
  };
}

async function startProxy(innerPort: number) {
  const [key, cert] = await Promise.all([
    readFile(join(import.meta.dirname, "fixtures", "localhost-key.pem")),
    readFile(join(import.meta.dirname, "fixtures", "localhost-cert.pem")),
  ]);
  const server = createHttpsServer({ key, cert }, (request, response) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const headers: Record<string, string> = {
        Origin: request.headers.origin ?? ORIGIN,
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
        inner.headers.forEach((value, name) => {
          response.setHeader(name, value);
        });
        response.writeHead(inner.status);
        response.end(Buffer.from(await inner.arrayBuffer()));
      });
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address !== null && typeof address !== "string");
  return {
    call: (method: string, path: string, body?: unknown, headers: Record<string, string> = {}) =>
      publicRequest(address.port, method, path, body, headers),
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}

function publicRequest(
  port: number,
  method: string,
  path: string,
  body: unknown,
  headers: Record<string, string>,
): Promise<{ status: number; headers: IncomingHttpHeaders; body: string }> {
  const encoded = body === undefined ? undefined : JSON.stringify(body);
  return new Promise((resolve) => {
    const request = httpsRequest(
      {
        host: "127.0.0.1",
        port,
        path,
        method,
        rejectUnauthorized: false,
        headers: {
          Origin: ORIGIN,
          ...(encoded === undefined
            ? {}
            : {
                "Content-Type": "application/json",
                "Content-Length": String(Buffer.byteLength(encoded)),
              }),
          ...headers,
        },
      },
      (response) => {
        const chunks: Buffer[] = [];
        response.on("data", (chunk: Buffer) => chunks.push(chunk));
        response.on("end", () =>
          resolve({
            status: response.statusCode ?? 0,
            headers: response.headers,
            body: Buffer.concat(chunks).toString("utf8"),
          }),
        );
      },
    );
    if (encoded !== undefined) request.write(encoded);
    request.end();
  });
}

function authHeaders(cookie: string, csrf: string): Record<string, string> {
  return { Cookie: cookie, "X-Quicklens-CSRF": csrf };
}

function cookiePair(value: string | string[] | undefined): string {
  const header = Array.isArray(value) ? value[0] : value;
  return header?.split(";", 1)[0] ?? "";
}

function testCost() {
  return { N: 1_024, r: 8, p: 1, keyLength: 32, maxmem: 16 * 1024 * 1024 };
}
