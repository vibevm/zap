/** Product HTTP error decoding. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import assert from "node:assert/strict";
import test from "node:test";

import { ClientRequestIdSchema } from "../protocol/index.ts";
import { createWorkspaceHttpConnection } from "./index.ts";

test("product client preserves a known registration conflict", async () => {
  const connection = createWorkspaceHttpConnection({
    baseUrl: "http://127.0.0.1:43121/quicklens/zap/",
    origin: "http://127.0.0.1:4174",
    fetcher: () =>
      Promise.resolve(
        Response.json(
          {
            ok: false,
            error: {
              code: "conflict",
              message: "registration request identity changed content",
            },
          },
          { status: 409 },
        ),
      ),
  });
  assert.notEqual(connection, null);
  if (connection === null) return;
  const result = await connection.product.request({
    operation: "product.project.register.v1",
    clientRequestId: ClientRequestIdSchema.parse("request.product.conflict"),
    directoryPath: "C:/fixture/project",
    displayName: "Fixture",
    profileId: "profile.fixture",
  });
  assert.deepEqual(result, {
    ok: false,
    error: { code: "conflict", message: "registration request identity changed content" },
  });
});
