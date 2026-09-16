/** Durable trusted setup catalog. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { z } from "zod";
import { ProductPlanContextSchema, ProductProjectSchema } from "../workspace-model/index.ts";
import {
  TrustedPlanContextRegistrationSchema,
  TrustedProjectRegistrationSchema,
  type TrustedPlanContextRegistration,
  type TrustedProjectRegistration,
} from "../workspace-store/index.ts";

const PlanEntrySchema = z
  .object({
    context: ProductPlanContextSchema,
    registration: TrustedPlanContextRegistrationSchema,
    requestDigest: z.string().length(64),
  })
  .strict();
export type ProductPlanContextEntry = z.infer<typeof PlanEntrySchema>;

const EntrySchema = z
  .object({
    project: ProductProjectSchema,
    registration: TrustedProjectRegistrationSchema,
    requestDigest: z.string().length(64),
    plans: z.array(PlanEntrySchema).max(256).default([]),
  })
  .strict();
export type ProductProjectEntry = z.infer<typeof EntrySchema>;
const StateSchema = z
  .object({ version: z.literal(1), entries: z.array(EntrySchema).max(256) })
  .strict();

export interface ProductAppRegistry {
  entries(): readonly ProductProjectEntry[];
  findByRequest(registrationId: string): ProductProjectEntry | null;
  findByDirectory(directoryPath: string): ProductProjectEntry | null;
  findPlanByRequest(registrationId: string): ProductPlanContextEntry | null;
  put(entry: ProductProjectEntry): ProductStoreResult<ProductProjectEntry>;
  putPlan(
    projectId: string,
    entry: ProductPlanContextEntry,
  ): ProductStoreResult<ProductPlanContextEntry>;
}

export type ProductStoreResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly message: string };

export function openProductAppRegistry(path: string): ProductStoreResult<ProductAppRegistry> {
  const file = resolve(path);
  let state: z.infer<typeof StateSchema> = { version: 1, entries: [] };
  try {
    mkdirSync(dirname(file), { recursive: true });
    try {
      const parsed: unknown = JSON.parse(readFileSync(file, "utf8"));
      const checked = StateSchema.safeParse(parsed);
      if (!checked.success) return failure("product setup catalog is invalid");
      state = checked.data;
    } catch (error) {
      if (!missing(error)) return failure("product setup catalog could not be read");
    }
  } catch {
    return failure("product setup catalog directory is unavailable");
  }
  const persist = (): ProductStoreResult<null> => {
    const temporary = `${file}.next`;
    try {
      writeFileSync(temporary, JSON.stringify(state), { encoding: "utf8", mode: 0o600 });
      renameSync(temporary, file);
      return { ok: true, value: null };
    } catch {
      return failure("product setup catalog could not be persisted");
    }
  };
  return {
    ok: true,
    value: {
      entries: () => [...state.entries],
      findByRequest: (registrationId) =>
        state.entries.find((entry) => entry.registration.registrationId === registrationId) ?? null,
      findByDirectory: (directoryPath) =>
        state.entries.find((entry) => entry.project.directoryPath === directoryPath) ?? null,
      findPlanByRequest: (registrationId) =>
        state.entries
          .flatMap((entry) => entry.plans)
          .find((entry) => entry.registration.registrationId === registrationId) ?? null,
      put: (input) => {
        const parsed = EntrySchema.safeParse(input);
        if (!parsed.success) return failure("product project entry is invalid");
        const prior = state.entries.find(
          (entry) => entry.registration.registrationId === parsed.data.registration.registrationId,
        );
        if (prior !== undefined)
          return prior.requestDigest === parsed.data.requestDigest
            ? { ok: true, value: prior }
            : failure("registration request identity changed content");
        if (state.entries.length >= 256) return failure("product project limit is reached");
        state = { version: 1, entries: [...state.entries, parsed.data] };
        const saved = persist();
        return saved.ok ? { ok: true, value: parsed.data } : saved;
      },
      putPlan: (projectId, input) => {
        const parsed = PlanEntrySchema.safeParse(input);
        if (!parsed.success) return failure("product plan context entry is invalid");
        const projectIndex = state.entries.findIndex(
          (entry) => entry.project.projectId === projectId,
        );
        const project = state.entries[projectIndex];
        if (project === undefined) return failure("product project is unavailable");
        const prior = state.entries
          .flatMap((entry) => entry.plans)
          .find(
            (entry) =>
              entry.registration.registrationId === parsed.data.registration.registrationId,
          );
        if (prior !== undefined)
          return prior.requestDigest === parsed.data.requestDigest
            ? { ok: true, value: prior }
            : failure("plan context request identity changed content");
        if (
          project.plans.some(
            (entry) =>
              entry.context.planId === parsed.data.context.planId ||
              entry.context.contextId === parsed.data.context.contextId,
          )
        )
          return failure("plan or context identity is already registered");
        const nextProject = ProductProjectSchema.parse({
          ...project.project,
          planContexts: [...project.project.planContexts, parsed.data.context],
        });
        const nextEntry = {
          ...project,
          project: nextProject,
          plans: [...project.plans, parsed.data],
        };
        state = {
          version: 1,
          entries: state.entries.map((entry, index) =>
            index === projectIndex ? nextEntry : entry,
          ),
        };
        const saved = persist();
        return saved.ok ? { ok: true, value: parsed.data } : saved;
      },
    },
  };
}

function missing(error: unknown): boolean {
  return typeof error === "object" && error !== null && Reflect.get(error, "code") === "ENOENT";
}

function failure(message: string): ProductStoreResult<never> {
  return { ok: false, message };
}

export function registrationOf(entry: ProductProjectEntry): TrustedProjectRegistration {
  return entry.registration;
}

export function planRegistrationOf(entry: ProductPlanContextEntry): TrustedPlanContextRegistration {
  return entry.registration;
}
