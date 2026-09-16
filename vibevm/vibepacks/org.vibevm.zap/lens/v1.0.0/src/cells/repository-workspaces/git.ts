/** Argv-only local Git execution. @scope spec://org.vibevm.zap/lens/PROP-014#preparation */
import { spawn } from "node:child_process";

export interface GitCommand {
  readonly cwd: string;
  readonly args: readonly string[];
  readonly environment?: Readonly<Record<string, string>>;
}
export interface GitCommandResult {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
}
export interface RepositoryGitPort {
  run(command: GitCommand): Promise<GitCommandResult>;
}

export interface GitArgvAdapterOptions {
  readonly executablePath?: string;
  readonly timeoutMs?: number;
  readonly maxOutputBytes?: number;
}

export function createGitArgvAdapter(options: GitArgvAdapterOptions = {}): RepositoryGitPort {
  const executable = options.executablePath ?? "git";
  const timeoutMs = options.timeoutMs ?? 30_000;
  const maxOutput = options.maxOutputBytes ?? 2_000_000;
  return {
    run(command) {
      return new Promise((resolve) => {
        const child = spawn(executable, ["-c", "core.longpaths=true", ...command.args], {
          cwd: command.cwd,
          env: { ...safeGitEnvironment(), ...command.environment },
          shell: false,
          windowsHide: true,
          stdio: ["ignore", "pipe", "pipe"],
        });
        let stdout = "";
        let stderr = "";
        let overflow = false;
        const append = (current: string, chunk: Buffer): string => {
          if (Buffer.byteLength(current) + chunk.byteLength > maxOutput) {
            overflow = true;
            child.kill();
            return current;
          }
          return current + chunk.toString("utf8");
        };
        child.stdout.on("data", (chunk: Buffer) => {
          stdout = append(stdout, chunk);
        });
        child.stderr.on("data", (chunk: Buffer) => {
          stderr = append(stderr, chunk);
        });
        const timeout = setTimeout(() => child.kill(), timeoutMs);
        child.once("error", (error) => {
          clearTimeout(timeout);
          resolve({ exitCode: -1, stdout, stderr: `${stderr}${error.message}` });
        });
        child.once("close", (code) => {
          clearTimeout(timeout);
          resolve({
            exitCode: overflow ? -1 : (code ?? -1),
            stdout,
            stderr: overflow ? `${stderr}Git output exceeded the configured bound` : stderr,
          });
        });
      });
    },
  };
}

function safeGitEnvironment(): NodeJS.ProcessEnv {
  const allowed = [
    "SystemRoot",
    "WINDIR",
    "PATH",
    "Path",
    "PATHEXT",
    "TEMP",
    "TMP",
    "HOME",
    "USERPROFILE",
  ];
  const environment: NodeJS.ProcessEnv = {
    GIT_TERMINAL_PROMPT: "0",
    GIT_CONFIG_NOSYSTEM: "1",
  };
  for (const key of allowed) {
    const value = process.env[key];
    if (value !== undefined) environment[key] = value;
  }
  return environment;
}

export function fixtureGitIdentityEnvironment(): Readonly<Record<string, string>> {
  return {
    GIT_AUTHOR_NAME: "Zap Repository Service",
    GIT_AUTHOR_EMAIL: "zap-repository@invalid.local",
    GIT_COMMITTER_NAME: "Zap Repository Service",
    GIT_COMMITTER_EMAIL: "zap-repository@invalid.local",
  };
}
