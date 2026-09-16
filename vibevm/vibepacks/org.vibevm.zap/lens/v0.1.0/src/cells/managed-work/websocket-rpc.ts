/** Minimal bounded authenticated JSON-RPC WebSocket client. @scope spec://org.vibevm.zap/lens/PROP-012#managed-control */
import { createHash, randomBytes } from "node:crypto";
import { request } from "node:http";
import type { Duplex } from "node:stream";
import { z } from "zod";
import type { ManagedWorkResult } from "./contracts.ts";

const MAX_MESSAGE_BYTES = 16 * 1024 * 1024;

export interface ManagedJsonRpcClient {
  request(method: string, params: unknown): Promise<ManagedWorkResult<unknown>>;
  notify(method: string, params: unknown): ManagedWorkResult<void>;
  subscribe(listener: (message: unknown) => void): () => void;
  close(): void;
}

export function connectManagedJsonRpc(input: {
  readonly url: string;
  readonly bearerToken: string;
  readonly timeoutMs: number;
}): Promise<ManagedWorkResult<ManagedJsonRpcClient>> {
  return new Promise((resolve) => {
    const target = new URL(input.url);
    const key = randomBytes(16).toString("base64");
    let settled = false;
    const finish = (result: ManagedWorkResult<ManagedJsonRpcClient>) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(result);
    };
    const timer = setTimeout(() => {
      finish(fail("unavailable", "Codex control WebSocket handshake timed out"));
    }, input.timeoutMs);
    const opening = request({
      host: target.hostname,
      port: target.port,
      path: `${target.pathname}${target.search}`,
      headers: {
        Connection: "Upgrade",
        Upgrade: "websocket",
        "Sec-WebSocket-Version": "13",
        "Sec-WebSocket-Key": key,
        Authorization: `Bearer ${input.bearerToken}`,
      },
    });
    opening.once("upgrade", (response, socket, head) => {
      const accept = response.headers["sec-websocket-accept"];
      const expected = createHash("sha1")
        .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`)
        .digest("base64");
      if (response.statusCode !== 101 || accept !== expected) {
        socket.destroy();
        finish(fail("unavailable", "Codex control WebSocket handshake was refused"));
        return;
      }
      const client = new SocketJsonRpc(socket, input.timeoutMs);
      if (head.length > 0) client.receive(head);
      finish({ ok: true, value: client });
    });
    opening.once("response", (response) => {
      response.resume();
      finish(
        fail("unavailable", `Codex control WebSocket returned ${String(response.statusCode)}`),
      );
    });
    opening.once("error", () => {
      finish(fail("unavailable", "Codex control WebSocket connection failed"));
    });
    opening.end();
  });
}

class SocketJsonRpc implements ManagedJsonRpcClient {
  readonly #socket: Duplex;
  readonly #timeoutMs: number;
  readonly #pending = new Map<
    number,
    {
      readonly finish: (result: ManagedWorkResult<unknown>) => void;
      readonly timer: NodeJS.Timeout;
    }
  >();
  readonly #listeners = new Set<(message: unknown) => void>();
  #nextId = 1;
  #buffer = Buffer.alloc(0);
  #fragment = Buffer.alloc(0);
  #closed = false;

  constructor(socket: Duplex, timeoutMs: number) {
    this.#socket = socket;
    this.#timeoutMs = timeoutMs;
    socket.on("data", (chunk: Buffer) => {
      this.receive(chunk);
    });
    socket.once("close", () => {
      this.#finishClosed();
    });
    socket.once("error", () => {
      this.#finishClosed();
    });
  }

  request(method: string, params: unknown): Promise<ManagedWorkResult<unknown>> {
    if (this.#closed) return Promise.resolve(fail("unavailable", "Codex control socket is closed"));
    const id = this.#nextId++;
    const sent = this.#send({ id, method, params });
    if (!sent.ok) return Promise.resolve(sent);
    return new Promise((resolve) => {
      let settled = false;
      const finish = (result: ManagedWorkResult<unknown>) => {
        if (settled) return;
        settled = true;
        const pending = this.#pending.get(id);
        if (pending !== undefined) clearTimeout(pending.timer);
        this.#pending.delete(id);
        resolve(result);
      };
      const timer = setTimeout(() => {
        finish(fail("uncertain", `Codex control request ${method} timed out`));
      }, this.#timeoutMs);
      this.#pending.set(id, { finish, timer });
    });
  }

  notify(method: string, params: unknown): ManagedWorkResult<void> {
    return this.#send({ method, params });
  }

  subscribe(listener: (message: unknown) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  receive(chunk: Buffer): void {
    if (this.#closed) return;
    this.#buffer = Buffer.concat([this.#buffer, chunk]);
    while (this.#consumeFrame()) {
      // Drain every complete frame currently buffered.
    }
  }

  close(): void {
    if (this.#closed) return;
    this.#writeFrame(0x8, Buffer.alloc(0));
    this.#socket.destroy();
    this.#finishClosed();
  }

  #consumeFrame(): boolean {
    if (this.#buffer.length < 2) return false;
    const first = this.#buffer.readUInt8(0);
    const second = this.#buffer.readUInt8(1);
    const final = (first & 0x80) !== 0;
    const opcode = first & 0x0f;
    const masked = (second & 0x80) !== 0;
    let length = second & 0x7f;
    let offset = 2;
    if (length === 126) {
      if (this.#buffer.length < 4) return false;
      length = this.#buffer.readUInt16BE(2);
      offset = 4;
    } else if (length === 127) {
      if (this.#buffer.length < 10) return false;
      const wide = this.#buffer.readBigUInt64BE(2);
      if (wide > BigInt(MAX_MESSAGE_BYTES)) {
        this.close();
        return false;
      }
      length = Number(wide);
      offset = 10;
    }
    if (length > MAX_MESSAGE_BYTES) {
      this.close();
      return false;
    }
    const maskBytes = masked ? 4 : 0;
    if (this.#buffer.length < offset + maskBytes + length) return false;
    const mask = masked ? this.#buffer.subarray(offset, offset + 4) : null;
    offset += maskBytes;
    const payload = Buffer.from(this.#buffer.subarray(offset, offset + length));
    this.#buffer = this.#buffer.subarray(offset + length);
    if (mask !== null)
      for (let index = 0; index < payload.length; index += 1)
        payload.writeUInt8(payload.readUInt8(index) ^ mask.readUInt8(index % 4), index);
    if (opcode === 0x8) {
      this.#finishClosed();
      return this.#buffer.length > 0;
    }
    if (opcode === 0x9) {
      this.#writeFrame(0x0a, payload);
      return true;
    }
    if (opcode !== 0x0 && opcode !== 0x1) return true;
    this.#fragment = Buffer.concat([this.#fragment, payload]);
    if (!final) return true;
    const message = this.#fragment.toString("utf8");
    this.#fragment = Buffer.alloc(0);
    this.#message(message);
    return true;
  }

  #message(text: string): void {
    let raw: unknown;
    try {
      raw = JSON.parse(text);
    } catch {
      return;
    }
    const response = z
      .object({
        id: z.number().int(),
        result: z.unknown().optional(),
        error: z.unknown().optional(),
      })
      .loose()
      .safeParse(raw);
    if (response.success) {
      const pending = this.#pending.get(response.data.id);
      if (pending === undefined) return;
      pending.finish(
        response.data.error === undefined
          ? { ok: true, value: response.data.result }
          : fail("unavailable", "Codex control request was refused"),
      );
      return;
    }
    for (const listener of this.#listeners) listener(raw);
  }

  #send(value: unknown): ManagedWorkResult<void> {
    try {
      this.#writeFrame(0x1, Buffer.from(JSON.stringify(value), "utf8"));
      return { ok: true, value: undefined };
    } catch {
      return fail("uncertain", "Codex control WebSocket write is uncertain");
    }
  }

  #writeFrame(opcode: number, payload: Buffer): void {
    const mask = randomBytes(4);
    const length = payload.length;
    const header = Buffer.alloc(length < 126 ? 2 : length <= 65_535 ? 4 : 10);
    header.writeUInt8(0x80 | opcode, 0);
    if (length < 126) header.writeUInt8(0x80 | length, 1);
    else if (length <= 65_535) {
      header.writeUInt8(0x80 | 126, 1);
      header.writeUInt16BE(length, 2);
    } else {
      header.writeUInt8(0x80 | 127, 1);
      header.writeBigUInt64BE(BigInt(length), 2);
    }
    const body = Buffer.from(payload);
    for (let index = 0; index < body.length; index += 1)
      body.writeUInt8(body.readUInt8(index) ^ mask.readUInt8(index % 4), index);
    this.#socket.write(Buffer.concat([header, mask, body]));
  }

  #finishClosed(): void {
    if (this.#closed) return;
    this.#closed = true;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.finish(fail("uncertain", "Codex control WebSocket closed"));
    }
    this.#pending.clear();
    this.#listeners.clear();
  }
}

function fail(code: "uncertain" | "unavailable", message: string): ManagedWorkResult<never> {
  return { ok: false, error: { code, message } };
}
