/** Stable, editable human-first names for execution configurations. @scope spec://org.vibevm.zap/lens/PROP-015#root */
import { ExecutionCatalogIdSchema } from "./types.ts";

export interface GeneratedExecutionNameInput {
  readonly agentProductName: string;
  readonly modelName: string;
  readonly accountName: string;
  readonly qualifier: string | null;
  readonly configurationId: string;
}

export function generateExecutionConfigurationName(
  input: GeneratedExecutionNameInput,
  existingNames: readonly string[],
): string {
  const configurationId = ExecutionCatalogIdSchema.parse(input.configurationId);
  const value = stableHash(configurationId);
  const first = FIRST_NAMES.at(value % FIRST_NAMES.length) ?? "Maya";
  const last = LAST_NAMES.at(Math.floor(value / FIRST_NAMES.length) % LAST_NAMES.length) ?? "Finch";
  const base = `${first} ${last}`;
  const used = new Set(existingNames.map((name) => name.toLocaleLowerCase()));
  for (let suffix = 1; suffix <= 100; suffix += 1) {
    const candidate = suffix === 1 ? base : `${base} ${String(((value + suffix) % 89) + 2)}`;
    if (!used.has(candidate.toLocaleLowerCase())) return candidate;
  }
  return `${base} ${configurationId.slice(-8)}`;
}

const FIRST_NAMES = ["Maya", "Nora", "Arden", "Robin", "Sasha", "Ellis", "Rowan", "Quinn"] as const;
const LAST_NAMES = ["Finch", "Vale", "Reed", "Stone", "Lane", "Brooks", "Wells", "Hart"] as const;

function stableHash(value: string): number {
  let hash = 2_166_136_261;
  for (const character of value) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16_777_619) >>> 0;
  }
  return hash;
}
