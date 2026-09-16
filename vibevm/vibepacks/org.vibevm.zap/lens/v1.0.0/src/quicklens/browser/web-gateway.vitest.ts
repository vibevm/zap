import { expect, test } from "vitest";

import { createQuicklensWebClient } from "./web-gateway.ts";
import { createWorkspaceWebClient } from "../../cells/workspace-client/web.ts";
import { ClientRequestIdSchema, ConversationIdSchema } from "../../cells/protocol/index.ts";
import { ProjectIdSchema, WorkContextIdSchema } from "../../cells/workspace-model/index.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-003#verification */
test("remote browser client keeps CSRF in memory and calls only same-origin named routes", async () => {
  const calls: Array<{ readonly path: string; readonly init: RequestInit }> = [];
  const fetcher = async (path: string, init?: RequestInit): Promise<Response> => {
    calls.push({ path, init: init ?? {} });
    if (path === "/api/v1/login") {
      return Response.json(
        {
          ok: true,
          value: {
            authenticated: true,
            role: "viewer",
            expiresAt: "2026-09-15T01:00:00.000Z",
          },
        },
        { headers: { "X-Quicklens-CSRF": "csrf-memory-only" } },
      );
    }
    if (path === "/api/v1/invalidations") {
      return Response.json({ ok: true, value: { events: [], next: 0 } });
    }
    return Response.json({ ok: true, value: path === "/api/v1/logout" ? { loggedOut: true } : {} });
  };
  const client = createQuicklensWebClient(fetcher);
  expect(await client.login("synthetic browser password")).toEqual({
    ok: true,
    message: "Authenticated.",
  });
  const unsubscribe = client.subscribe(() => undefined);
  await new Promise((resolve) => setTimeout(resolve, 0));
  await client.read();
  unsubscribe();
  await client.logout();
  expect(calls.map((call) => call.path)).toEqual([
    "/api/v1/login",
    "/api/v1/invalidations",
    "/api/v1/read",
    "/api/v1/logout",
  ]);
  expect(new Headers(calls[2]?.init.headers).get("X-Quicklens-CSRF")).toBe("csrf-memory-only");
  expect(calls.every((call) => call.path.startsWith("/api/v1/"))).toBe(true);
  expect(calls[0]?.init.body).toBe(JSON.stringify({ password: "synthetic browser password" }));
  expect(calls.every((call) => call.init.signal instanceof AbortSignal)).toBe(true);
});

test("expired session clears authenticated state and stops invalidation polling", async () => {
  const calls: string[] = [];
  const client = createQuicklensWebClient(async (path) => {
    calls.push(path);
    return path === "/api/v1/login"
      ? Response.json(
          { ok: true, value: { authenticated: true } },
          { headers: { "X-Quicklens-CSRF": "expired-session-csrf" } },
        )
      : Response.json(
          {
            ok: false,
            error: { code: "forbidden", message: "expired", recovery: "login" },
          },
          { status: 401 },
        );
  });
  await client.login("synthetic browser password");
  const lost = new Promise<void>((resolve) => {
    client.onAuthenticationLost(resolve);
  });
  client.subscribe(() => undefined);
  await lost;
  await new Promise((resolve) => setTimeout(resolve, 550));
  expect(calls).toEqual(["/api/v1/login", "/api/v1/invalidations"]);
  await expect(client.read()).resolves.toMatchObject({ ok: false, error: { code: "forbidden" } });
  expect(calls).toHaveLength(2);
});

test("shared browser workspace client reads two projects and sends a named command", async () => {
  const projects = ["project.browser.a", "project.browser.b"];
  const calls: string[] = [];
  const client = createWorkspaceWebClient({
    baseUrl: "https://quicklens.test/quicklens/browser/",
    origin: "https://quicklens.test",
    fetcher: async (url, init) => {
      calls.push(`${init?.method}:${new URL(url).pathname}`);
      if (url.endsWith("api/v1/login"))
        return Response.json(
          { ok: true, value: { authenticated: true } },
          { headers: { "X-Quicklens-CSRF": "csrf.workspace" } },
        );
      if (url.endsWith("workspace/read"))
        return Response.json({
          ok: true,
          value: {
            operation: "project.list.v1",
            projects: projects.map((projectId) => ({
              projectId,
              displayName: projectId,
              repositoryRootRefs: [`repository.${projectId}`],
              defaultContextId: `context.${projectId}`,
              actions: {},
              revision: "1",
              createdAt: "2026-09-15T00:00:00.000Z",
              updatedAt: "2026-09-15T00:00:00.000Z",
            })),
          },
        });
      return Response.json({
        ok: true,
        value: {
          operation: "chat.post.v1",
          message: {
            messageId: "message.browser",
            projectId: projects[0],
            contextId: "context.browser.a",
            conversationId: "conversation.browser.a",
            senderActorId: null,
            role: "user",
            bodyMarkdown: "switch",
            artifactRefs: [],
            correlationId: null,
            causationMessageId: null,
            deliveryState: "persisted",
            revision: "1",
            createdAt: "2026-09-15T00:00:00.000Z",
            updatedAt: "2026-09-15T00:00:00.000Z",
          },
        },
      });
    },
  });
  expect(client).not.toBeNull();
  if (client === null) return;
  await expect(client.login("synthetic")).resolves.toMatchObject({ ok: true });
  const listed = await client.read({ operation: "project.list.v1" });
  expect(listed.ok).toBe(true);
  if (listed.ok && listed.value.operation === "project.list.v1")
    expect(listed.value.projects.map((project) => project.projectId)).toEqual(projects);
  await expect(
    client.command({
      operation: "chat.post.v1",
      clientRequestId: ClientRequestIdSchema.parse("request.browser.chat"),
      projectId: ProjectIdSchema.parse("project.browser.a"),
      contextId: WorkContextIdSchema.parse("context.browser.a"),
      conversationId: ConversationIdSchema.parse("conversation.browser.a"),
      bodyMarkdown: "switch",
      artifactRefs: [],
      correlationId: null,
      causationMessageId: null,
    }),
  ).resolves.toMatchObject({ ok: true });
  expect(calls.some((call) => call.includes("workspace/read"))).toBe(true);
  expect(calls.some((call) => call.includes("workspace/command"))).toBe(true);
});
