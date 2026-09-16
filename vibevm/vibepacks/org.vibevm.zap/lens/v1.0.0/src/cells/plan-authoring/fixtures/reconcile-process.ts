import { readFile } from "node:fs/promises";
import { z } from "zod";
import {
  ZapDigestSchema,
  ZapIdSchema,
  createZapClient,
  type ZapHttpExchange,
} from "../../zap-client/index.ts";

const InputSchema = z
  .object({
    fixture: z.string(),
    reconciliation: z.object({ command_id: ZapIdSchema, command_digest: ZapDigestSchema }).strict(),
  })
  .strict();
const FixtureSchema = z.looseObject({
  endpoint_file: z.string(),
  credentials: z.object({ reader: z.object({ id: z.string(), file: z.string() }).strict() }),
});
const EndpointSchema = z.looseObject({ address: z.string() });

const inputPath = process.argv[2];
if (inputPath === undefined) process.exitCode = 2;
else {
  const input = InputSchema.parse(JSON.parse(await readFile(inputPath, "utf8")));
  const fixture = FixtureSchema.parse(JSON.parse(await readFile(input.fixture, "utf8")));
  const endpoint = EndpointSchema.parse(JSON.parse(await readFile(fixture.endpoint_file, "utf8")));
  const client = createZapClient({
    endpoint: new URL(`http://${endpoint.address}/`),
    credential: {
      id: ZapIdSchema.parse(fixture.credentials.reader.id),
      bearer: await readFile(fixture.credentials.reader.file, "utf8"),
    },
    exchange: httpExchange(),
  });
  if (!client.ok) process.exitCode = 2;
  else {
    const result = await client.value.reconcile(input.reconciliation);
    if (result.ok && result.value.status === "committed") {
      process.stdout.write(
        JSON.stringify({
          ok: true,
          commandId: result.value.receipt.command_id,
          revision: result.value.receipt.revision,
        }),
      );
      process.exitCode = 0;
    } else {
      process.stdout.write(JSON.stringify({ ok: false }));
      process.exitCode = 1;
    }
  }
}

function httpExchange(): ZapHttpExchange {
  return {
    request: async (input) => {
      const response = await fetch(input.url, {
        method: input.method,
        headers: input.headers,
        ...(input.body === undefined ? {} : { body: Buffer.from(input.body) }),
        ...(input.signal === undefined ? {} : { signal: input.signal }),
      });
      const headers: Record<string, string> = {};
      response.headers.forEach((value, key) => {
        headers[key] = value;
      });
      return {
        status: response.status,
        headers,
        body: new Uint8Array(await response.arrayBuffer()),
      };
    },
  };
}
