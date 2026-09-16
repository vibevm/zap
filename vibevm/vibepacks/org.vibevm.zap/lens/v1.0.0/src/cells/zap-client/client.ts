/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import type { ZodType } from "zod";
import { createHash } from "node:crypto";
import {
  bytesToWire,
  encodeCanonicalJson,
  parseCanonicalJson,
  rustSerdeJsonValue,
} from "./codec.ts";
import {
  ActiveContextViewSchema,
  AdvanceChangeAdmissionInputSchema,
  CapabilitiesSchema,
  ChangeAdmissionViewSchema,
  EventPageSchema,
  PrepareBundleInputSchema,
  PrepareComparisonInputSchema,
  PrepareCompositeSuccessorInputSchema,
  PrepareProjectedInputSchema,
  PreparedBundleSchema,
  PreparedComparisonSchema,
  PreparedCompositeSuccessorSchema,
  PreparedCompositeSuccessorWireSchema,
  ProjectedRecordSchema,
  ProtectedSubmissionSchema,
  QueryPageWireSchema,
  ReconcileInputSchema,
  RecordedCompositeSuccessorWireSchema,
  SnapshotSchema,
  SubmissionStatusSchema,
  ZapIdSchema,
} from "./schemas.ts";
import { parseSse, Transport } from "./transport.ts";
import type {
  ActiveContextView,
  AdvanceChangeAdmissionRequest,
  CanonicalJsonInput,
  ChangeAdmissionView,
  EventCursor,
  PrepareBundleRequest,
  PrepareComparisonRequest,
  PrepareCompositeSuccessorRequest,
  PrepareProjectedRecordRequest,
  PreparedCompositeSuccessorView,
  ProtectedChannel,
  ProtectedSubmission,
  QueryInputWire,
  ReconcileRequest,
  RecordedCompositeSuccessorView,
  SubmissionStatus,
  ZapCapabilities,
  ZapClient,
  ZapClientResult,
  ZapCredential,
  ZapEventPage,
  ZapHttpExchange,
  ZapId,
  ZapQueryPage,
  ZapSnapshot,
  ZapStreamPage,
} from "./types.ts";

const DEFAULT_MAX_RESPONSE_BYTES = 1024 * 1024;

export interface ZapClientOptions {
  readonly endpoint: URL;
  readonly credential: ZapCredential;
  readonly exchange: ZapHttpExchange;
  readonly maximumResponseBytes?: number;
}

/** @implements spec://org.vibevm.zap/lens/PROP-002#plan-control */
export function createZapClient(options: ZapClientOptions): ZapClientResult<ZapClient> {
  const endpoint = validateEndpoint(options.endpoint);
  const credential = validateCredential(options.credential);
  const maximum = options.maximumResponseBytes ?? DEFAULT_MAX_RESPONSE_BYTES;
  if (!endpoint.ok) return endpoint;
  if (!credential.ok) return credential;
  if (!Number.isInteger(maximum) || maximum < 1024 || maximum > 64 * 1024 * 1024) {
    return failure("configuration", "maximum response bytes must be 1024..=67108864");
  }
  const transport = new Transport(endpoint.value, credential.value, options.exchange, maximum);
  return success<ZapClient>(new Client(transport));
}

export function canonicalQueryInput(value: CanonicalJsonInput): QueryInputWire {
  return { codec: 2, canonical_json: bytesToWire(encodeCanonicalJson(value)) };
}

export function protectedCommandDigest(command: ProtectedSubmission["command"]): string {
  const payload = parseCanonicalJson(Uint8Array.from(command.frame.payload.canonical_json));
  const bytes = encodeCanonicalJson({
    header: command.frame.header,
    payload: rustSerdeJsonValue(payload),
    reason: command.frame.reason,
  });
  return createHash("sha256").update(bytes).digest("hex");
}

class Client implements ZapClient {
  readonly #transport: Transport;

  constructor(transport: Transport) {
    this.#transport = transport;
  }

  capabilities(signal?: AbortSignal): Promise<ZapClientResult<ZapCapabilities>> {
    return this.#transport.get("/v1/capabilities", "capabilities", CapabilitiesSchema, signal);
  }

  snapshot(signal?: AbortSignal): Promise<ZapClientResult<ZapSnapshot>> {
    return this.#transport.get("/v1/snapshot", "snapshot", SnapshotSchema, signal);
  }

  async events(
    after: EventCursor | null,
    limit: number,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapEventPage>> {
    if (!validLimit(limit)) return invalidLimit();
    const result = await this.#transport.post(
      "/v1/events",
      { kind: "events", after: after === null ? null : cursorToWire(after), limit },
      "events",
      EventPageSchema,
      signal,
    );
    if (!result.ok) return result;
    return eventContextMatches(result.value, after)
      ? result
      : failure("stale_context", "event page cursor identity differs from its requested context");
  }

  async streamEvents(
    after: EventCursor | null,
    limit: number,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapStreamPage>> {
    if (!validLimit(limit)) return invalidLimit();
    const response = await this.#transport.raw(
      "/v1/stream",
      { kind: "events", after: after === null ? null : cursorToWire(after), limit },
      signal,
    );
    if (!response.ok) return response;
    const contentType = header(response.value.headers, "content-type") ?? "";
    if (!contentType.toLowerCase().startsWith("text/event-stream")) {
      return failure("malformed_response", "bounded stream response is not text/event-stream");
    }
    try {
      const page = parseSse(response.value.body);
      return after === null || cursorContextMatches(page.cursor, after)
        ? success(page)
        : failure("stale_context", "stream cursor identity differs from its requested context");
    } catch {
      return failure("malformed_response", "bounded stream response has invalid SSE framing");
    }
  }

  async query<T>(
    queryId: ZapId,
    input: CanonicalJsonInput,
    itemSchema: ZodType<T>,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapQueryPage<T>>> {
    const id = ZapIdSchema.safeParse(queryId);
    if (!id.success) return failure("configuration", "query ID is invalid");
    let queryInput: QueryInputWire;
    try {
      queryInput = canonicalQueryInput(input);
    } catch {
      return failure("configuration", "query input is not exact canonical JSON data");
    }
    const response = await this.#transport.post(
      "/v1/query",
      { kind: "query", query_id: id.data, input: queryInput },
      "query",
      QueryPageWireSchema,
      signal,
    );
    if (!response.ok) return response;
    const items: T[] = [];
    for (const bytes of response.value.items) {
      try {
        const parsed = itemSchema.safeParse(parseCanonicalJson(bytes));
        if (!parsed.success) return failure("malformed_response", "query item schema is invalid");
        items.push(parsed.data);
      } catch {
        return failure("malformed_response", "query item is not codec-2 canonical JSON");
      }
    }
    return success({ ...response.value, items });
  }

  async activeContext(
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ZapQueryPage<ActiveContextView>>> {
    const result = await this.query(
      ZapIdSchema.parse("zap.planning.active-context.v1"),
      {},
      ActiveContextViewSchema,
      signal,
    );
    if (!result.ok) return result;
    const item = result.value.items[0];
    if (result.value.completeness !== "complete" || result.value.items.length !== 1 || !item) {
      return failure("stale_context", "active-context query did not return one complete item");
    }
    if (
      item.snapshot.store_id !== result.value.store.store_id ||
      item.snapshot.campaign_id !== result.value.store.campaign_id ||
      item.snapshot.base_id !== result.value.store.base_id ||
      item.snapshot.revision !== result.value.revision
    ) {
      return failure("stale_context", "active-context item and query page identities differ");
    }
    return result;
  }

  async prepareBundle(request: PrepareBundleRequest, signal?: AbortSignal) {
    return this.preparation(
      "/v1/prepare/bundle",
      "prepare_effect_bundle",
      request,
      PrepareBundleInputSchema,
      "prepared_effect_bundle",
      PreparedBundleSchema,
      signal,
    );
  }

  async prepareComparison(request: PrepareComparisonRequest, signal?: AbortSignal) {
    return this.preparation(
      "/v1/prepare/comparison",
      "prepare_effect_comparison",
      request,
      PrepareComparisonInputSchema,
      "prepared_effect_comparison",
      PreparedComparisonSchema,
      signal,
    );
  }

  async prepareProjectedRecord(request: PrepareProjectedRecordRequest, signal?: AbortSignal) {
    return this.preparation(
      "/v1/prepare/projected-record",
      "prepare_projected_record",
      request,
      PrepareProjectedInputSchema,
      "projected_record",
      ProjectedRecordSchema,
      signal,
    );
  }

  async prepareCompositeSuccessor(
    request: PrepareCompositeSuccessorRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<PreparedCompositeSuccessorView>> {
    const parsed = PrepareCompositeSuccessorInputSchema.safeParse(request);
    if (!parsed.success) return failure("configuration", "composite successor request is invalid");
    const captured = await this.#transport.postCaptured(
      "/v1/prepare/composite-successor",
      { kind: "prepare_composite_successor", request: parsed.data },
      "prepared_composite_successor",
      PreparedCompositeSuccessorWireSchema,
      signal,
    );
    return captured.ok
      ? success({
          ...captured.value.value,
          replay: { codec: 2, canonical_json: bytesToWire(captured.value.canonicalValue) },
        })
      : captured;
  }

  async recordCompositeSuccessor(
    prepared: PreparedCompositeSuccessorView,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<RecordedCompositeSuccessorView>> {
    const parsed = PreparedCompositeSuccessorSchema.safeParse(prepared);
    if (!parsed.success) return failure("configuration", "prepared composite successor is invalid");
    let replay: unknown;
    try {
      replay = parseCanonicalJson(Uint8Array.from(parsed.data.replay.canonical_json));
      const checked = PreparedCompositeSuccessorWireSchema.parse(replay);
      if (!sameComposite(checked, parsed.data)) {
        return failure("configuration", "composite successor replay bytes differ from its view");
      }
    } catch {
      return failure("configuration", "composite successor replay bytes are invalid");
    }
    const result = await this.#transport.post(
      "/v1/composite-successor",
      { kind: "record_composite_successor", request: { prepared: replay } },
      "recorded_composite_successor",
      RecordedCompositeSuccessorWireSchema,
      signal,
    );
    if (!result.ok) {
      return result.error.kind === "http_refusal" ? result : uncertain(parsed.data.reconciliation);
    }
    if (
      !sameComposite(result.value.prepared, parsed.data) ||
      !submissionMatches(result.value.submission, parsed.data.reconciliation)
    ) {
      return foreign("composite successor", "different prepared or reconciliation identity");
    }
    return success({ prepared: parsed.data, submission: result.value.submission });
  }

  async submit(
    channel: ProtectedChannel,
    submission: ProtectedSubmission,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<SubmissionStatus>> {
    const parsed = ProtectedSubmissionSchema.safeParse(submission);
    if (!parsed.success) return failure("configuration", "protected submission is invalid");
    try {
      if (
        protectedCommandDigest(parsed.data.command) !== parsed.data.reconciliation.command_digest
      ) {
        return failure(
          "configuration",
          "reconciliation digest does not match the canonical command",
        );
      }
    } catch {
      return failure("configuration", "protected command payload is not codec-2 canonical JSON");
    }
    const result = await this.#transport.post(
      `/v1/${channel}`,
      { kind: channel, command: parsed.data.command },
      "command",
      SubmissionStatusSchema,
      signal,
    );
    if (!result.ok) {
      return result.error.kind === "http_refusal" ? result : uncertain(parsed.data.reconciliation);
    }
    return submissionMatches(result.value, parsed.data.reconciliation)
      ? result
      : uncertain(parsed.data.reconciliation);
  }

  async reconcile(
    request: ReconcileRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<SubmissionStatus>> {
    const parsed = ReconcileInputSchema.safeParse(request);
    if (!parsed.success) return failure("configuration", "reconciliation identity is invalid");
    const result = await this.#transport.post(
      "/v1/reconcile",
      { kind: "reconcile", request: parsed.data },
      "command",
      SubmissionStatusSchema,
      signal,
    );
    if (!result.ok) return result;
    return submissionMatches(result.value, parsed.data)
      ? result
      : foreign("reconciliation identity", "different command identity");
  }

  async advanceChangeAdmission(
    request: AdvanceChangeAdmissionRequest,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<ChangeAdmissionView>> {
    const parsed = AdvanceChangeAdmissionInputSchema.safeParse(request);
    if (!parsed.success) return failure("configuration", "change admission request is invalid");
    const result = await this.#transport.post(
      "/v1/change/admission",
      { kind: "advance_change_admission", request: parsed.data },
      "change_admission",
      ChangeAdmissionViewSchema,
      signal,
    );
    if (!result.ok && result.error.kind !== "http_refusal") {
      return {
        ok: false,
        error: {
          kind: "uncertain_operation",
          operation_id: parsed.data.operation_id,
          route: "/v1/change/admission",
          retry_exact: true,
        },
      };
    }
    return result;
  }

  private async preparation<I, O>(
    path: string,
    requestKind: string,
    request: I,
    inputSchema: ZodType<I>,
    responseKind: string,
    outputSchema: ZodType<O>,
    signal?: AbortSignal,
  ): Promise<ZapClientResult<O>> {
    const parsed = inputSchema.safeParse(request);
    if (!parsed.success) return failure("configuration", "preparation request is invalid");
    return this.#transport.post(
      path,
      { kind: requestKind, request: parsed.data },
      responseKind,
      outputSchema,
      signal,
    );
  }
}

function cursorToWire(cursor: EventCursor) {
  return {
    store: cursor.store,
    revision: BigInt(cursor.revision),
    next_sequence: BigInt(cursor.next_sequence),
  };
}

function eventContextMatches(page: ZapEventPage, after: EventCursor | null): boolean {
  if (!sameStore(page.store, page.resume.store) || page.revision !== page.resume.revision) {
    return false;
  }
  if (
    page.next !== null &&
    (!sameStore(page.store, page.next.store) || page.revision !== page.next.revision)
  ) {
    return false;
  }
  return after === null || cursorContextMatches(page.resume, after);
}

function cursorContextMatches(actual: EventCursor, expected: EventCursor): boolean {
  return sameStore(actual.store, expected.store) && actual.revision === expected.revision;
}

function sameStore(left: EventCursor["store"], right: EventCursor["store"]): boolean {
  return (
    left.store_id === right.store_id &&
    left.campaign_id === right.campaign_id &&
    left.base_id === right.base_id &&
    left.store_epoch === right.store_epoch &&
    left.codec_epoch === right.codec_epoch &&
    left.reducer_epoch === right.reducer_epoch
  );
}

function submissionMatches(status: SubmissionStatus, expected: ReconcileRequest): boolean {
  if (status.status === "committed") return status.receipt.command_id === expected.command_id;
  if (status.command_id !== expected.command_id) return false;
  return status.status !== "unknown" || status.command_digest === expected.command_digest;
}

function sameComposite(
  actual: Omit<PreparedCompositeSuccessorView, "replay">,
  expected: PreparedCompositeSuccessorView,
): boolean {
  const expectedValue = {
    request: expected.request,
    request_digest: expected.request_digest,
    plan: expected.plan,
    precursor_preparation: expected.precursor_preparation,
    reconciliation: expected.reconciliation,
  };
  const left = encodeCanonicalJson(actual);
  const right = encodeCanonicalJson(expectedValue);
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function uncertain(identity: ReconcileRequest): ZapClientResult<never> {
  return {
    ok: false,
    error: {
      kind: "uncertain_submission",
      command_id: identity.command_id,
      command_digest: identity.command_digest,
      reconcile_required: true,
    },
  };
}

function validateEndpoint(endpoint: URL): ZapClientResult<URL> {
  const host = endpoint.hostname.toLowerCase();
  if (
    endpoint.protocol !== "http:" ||
    !["127.0.0.1", "localhost", "[::1]"].includes(host) ||
    endpoint.username !== "" ||
    endpoint.password !== "" ||
    endpoint.search !== "" ||
    endpoint.hash !== "" ||
    endpoint.pathname !== "/"
  ) {
    return failure("configuration", "ZAP endpoint must be an uncredentialed loopback HTTP root");
  }
  return success(new URL(endpoint.href));
}

function validateCredential(credential: ZapCredential): ZapClientResult<ZapCredential> {
  const id = ZapIdSchema.safeParse(credential.id);
  const bearer = credential.bearer;
  if (!id.success || bearer.length === 0 || bearer.length > 4096 || containsHeaderControl(bearer)) {
    return failure("configuration", "ZAP credential ID or bearer is invalid");
  }
  return success({ id: id.data, bearer });
}

function containsHeaderControl(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code <= 31 || code === 127) return true;
  }
  return false;
}

function validLimit(value: number): boolean {
  return Number.isInteger(value) && value > 0 && value <= 4_294_967_295;
}

function invalidLimit<T>(): ZapClientResult<T> {
  return failure("configuration", "event limit must be a positive u32");
}

function header(headers: Readonly<Record<string, string>>, name: string): string | undefined {
  const match = Object.entries(headers).find(([key]) => key.toLowerCase() === name);
  return match?.[1];
}

function success<T>(value: T): ZapClientResult<T> {
  return { ok: true, value };
}

type MessageErrorKind = "configuration" | "transport" | "malformed_response" | "stale_context";

function failure(kind: MessageErrorKind, message: string): ZapClientResult<never> {
  switch (kind) {
    case "configuration":
      return { ok: false, error: { kind: "configuration", message } };
    case "transport":
      return { ok: false, error: { kind: "transport", message } };
    case "malformed_response":
      return { ok: false, error: { kind: "malformed_response", message } };
    case "stale_context":
      return { ok: false, error: { kind: "stale_context", message } };
  }
}

function foreign(expected: string, received: string): ZapClientResult<never> {
  return { ok: false, error: { kind: "foreign_response", expected, received } };
}
