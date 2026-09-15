/**
 * Bounded scrypt password verifier for the explicit remote Quicklens profile.
 * @scope spec://org.vibevm.zap/lens/PROP-003#password-authentication
 */
import { createHash, randomBytes, scrypt, timingSafeEqual } from "node:crypto";
import { spawnSync } from "node:child_process";
import { chmod, mkdir, open, readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir, userInfo } from "node:os";
import { dirname, join } from "node:path";
import { z } from "zod";

const ScryptCostSchema = z
  .object({
    N: z.number().int().min(1_024).max(262_144),
    r: z.number().int().min(1).max(32),
    p: z.number().int().min(1).max(16),
    keyLength: z.number().int().min(32).max(64),
    maxmem: z
      .number()
      .int()
      .min(16 * 1024 * 1024)
      .max(512 * 1024 * 1024),
  })
  .strict();
export const PasswordVerifierSchema = z
  .object({
    protocol: z.literal("quicklens-web-auth/1"),
    kdf: z.literal("scrypt"),
    salt: z
      .string()
      .min(22)
      .max(128)
      .regex(/^[A-Za-z0-9_-]+$/),
    derivedKey: z
      .string()
      .min(43)
      .max(128)
      .regex(/^[A-Za-z0-9_-]+$/),
    cost: ScryptCostSchema,
    createdAt: z.iso.datetime(),
  })
  .strict();
export type PasswordVerifier = z.infer<typeof PasswordVerifierSchema>;

export type WebAuthResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      readonly error: {
        readonly code: "invalid_input" | "unavailable" | "conflict";
        readonly message: string;
      };
    };

export interface PasswordVerification {
  readonly ok: boolean;
  readonly version: string;
}

export interface PasswordAuthenticator {
  verify(password: string): Promise<PasswordVerification>;
  version(): string;
  replace(verifier: PasswordVerifier): void;
  refresh(): Promise<boolean>;
}

export interface PasswordCostOptions {
  readonly N?: number;
  readonly r?: number;
  readonly p?: number;
  readonly keyLength?: number;
  readonly maxmem?: number;
}

const DEFAULT_COST = {
  N: 65_536,
  r: 8,
  p: 2,
  keyLength: 32,
  maxmem: 128 * 1024 * 1024,
};

/** @implements spec://org.vibevm.zap/lens/PROP-003#password-authentication */
export async function createPasswordVerifier(
  password: string,
  options: PasswordCostOptions = {},
): Promise<WebAuthResult<PasswordVerifier>> {
  const valid = passwordBytes(password);
  const cost = ScryptCostSchema.safeParse({ ...DEFAULT_COST, ...options });
  if (!valid.ok || !cost.success)
    return failure("invalid_input", "Password or scrypt bounds are invalid.");
  try {
    const salt = randomBytes(16);
    const derivedKey = await derive(valid.value, salt, cost.data);
    return {
      ok: true,
      value: PasswordVerifierSchema.parse({
        protocol: "quicklens-web-auth/1",
        kdf: "scrypt",
        salt: salt.toString("base64url"),
        derivedKey: derivedKey.toString("base64url"),
        cost: cost.data,
        createdAt: new Date().toISOString(),
      }),
    };
  } catch {
    return failure("unavailable", "Password verifier derivation failed.");
  }
}

export function createPasswordAuthenticator(
  verifier: PasswordVerifier,
  options: { readonly maximumConcurrent?: number; readonly allowWeakTestCost?: boolean } = {},
): WebAuthResult<PasswordAuthenticator> {
  const initial = PasswordVerifierSchema.safeParse(verifier);
  const maximum = options.maximumConcurrent ?? 2;
  if (
    !initial.success ||
    (!options.allowWeakTestCost && !productionCost(initial.data.cost)) ||
    !Number.isInteger(maximum) ||
    maximum < 1 ||
    maximum > 8
  ) {
    return failure("invalid_input", "Password authenticator bounds are invalid.");
  }
  let current = initial.data;
  const gate = new WorkGate(maximum);
  return {
    ok: true,
    value: {
      verify: async (password) => {
        const selected = current;
        const version = verifierVersion(selected);
        const result = await gate.run(async () => {
          const valid = passwordBytes(password);
          if (!valid.ok) return false;
          try {
            const expected = Buffer.from(selected.derivedKey, "base64url");
            const actual = await derive(
              valid.value,
              Buffer.from(selected.salt, "base64url"),
              selected.cost,
            );
            return expected.length === actual.length && timingSafeEqual(expected, actual);
          } catch {
            return false;
          }
        });
        return { ok: result ?? false, version };
      },
      version: () => verifierVersion(current),
      replace: (next) => {
        const parsed = PasswordVerifierSchema.parse(next);
        if (!options.allowWeakTestCost && !productionCost(parsed.cost)) {
          throw new Error(
            "violates REQ spec://org.vibevm.zap/lens/PROP-003#password-authentication: replacement verifier cost is below the production profile; fix surface: rotate with N=65536/r=8/p=2 or stronger",
          );
        }
        current = parsed;
      },
      refresh: () => Promise.resolve(true),
    },
  };
}

export async function loadPasswordAuthenticator(
  path: string,
  options: { readonly maximumConcurrent?: number; readonly allowWeakTestCost?: boolean } = {},
): Promise<WebAuthResult<PasswordAuthenticator>> {
  try {
    const raw: unknown = JSON.parse(await readFile(path, "utf8"));
    const verifier = PasswordVerifierSchema.safeParse(raw);
    if (!verifier.success) return failure("invalid_input", "Password verifier file is invalid.");
    const memory = createPasswordAuthenticator(verifier.data, options);
    if (!memory.ok) return memory;
    return {
      ok: true,
      value: {
        verify: (password) => memory.value.verify(password),
        version: () => memory.value.version(),
        replace: (next) => {
          memory.value.replace(next);
        },
        refresh: async () => {
          try {
            const candidate: unknown = JSON.parse(await readFile(path, "utf8"));
            const parsed = PasswordVerifierSchema.safeParse(candidate);
            if (
              !parsed.success ||
              (!options.allowWeakTestCost && !productionCost(parsed.data.cost))
            )
              return false;
            if (verifierVersion(parsed.data) !== memory.value.version())
              memory.value.replace(parsed.data);
            return true;
          } catch {
            return false;
          }
        },
      },
    };
  } catch {
    return failure("unavailable", "Password verifier file could not be read.");
  }
}

export function defaultPasswordVerifierPath(home: string = homedir()): string {
  return join(home, ".vibe", "zap", "quicklens-web-auth.json");
}

export async function writePasswordVerifier(input: {
  readonly path?: string;
  readonly password: string;
  readonly mode: "init" | "rotate";
  readonly cost?: PasswordCostOptions;
}): Promise<WebAuthResult<{ readonly path: string; readonly verifier: PasswordVerifier }>> {
  const path = input.path ?? defaultPasswordVerifierPath();
  const lockPath = `${path}.lock`;
  const temporary = `${path}.${randomBytes(8).toString("hex")}.tmp`;
  let lock: Awaited<ReturnType<typeof open>> | undefined;
  try {
    await mkdir(dirname(path), { recursive: true, mode: 0o700 });
    lock = await open(lockPath, "wx", 0o600);
    const existing = await readExisting(path);
    if (!existing.ok) return existing;
    if (
      (input.mode === "init" && existing.value !== null) ||
      (input.mode === "rotate" && existing.value === null)
    ) {
      return failure("conflict", `Password verifier cannot ${input.mode} in its current state.`);
    }
    const created = await createPasswordVerifier(input.password, input.cost);
    if (!created.ok) return created;
    await writeFile(temporary, `${JSON.stringify(created.value, null, 2)}\n`, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    });
    await protectVerifierFile(temporary);
    await rename(temporary, path);
    return { ok: true, value: { path, verifier: created.value } };
  } catch {
    return failure("conflict", "Password verifier update is locked or unavailable.");
  } finally {
    await rm(temporary, { force: true });
    if (lock !== undefined) {
      await lock.close();
      await rm(lockPath, { force: true });
    }
  }
}

class WorkGate {
  readonly #maximum: number;
  #active = 0;

  constructor(maximum: number) {
    this.#maximum = maximum;
  }

  async run<T>(work: () => Promise<T>): Promise<T | null> {
    if (this.#active >= this.#maximum) return null;
    this.#active += 1;
    try {
      return await work();
    } finally {
      this.#active -= 1;
    }
  }
}

function passwordBytes(password: string): WebAuthResult<Buffer> {
  const bytes = Buffer.from(password, "utf8");
  const codePoints = Array.from(password).length;
  return codePoints >= 15 && bytes.length <= 1_024 && password.trim().length > 0
    ? { ok: true, value: bytes }
    : failure(
        "invalid_input",
        "Password must contain at least 15 Unicode characters and at most 1024 UTF-8 bytes.",
      );
}

function derive(
  password: Buffer,
  salt: Buffer,
  cost: z.infer<typeof ScryptCostSchema>,
): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    scrypt(password, salt, cost.keyLength, cost, (error, key) => {
      if (error === null) resolve(key);
      else reject(error);
    });
  });
}

function verifierVersion(verifier: PasswordVerifier): string {
  return createHash("sha256").update(JSON.stringify(verifier)).digest("hex");
}

async function readExisting(path: string): Promise<WebAuthResult<PasswordVerifier | null>> {
  try {
    const raw: unknown = JSON.parse(await readFile(path, "utf8"));
    const parsed = PasswordVerifierSchema.safeParse(raw);
    return parsed.success
      ? { ok: true, value: parsed.data }
      : failure("conflict", "Existing password verifier is invalid.");
  } catch (error) {
    return isMissing(error)
      ? { ok: true, value: null }
      : failure("unavailable", "Existing password verifier could not be inspected.");
  }
}

function productionCost(cost: z.infer<typeof ScryptCostSchema>): boolean {
  return cost.r >= 8 && (cost.N >= 131_072 || (cost.N >= 65_536 && cost.p >= 2));
}

async function protectVerifierFile(path: string): Promise<void> {
  await chmod(path, 0o600);
  if (process.platform !== "win32") return;
  const acl = spawnSync(
    "icacls.exe",
    [path, "/inheritance:r", "/grant:r", `${userInfo().username}:(R,W)`],
    { windowsHide: true, stdio: "ignore" },
  );
  if (acl.status !== 0) {
    throw new Error(
      "violates REQ spec://org.vibevm.zap/lens/PROP-003#password-authentication: verifier ACL failed; fix surface: grant only the current user read/write access",
    );
  }
}

function isMissing(error: unknown): boolean {
  const parsed = z.looseObject({ code: z.unknown() }).safeParse(error);
  return parsed.success && parsed.data.code === "ENOENT";
}

function failure(
  code: "invalid_input" | "unavailable" | "conflict",
  message: string,
): WebAuthResult<never> {
  return { ok: false, error: { code, message } };
}
