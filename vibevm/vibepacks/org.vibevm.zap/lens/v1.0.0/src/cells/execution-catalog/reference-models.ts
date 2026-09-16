/** Dated model-family reference facts and editable seed heuristics. @scope spec://org.vibevm.zap/lens/PROP-015#recommendations */
import type { TaskSpecialization } from "./types.ts";

export interface ExecutionModelReference {
  readonly familyId: string;
  readonly vendor: string;
  readonly modelId: string;
  readonly status: "current" | "preview" | "metadata_incomplete" | "open_weight" | "synthetic";
  readonly conversationModel: boolean;
  readonly launchAvailability:
    | "verified_profile_required"
    | "catalog_only"
    | "deployment_required"
    | "tool_only";
  readonly inputModalities: readonly ("text" | "image" | "audio" | "video" | "pdf")[];
  readonly outputModalities: readonly ("text" | "image")[];
  readonly documentedContextMaximumTokens: number | null;
  readonly maximumOutputTokens: number | null;
  readonly effort: {
    readonly control: string;
    readonly values: readonly string[];
    readonly defaultValue: string | null;
  };
  readonly toolCapabilities: readonly string[];
  readonly presets: readonly {
    readonly specialization: TaskSpecialization;
    readonly suitability: number;
    readonly quality: number;
    readonly economy: number;
    readonly preferenceOrder: number;
    readonly rationale: string;
    readonly provenance: "owner" | "reference_preset" | "synthetic_fixture";
  }[];
  readonly sourceUrls: readonly string[];
  readonly note: string;
}

export const EXECUTION_MODEL_REFERENCE_AS_OF = "2026-09-16";

const TEXT_SPECIALIZATIONS: readonly TaskSpecialization[] = [
  "general",
  "architecture",
  "backend",
  "web_ui",
  "testing",
  "code_review",
  "research",
  "documentation",
  "data_analysis",
  "automation",
];

export const EXECUTION_MODEL_REFERENCE: readonly ExecutionModelReference[] = [
  {
    familyId: "zap_mock",
    vendor: "Zap",
    modelId: "zap-mock/deterministic-v1",
    status: "synthetic",
    conversationModel: true,
    launchAvailability: "verified_profile_required",
    inputModalities: ["text"],
    outputModalities: ["text"],
    documentedContextMaximumTokens: null,
    maximumOutputTokens: null,
    effort: { control: "unsupported", values: [], defaultValue: null },
    toolCapabilities: ["zap_local_protocol"],
    presets: baselineScores([], 50, 100).map((entry) => ({
      ...entry,
      provenance: "synthetic_fixture",
      rationale: "Synthetic deterministic fixture; never a real-provider fallback.",
    })),
    sourceUrls: ["spec://org.vibevm.zap/lens/PROP-013#model"],
    note: "Explicit zero-LLM development fixture with synthetic provenance.",
  },
  openAiText("gpt-6-astra", ["low", "medium", "high", "xhigh", "max"], null, [
    score("architecture", 98, 100, 5, 0, "OpenAI frontier reference for demanding work."),
    score("code_review", 98, 100, 5, 0, "OpenAI frontier reference for demanding review."),
  ]),
  openAiText(
    "gpt-5.6-sol",
    ["none", "low", "medium", "high", "xhigh", "max"],
    "medium",
    [
      score("backend", 94, 90, 45, 0, "Reliable coding reference."),
      score("web_ui", 90, 88, 45, 10, "Capable UI controller; owner prefers Claude Opus 5 first."),
      {
        ...score(
          "image_generation",
          100,
          92,
          40,
          0,
          "Owner-selected image workflow controller; requires an authorized image-generation tool.",
        ),
        provenance: "owner",
      },
    ],
    ["image_generation_tool"],
  ),
  openAiText("gpt-5.6-terra", ["none", "low", "medium", "high", "xhigh", "max"], "medium", [
    score("testing", 88, 78, 70, 0, "Balanced OpenAI execution reference."),
  ]),
  openAiText("gpt-5.6-luna", ["none", "low", "medium", "high", "xhigh", "max"], "medium", [
    score("general", 82, 65, 96, 0, "Economy-oriented OpenAI reference."),
    score("automation", 84, 66, 96, 0, "Economy-oriented automation reference."),
  ]),
  {
    familyId: "openai_gpt",
    vendor: "OpenAI",
    modelId: "gpt-image-2.5-sunburst",
    status: "current",
    conversationModel: false,
    launchAvailability: "tool_only",
    inputModalities: ["text", "image"],
    outputModalities: ["image"],
    documentedContextMaximumTokens: null,
    maximumOutputTokens: null,
    effort: {
      control: "image_quality",
      values: ["low", "medium", "high", "xhigh", "max", "auto"],
      defaultValue: "auto",
    },
    toolCapabilities: ["image_generation"],
    presets: [
      score(
        "image_generation",
        100,
        100,
        25,
        0,
        "OpenAI image-tool reference; never a coding conversation model.",
      ),
    ],
    sourceUrls: ["https://developers.openai.com/api/docs/models/gpt-image-2.5-sunburst"],
    note: "Use behind an authorized image tool. The owner-selected conversation controller is gpt-5.6-sol.",
  },
  {
    familyId: "openai_gpt",
    vendor: "OpenAI",
    modelId: "gpt-image-2.5-flare",
    status: "current",
    conversationModel: false,
    launchAvailability: "tool_only",
    inputModalities: ["text", "image"],
    outputModalities: ["image"],
    documentedContextMaximumTokens: null,
    maximumOutputTokens: null,
    effort: {
      control: "image_quality",
      values: ["low", "medium", "high", "xhigh", "max", "auto"],
      defaultValue: "auto",
    },
    toolCapabilities: ["image_generation"],
    presets: [score("image_generation", 95, 85, 70, 10, "OpenAI image-tool economy reference.")],
    sourceUrls: ["https://developers.openai.com/api/docs/models/gpt-image-2.5-flare"],
    note: "Use behind an authorized image tool, not as a coding conversation model.",
  },
  {
    familyId: "anthropic_claude",
    vendor: "Anthropic",
    modelId: "claude-opus-5",
    status: "current",
    conversationModel: true,
    launchAvailability: "verified_profile_required",
    inputModalities: ["text", "image"],
    outputModalities: ["text"],
    documentedContextMaximumTokens: 1_000_000,
    maximumOutputTokens: 128_000,
    effort: {
      control: "output_config.effort",
      values: ["low", "medium", "high", "xhigh", "max"],
      defaultValue: "high",
    },
    toolCapabilities: ["function_calling"],
    presets: baselineScores(
      [
        {
          ...score("web_ui", 100, 98, 45, 0, "Owner-preferred web UI configuration."),
          provenance: "owner",
        },
        score("architecture", 97, 98, 35, 5, "Vendor positions Opus 5 for complex agentic work."),
      ],
      92,
      40,
    ),
    sourceUrls: ["https://platform.claude.com/docs/en/models/opus-5/whats-new-opus-5"],
    note: "1M is both default and maximum; thinking cannot be disabled at xhigh or max.",
  },
  reference(
    "anthropic_claude",
    "Anthropic",
    "claude-haiku-4-5-20251001",
    200_000,
    64_000,
    [],
    null,
    ["text", "image"],
    ["function_calling"],
    [score("automation", 88, 72, 94, 0, "Fast current Claude reference for bounded work.")],
    "https://platform.claude.com/docs/en/models/overview",
    "verified_profile_required",
  ),
  reference(
    "google_gemini",
    "Google",
    "gemini-3.8-flash",
    1_048_576,
    65_536,
    ["low", "medium", "high"],
    "medium",
    ["text", "image", "video", "audio", "pdf"],
    ["function_calling", "code_execution", "search_grounding"],
    [score("research", 94, 88, 62, 0, "Multimodal long-context research reference.")],
    "https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash",
  ),
  reference(
    "alibaba_qwen",
    "Alibaba Cloud / Qwen",
    "qwen3.8-max",
    1_000_000,
    131_072,
    ["none", "low", "medium", "xhigh"],
    "xhigh",
    ["text", "image", "video"],
    ["function_calling", "structured_outputs"],
    [score("data_analysis", 94, 91, 52, 0, "Qwen flagship reasoning reference.")],
    "https://help.aliyun.com/en/model-studio/qwen3-8-max",
    "verified_profile_required",
  ),
  reference(
    "deepseek",
    "DeepSeek",
    "deepseek-flash",
    1_000_000,
    384_000,
    ["none", "low", "high", "max"],
    "high",
    ["text", "image"],
    ["function_calling", "structured_outputs"],
    [score("backend", 92, 86, 90, 5, "DeepSeek fast agent reference.")],
    "https://api-docs.deepseek.com/quick_start/pricing/",
  ),
  reference(
    "zai_glm",
    "Z.AI",
    "glm-5.1",
    200_000,
    128_000,
    ["disabled", "enabled"],
    "enabled",
    ["text"],
    ["function_calling", "mcp", "structured_outputs"],
    [score("automation", 93, 87, 82, 5, "Vendor long-horizon agentic engineering reference.")],
    "https://docs.z.ai/guides/llm/glm-5.1",
  ),
  reference(
    "mistral",
    "Mistral AI",
    "mistral-medium-3-5",
    256_000,
    null,
    ["none", "high"],
    null,
    ["text", "image"],
    ["function_calling", "structured_outputs"],
    [score("documentation", 89, 84, 58, 10, "Multimodal agentic reference.")],
    "https://docs.mistral.ai/models/mistral-medium-3-5-26-04",
  ),
  reference(
    "moonshot_kimi",
    "Moonshot AI / Kimi",
    "kimi-k3",
    1_000_000,
    1_048_576,
    ["low", "high", "max"],
    "max",
    ["text", "image", "video"],
    ["function_calling", "dynamic_tool_loading", "structured_outputs"],
    [score("research", 95, 92, 55, 5, "Long-context knowledge-work reference.")],
    "https://platform.kimi.ai/docs/guide/kimi-k3-quickstart",
  ),
  reference(
    "meta_llama",
    "Meta",
    "meta-llama/Llama-4-Maverick-17B-128E-Instruct",
    1_000_000,
    null,
    [],
    null,
    ["text", "image"],
    ["prompt_function_definitions"],
    [
      score(
        "general",
        82,
        80,
        60,
        20,
        "Open-weight multimodal reference; economics depend on deployment.",
      ),
    ],
    "https://github.com/meta-llama/llama-models/blob/main/models/llama4/MODEL_CARD.md",
    "deployment_required",
    "open_weight",
  ),
  reference(
    "xai_grok",
    "xAI",
    "grok-4.6",
    500_000,
    null,
    ["low", "medium", "high", "xhigh"],
    "high",
    ["text", "image"],
    ["function_calling", "web_search", "x_search", "code_execution"],
    [score("general", 91, 90, 48, 10, "Vendor coding, agentic and knowledge-work reference.")],
    "https://docs.x.ai/developers/grok-4-6",
  ),
];

function openAiText(
  modelId: string,
  values: readonly string[],
  defaultValue: string | null,
  presets: ExecutionModelReference["presets"],
  extraTools: readonly string[] = [],
): ExecutionModelReference {
  return {
    familyId: "openai_gpt",
    vendor: "OpenAI",
    modelId,
    status: "current",
    conversationModel: true,
    launchAvailability: "verified_profile_required",
    inputModalities: ["text", "image"],
    outputModalities: ["text"],
    documentedContextMaximumTokens: 1_050_000,
    maximumOutputTokens: 128_000,
    effort: { control: "reasoning.effort", values, defaultValue },
    toolCapabilities: ["function_calling", ...extraTools],
    presets: baselineScores(presets, presets[0]?.quality ?? 70, presets[0]?.economy ?? 50),
    sourceUrls: [`https://developers.openai.com/api/docs/models/${modelId}`],
    note: "Model maximum; applied Codex host context and adapter-supported effort are separate evidence.",
  };
}

function reference(
  familyId: string,
  vendor: string,
  modelId: string,
  context: number,
  output: number | null,
  effortValues: readonly string[],
  defaultEffort: string | null,
  inputModalities: ExecutionModelReference["inputModalities"],
  toolCapabilities: readonly string[],
  presets: ExecutionModelReference["presets"],
  source: string,
  launchAvailability: ExecutionModelReference["launchAvailability"] = "catalog_only",
  status: ExecutionModelReference["status"] = "current",
): ExecutionModelReference {
  return {
    familyId,
    vendor,
    modelId,
    status,
    conversationModel: true,
    launchAvailability,
    inputModalities,
    outputModalities: ["text"],
    documentedContextMaximumTokens: context,
    maximumOutputTokens: output,
    effort: {
      control: effortValues.length === 0 ? "no_family_standard" : "provider_native",
      values: effortValues,
      defaultValue: defaultEffort,
    },
    toolCapabilities,
    presets: baselineScores(presets, presets[0]?.quality ?? 70, presets[0]?.economy ?? 50),
    sourceUrls: [source],
    note: "Reference metadata does not enable an account, profile, executable or Zap adapter.",
  };
}

function score(
  specialization: TaskSpecialization,
  suitability: number,
  quality: number,
  economy: number,
  preferenceOrder: number,
  rationale: string,
): ExecutionModelReference["presets"][number] {
  return {
    specialization,
    suitability,
    quality,
    economy,
    preferenceOrder,
    rationale,
    provenance: "reference_preset",
  };
}

function baselineScores(
  overrides: ExecutionModelReference["presets"],
  quality: number,
  economy: number,
): ExecutionModelReference["presets"] {
  const bySpecialization = new Map(overrides.map((entry) => [entry.specialization, entry]));
  return [
    ...TEXT_SPECIALIZATIONS.map(
      (specialization) =>
        bySpecialization.get(specialization) ??
        score(
          specialization,
          70,
          quality,
          economy,
          100,
          "Editable baseline for ordinary text work; no vendor superiority claim.",
        ),
    ),
    ...overrides.filter((entry) => entry.specialization === "image_generation"),
  ];
}
