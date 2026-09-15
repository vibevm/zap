/** @scope spec://org.vibevm.zap/lens/PROP-002#read-model */
import { LosslessNumber, isLosslessNumber, parse } from "lossless-json";
import { diagnostic } from "./diagnostics.ts";
import type { LosslessJsonValue, LosslessNumberText, U64 } from "./types.ts";

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });
const U64_MAX = 18_446_744_073_709_551_615n;
const I64_MIN = -9_223_372_036_854_775_808n;
const DECIMAL_INTEGER = /^-?(0|[1-9][0-9]*)$/;
const UNSIGNED_INTEGER = /^(0|[1-9][0-9]*)$/;

/** @implements spec://org.vibevm.zap/lens/PROP-002#read-model */
export function encodeCanonicalJson(value: unknown): Uint8Array {
  return encoder.encode(canonicalText(value, new Set<object>()));
}

/** Strict UTF-8 and duplicate-key parsing; numeric tokens stay lossless. */
export function parseWireJson(bytes: Uint8Array): unknown {
  const text = decoder.decode(bytes);
  assertNoDuplicateKeys(text);
  return parse(text);
}

/** Query item bytes must already equal codec-2 sorted canonical JSON. */
export function parseCanonicalJson(bytes: Uint8Array): unknown {
  const value = parseWireJson(bytes);
  const text = canonicalText(value, new Set<object>());
  if (!bytesEqual(bytes, encoder.encode(text))) {
    throw diagnostic("query item is not exact codec-2 canonical JSON");
  }
  return value;
}

export function normalizeLossless(value: unknown): LosslessJsonValue {
  if (value === null || typeof value === "boolean" || typeof value === "string") return value;
  if (isLosslessNumber(value)) return losslessNumberText(value.toString());
  if (Array.isArray(value)) return value.map(normalizeLossless);
  if (plainObject(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [key, normalizeLossless(child)]),
    );
  }
  throw diagnostic("value is outside lossless JSON");
}

/** Mirrors serde_json arbitrary-precision Value serialization inside a typed Rust frame. */
export function rustSerdeJsonValue(value: unknown): unknown {
  if (isLosslessNumber(value)) {
    const token = value.toString();
    if (!DECIMAL_INTEGER.test(token)) {
      throw diagnostic("Rust command payload numeric token is unsupported by codec 2");
    }
    return { "$serde_json::private::Number": token };
  }
  if (Array.isArray(value)) return value.map(rustSerdeJsonValue);
  if (plainObject(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [key, rustSerdeJsonValue(child)]),
    );
  }
  return value;
}

export function wireU64(value: unknown): U64 {
  if (!isLosslessNumber(value)) throw diagnostic("expected a JSON u64 number");
  const text = value.toString();
  if (!isU64(text)) {
    throw diagnostic("JSON number is outside u64");
  }
  return text;
}

export function wireU32(value: unknown): number {
  const text = wireU64(value);
  const parsed = BigInt(text);
  if (parsed > 4_294_967_295n) throw diagnostic("JSON number is outside u32");
  return Number(parsed);
}

export function byteArray(value: unknown): Uint8Array {
  if (!Array.isArray(value)) throw diagnostic("expected a JSON byte array");
  const bytes = value.map((item) => {
    const parsed = wireU32(item);
    if (parsed > 255) throw diagnostic("JSON byte is outside u8");
    return parsed;
  });
  return Uint8Array.from(bytes);
}

export function bytesToWire(bytes: Uint8Array): number[] {
  return [...bytes];
}

function canonicalText(value: unknown, ancestors: Set<object>): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "string") {
    if (!validUnicodeScalarString(value)) throw diagnostic("string contains an unpaired surrogate");
    return JSON.stringify(value);
  }
  if (isLosslessNumber(value)) return value.toString();
  if (typeof value === "bigint") {
    if (value < I64_MIN || value > U64_MAX) throw diagnostic("bigint is outside Rust JSON range");
    return value.toString();
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))) {
      throw diagnostic("number is not a finite safe JavaScript value");
    }
    if (Object.is(value, -0)) return "-0.0";
    return JSON.stringify(value);
  }
  if ((Array.isArray(value) || plainObject(value)) && ancestors.has(value)) {
    throw diagnostic("canonical JSON cannot contain cycles");
  }
  if (Array.isArray(value)) {
    ancestors.add(value);
    const items = Array.from({ length: value.length }, (_, index) => {
      if (!(index in value)) throw diagnostic("canonical JSON array contains a hole");
      return canonicalText(value[index], ancestors);
    });
    const result = `[${items.join(",")}]`;
    ancestors.delete(value);
    return result;
  }
  if (!plainObject(value)) throw diagnostic("value is outside canonical JSON");
  ancestors.add(value);
  const result = `{${Object.keys(value)
    .sort(compareUtf8)
    .map((key) => {
      if (!validUnicodeScalarString(key))
        throw diagnostic("object key contains an unpaired surrogate");
      const child: unknown = value[key];
      if (child === undefined) throw diagnostic("canonical JSON cannot contain undefined");
      return `${JSON.stringify(key)}:${canonicalText(child, ancestors)}`;
    })
    .join(",")}}`;
  ancestors.delete(value);
  return result;
}

function compareUtf8(left: string, right: string): number {
  const leftBytes = encoder.encode(left);
  const rightBytes = encoder.encode(right);
  const shared = Math.min(leftBytes.length, rightBytes.length);
  for (let index = 0; index < shared; index += 1) {
    const difference = (leftBytes[index] ?? 0) - (rightBytes[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return leftBytes.length - rightBytes.length;
}

function validUnicodeScalarString(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (Number.isNaN(next) || next < 0xdc00 || next > 0xdfff) return false;
      index += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return false;
    }
  }
  return true;
}

function plainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype: unknown = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function bytesEqual(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function assertNoDuplicateKeys(text: string): void {
  let offset = 0;
  const whitespace = (): void => {
    while (/\s/u.test(text[offset] ?? "")) offset += 1;
  };
  const stringToken = (): string => {
    const start = offset;
    offset += 1;
    while (offset < text.length) {
      if (text[offset] === "\\") {
        offset += 2;
      } else if (text[offset] === '"') {
        offset += 1;
        const parsed: unknown = JSON.parse(text.slice(start, offset));
        if (typeof parsed !== "string") throw diagnostic("JSON object key is not a string");
        return parsed;
      } else {
        offset += 1;
      }
    }
    throw diagnostic("unterminated JSON string");
  };
  const value = (): void => {
    whitespace();
    if (text[offset] === "{") {
      object();
    } else if (text[offset] === "[") {
      array();
    } else if (text[offset] === '"') {
      stringToken();
    } else {
      while (offset < text.length && !/[\s,}\]]/u.test(text[offset] ?? "")) offset += 1;
    }
    whitespace();
  };
  const object = (): void => {
    offset += 1;
    whitespace();
    const keys = new Set<string>();
    if (text[offset] === "}") {
      offset += 1;
      return;
    }
    while (offset < text.length) {
      if (text[offset] !== '"') throw diagnostic("JSON object key is missing");
      const key = stringToken();
      if (keys.has(key)) throw diagnostic("duplicate JSON object key");
      keys.add(key);
      whitespace();
      if (text[offset] !== ":") throw diagnostic("JSON object colon is missing");
      offset += 1;
      value();
      if (text[offset] === "}") {
        offset += 1;
        return;
      }
      if (text[offset] !== ",") throw diagnostic("JSON object comma is missing");
      offset += 1;
      whitespace();
    }
  };
  const array = (): void => {
    offset += 1;
    whitespace();
    if (text[offset] === "]") {
      offset += 1;
      return;
    }
    while (offset < text.length) {
      value();
      if (text[offset] === "]") {
        offset += 1;
        return;
      }
      if (text[offset] !== ",") throw diagnostic("JSON array comma is missing");
      offset += 1;
    }
  };
  value();
}

export function isCanonicalIntegerText(value: string): boolean {
  return DECIMAL_INTEGER.test(value);
}

function losslessNumberText(value: string): LosslessNumberText {
  if (!isCanonicalNumber(value)) throw diagnostic("JSON number text is invalid");
  return value;
}

function isCanonicalNumber(value: string): value is LosslessNumberText {
  try {
    new LosslessNumber(value);
    return true;
  } catch {
    return false;
  }
}

function isU64(value: string): value is U64 {
  return UNSIGNED_INTEGER.test(value) && BigInt(value) <= U64_MAX;
}
