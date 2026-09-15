/** @scope spec://org.vibevm.zap/lens/PROP-007#output-inspection */
/** Readable agent-output projection without inventing native message identity. */
import type { AgentOutputItem } from "../workspace-model/index.ts";

export interface AgentOutputGroup {
  readonly key: string;
  readonly kind: AgentOutputItem["kind"];
  readonly bodyMarkdown: string;
  readonly occurredAt: string;
  readonly artifactCount: number;
  readonly rawMetadata: readonly string[];
  readonly deltaCount: number;
}

export function groupAgentOutput(items: readonly AgentOutputItem[]): readonly AgentOutputGroup[] {
  const groups: AgentOutputGroup[] = [];
  for (const item of items) {
    const presented = readableOutput(item.bodyMarkdown);
    const previous = groups.at(-1);
    if (
      presented.delta &&
      previous !== undefined &&
      previous.kind === "text" &&
      previous.key === `${item.actorId}:${item.sessionId}:${item.runId ?? "none"}:delta`
    ) {
      groups[groups.length - 1] = {
        ...previous,
        bodyMarkdown: previous.bodyMarkdown + presented.text,
        occurredAt: item.occurredAt,
        artifactCount: previous.artifactCount + item.artifactRefs.length,
        rawMetadata: [...previous.rawMetadata, item.bodyMarkdown],
        deltaCount: previous.deltaCount + 1,
      };
      continue;
    }
    groups.push({
      key: `${item.actorId}:${item.sessionId}:${item.runId ?? "none"}:${presented.delta ? "delta" : item.sequence}`,
      kind: item.kind,
      bodyMarkdown: presented.text,
      occurredAt: item.occurredAt,
      artifactCount: item.artifactRefs.length,
      rawMetadata: presented.text === item.bodyMarkdown ? [] : [item.bodyMarkdown],
      deltaCount: presented.delta ? 1 : 0,
    });
  }
  return groups;
}

function readableOutput(value: string): { readonly text: string; readonly delta: boolean } {
  try {
    const parsed = parseJson(value);
    const delta = findText(parsed, "delta", 0);
    if (delta !== null) return { text: delta, delta: true };
    const text = findReadableText(parsed, 0);
    if (text !== null) return { text, delta: false };
  } catch {
    // Public output may be ordinary Markdown rather than a structured envelope.
  }
  return { text: value, delta: false };
}

function findReadableText(value: unknown, depth: number): string | null {
  if (depth > 4) return null;
  if (typeof value === "string") return value;
  if (Array.isArray(value)) {
    const parts = value
      .map((part) => findReadableText(part, depth + 1))
      .filter((part): part is string => part !== null);
    return parts.length === 0 ? null : parts.join("");
  }
  if (!isRecord(value)) return null;
  for (const key of ["text", "bodyMarkdown", "userMessage", "message", "content", "item"]) {
    const text = findReadableText(value[key], depth + 1);
    if (text !== null) return text;
  }
  return null;
}

function findText(value: unknown, key: string, depth: number): string | null {
  if (depth > 4 || !isRecord(value)) return null;
  const direct = value[key];
  if (typeof direct === "string") return direct;
  for (const nested of Object.values(value)) {
    if (Array.isArray(nested)) {
      for (const item of nested) {
        const found = findText(item, key, depth + 1);
        if (found !== null) return found;
      }
    } else {
      const found = findText(nested, key, depth + 1);
      if (found !== null) return found;
    }
  }
  return null;
}

function parseJson(text: string): unknown {
  return JSON.parse(text);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
