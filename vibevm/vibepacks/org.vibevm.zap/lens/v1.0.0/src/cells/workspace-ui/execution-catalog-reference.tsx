/** Readable execution model reference table. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import { component$ } from "@qwik.dev/core";
import type {
  ExecutionConnectionRecord,
  ExecutionModelReferenceView,
} from "../execution-catalog/index.ts";

export const ExecutionReferenceTable = component$<{
  readonly references: readonly ExecutionModelReferenceView[];
}>((props) => (
  <details class="execution-reference-table">
    <summary>Advanced model reference library · {props.references.length} entries</summary>
    <p class="workspace-muted">
      This is background reference data, not a list of installed or active models. Only a named
      configuration shown above can be used for dispatch.
    </p>
    <div class="execution-reference-grid">
      {props.references.map((reference) => (
        <article key={reference.referenceId}>
          <strong>{humanModelName(reference.modelId)}</strong>
          <span>{humanFamilyName(reference.modelId)}</span>
          <small>
            {reference.specializations.length === 0
              ? "No routing preset"
              : reference.specializations.map((value) => value.replaceAll("_", " ")).join(", ")}
          </small>
          <small>{reference.availability.replaceAll("_", " ")}</small>
          <p>{reference.note}</p>
          {reference.sourceUrls.length === 0 ? (
            <small>No public source link recorded.</small>
          ) : (
            <span class="execution-reference-links">
              {reference.sourceUrls.map((url, index) => (
                <a key={url} href={url} target="_blank" rel="noreferrer">
                  Source {index + 1}
                </a>
              ))}
            </span>
          )}
        </article>
      ))}
    </div>
  </details>
));

export function preferredExecutionReference(
  agentProduct: ExecutionConnectionRecord["agentProduct"] | undefined,
  references: readonly ExecutionModelReferenceView[],
): string {
  const preferredModel =
    agentProduct === "codex"
      ? "gpt-5.6-luna"
      : agentProduct === "claude_code"
        ? "claude-opus-5"
        : agentProduct === "qwen_code"
          ? "qwen3.8-max"
          : agentProduct === "zap_mock"
            ? "zap-mock/deterministic-v1"
            : "gemini-3.8-flash";
  return (
    (
      references.find(
        (reference) => reference.conversationModel && reference.modelId === preferredModel,
      ) ?? references.find((reference) => reference.conversationModel)
    )?.referenceId ?? ""
  );
}

function humanModelName(modelId: string): string {
  return modelId.replaceAll("-", " ").replace("/", " · ");
}

function humanFamilyName(modelId: string): string {
  if (modelId.startsWith("gpt")) return "OpenAI GPT";
  if (modelId.startsWith("claude")) return "Anthropic Claude";
  if (modelId.startsWith("gemini")) return "Google Gemini";
  if (modelId.startsWith("qwen")) return "Qwen";
  if (modelId.startsWith("deepseek")) return "DeepSeek";
  if (modelId.startsWith("glm")) return "GLM";
  if (modelId.startsWith("mistral")) return "Mistral";
  if (modelId.startsWith("kimi")) return "Kimi";
  if (modelId.startsWith("meta-llama")) return "Meta Llama";
  if (modelId.startsWith("grok")) return "Grok";
  if (modelId.startsWith("zap-mock")) return "ZapMock synthetic";
  return "Catalog reference";
}
