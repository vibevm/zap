import assert from "node:assert/strict";
import test from "node:test";
import { ProjectIdSchema } from "../workspace-model/index.ts";
import { DecimalSchema } from "../protocol/index.ts";
import { createWorkspaceWebClient } from "./web.ts";

test("browser workspace client sends same-origin CSRF and stops polling after 401", async () => {
  const calls: string[] = [];
  const client = createWorkspaceWebClient({
    baseUrl: "https://quicklens.test/quicklens/test/",
    origin: "https://quicklens.test",
    fetcher: async (input, init) => {
      calls.push(`${init?.method ?? "GET"}:${input}`);
      if (input.endsWith("api/v1/login"))
        return new Response(JSON.stringify({ ok: true, value: { authenticated: true } }), {
          status: 200,
          headers: { "Content-Type": "application/json", "X-Quicklens-CSRF": "csrf.synthetic" },
        });
      return new Response(
        JSON.stringify({ ok: false, error: { code: "forbidden", message: "session expired" } }),
        { status: 401, headers: { "Content-Type": "application/json" } },
      );
    },
  });
  assert.notEqual(client, null);
  if (client === null) return;
  const login = await client.login("synthetic");
  assert.equal(login.ok, true);
  const read = await client.read({ operation: "project.list.v1" });
  assert.equal(read.ok, false);
  const iterator = client.subscribe({
    cursor: {
      scope: { kind: "project", projectId: ProjectIdSchema.parse("project.web") },
      afterGlobalSequence: DecimalSchema.parse("0"),
    },
  });
  const result = await iterator[Symbol.asyncIterator]().next();
  assert.equal(result.done, true);
  assert.equal(calls.filter((call) => call.includes("workspace/events")).length, 0);
});
