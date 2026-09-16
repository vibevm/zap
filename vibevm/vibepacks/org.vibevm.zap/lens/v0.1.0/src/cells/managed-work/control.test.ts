/** No-model managed provider control proof. @scope spec://org.vibevm.zap/lens/PROP-012#acceptance */
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, appendFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { z } from "zod";
import {
  createManagedControlRuntime,
  createManagedProviderControlAdapters,
  createZapMockManagedControlAdapter,
  ZapMockManagedAssignmentSchema,
} from "./index.ts";
import type { ManagedControlTarget } from "./index.ts";
import {
  AgentSessionIdSchema,
  ClientIdSchema,
  ProjectIdSchema,
  RunIdSchema,
  TerminalIdSchema,
  WorkspaceAccessContextSchema,
} from "../workspace-model/index.ts";
import { ActorIdSchema, PrincipalIdSchema } from "../protocol/index.ts";

const HookSettingsSchema = z.object({
  hooks: z.record(
    z.string(),
    z.array(
      z.looseObject({
        hooks: z.array(
          z.looseObject({
            url: z.url(),
            headers: z.object({ Authorization: z.string().min(10) }),
          }),
        ),
      }),
    ),
  ),
});

test("Claude hook state fences wake and safely frames text", async () => {
  const root = mkdtempSync(join(tmpdir(), "managed-claude-control-"));
  const io = new RecordingIo();
  const control = createManagedControlRuntime({
    adapters: createManagedProviderControlAdapters({ directory: root }),
    io: () => io,
  });
  try {
    const prepared = await control.prepare({
      ...identity("claude"),
      provider: "claude_code",
      launch: launch(["--model", "sonnet", "do bounded work"]),
    });
    assert.equal(prepared.ok, true);
    if (!prepared.ok) return;
    const settingsIndex = prepared.value.launch.args.indexOf("--settings");
    assert.notEqual(settingsIndex, -1);
    const settingsPath = prepared.value.launch.args[settingsIndex + 1];
    assert.notEqual(settingsPath, undefined);
    if (settingsPath === undefined) return;
    const settings = HookSettingsSchema.parse(JSON.parse(readFileSync(settingsPath, "utf8")));
    const hook = settings.hooks["SessionStart"]?.[0]?.hooks[0];
    assert.notEqual(hook, undefined);
    if (hook === undefined) return;
    assert.equal((await control.activate(activation(prepared.value.target))).ok, true);
    await post(hook, { hook_event_name: "SessionStart", session_id: "claude.session.fixture" });
    assert.equal(readiness(control, prepared.value.target), "starting");
    await post(hook, {
      hook_event_name: "UserPromptSubmit",
      session_id: "claude.session.fixture",
      prompt_id: "prompt.fixture",
    });
    await post(hook, { hook_event_name: "Stop", session_id: "claude.session.fixture" });
    assert.equal(readiness(control, prepared.value.target), "busy");
    await post(hook, {
      hook_event_name: "PermissionRequest",
      session_id: "claude.session.fixture",
      tool_use_id: "tool.fixture",
    });
    const blocked = await control.offer({
      ...prepared.value.target,
      expectedAutomationControlEpoch: "1",
      deliveryId: "delivery.blocked",
      bodyMarkdown: "blocked",
    });
    assert.equal(blocked.ok && blocked.value.observation, "permission_required");
    await post(hook, {
      hook_event_name: "Notification",
      notification_type: "idle_prompt",
      session_id: "claude.session.fixture",
    });
    const accepted = await control.offer({
      ...prepared.value.target,
      expectedAutomationControlEpoch: "1",
      deliveryId: "delivery.safe",
      bodyMarkdown: "First line\nSecond line",
    });
    assert.equal(accepted.ok && accepted.value.observation, "host_accepted");
    assert.deepEqual(io.inputs, ["\u001b[200~First line\nSecond line\u001b[201~\r"]);
    await post(hook, {
      hook_event_name: "Notification",
      notification_type: "idle_prompt",
      session_id: "claude.session.fixture",
    });
    const unsafe = await control.offer({
      ...prepared.value.target,
      expectedAutomationControlEpoch: "1",
      deliveryId: "delivery.unsafe",
      bodyMarkdown: "unsafe\u001bcommand",
    });
    assert.equal(unsafe.ok, false);
    assert.equal(io.inputs.length, 1);
    const stale = await control.offer({
      ...prepared.value.target,
      expectedAutomationControlEpoch: "2",
      deliveryId: "delivery.stale",
      bodyMarkdown: "stale",
    });
    assert.equal(stale.ok, false);
    await post(hook, {
      hook_event_name: "UserPromptSubmit",
      session_id: "claude.session.fixture",
      prompt_id: "prompt.pause.fixture",
    });
    const paused = await control.interrupt({
      ...prepared.value.target,
      reason: "project_pause",
    });
    assert.equal(paused.ok && paused.value.observation, "requested");
    assert.equal(io.stops, 1);
    const pausing = control.inspect(prepared.value.target);
    assert.equal(pausing.ok && pausing.value.pauseRequested, true);
    await post(hook, {
      hook_event_name: "SessionEnd",
      session_id: "claude.session.fixture",
    });
    assert.equal(readiness(control, prepared.value.target), "stopped");
    const resumed = await control.prepare({
      runId: prepared.value.target.runId,
      actorId: prepared.value.target.actorId,
      sessionId: prepared.value.target.sessionId,
      terminalId: TerminalIdSchema.parse("terminal.claude.resume"),
      provider: "claude_code",
      launch: launch(["--resume", "claude.session.fixture"]),
    });
    assert.equal(resumed.ok, true);
    if (resumed.ok)
      assert.notEqual(
        resumed.value.target.expectedProcessEpoch,
        prepared.value.target.expectedProcessEpoch,
      );
  } finally {
    control.close();
    rmSync(root, { recursive: true, force: true });
  }
});

test("Qwen keeps bare TUI and uses structured files without terminal injection", async () => {
  const root = mkdtempSync(join(tmpdir(), "managed-qwen-control-"));
  const io = new RecordingIo();
  const control = createManagedControlRuntime({
    adapters: createManagedProviderControlAdapters({ directory: root }),
    io: () => io,
  });
  try {
    const prepared = await control.prepare({
      ...identity("qwen"),
      provider: "qwen_code",
      launch: launch(["--bare", "--prompt-interactive", "do bounded work"]),
    });
    assert.equal(prepared.ok, true);
    if (!prepared.ok) return;
    assert.equal(prepared.value.launch.args.includes("--bare"), true);
    const inputPath = argument(prepared.value.launch.args, "--input-file");
    const outputPath = argument(prepared.value.launch.args, "--json-file");
    assert.equal((await control.activate(activation(prepared.value.target))).ok, true);
    const queued = await control.offer({
      ...prepared.value.target,
      expectedAutomationControlEpoch: "1",
      deliveryId: "delivery.qwen",
      bodyMarkdown: "late addressed notice",
    });
    assert.equal(queued.ok && queued.value.observation, "provider_queued");
    assert.match(readFileSync(inputPath, "utf8"), /"type":"submit"/);
    assert.deepEqual(io.inputs, []);
    const interrupted = await control.interrupt({
      ...prepared.value.target,
      reason: "user_interrupt",
    });
    assert.equal(interrupted.ok && interrupted.value.observation, "requested");
    assert.equal(io.interrupts, 1);
    appendFileSync(
      outputPath,
      `${JSON.stringify({ type: "assistant", uuid: "assistant.fixture", session_id: "qwen.fixture", message: { stop_reason: null } })}\n`,
    );
    await new Promise((resolve) => setTimeout(resolve, 250));
    assert.equal(readiness(control, prepared.value.target), "idle");
  } finally {
    control.close();
    rmSync(root, { recursive: true, force: true });
  }
});

test("ZapMock managed bridge uses explicit files and never terminal input", async () => {
  const root = mkdtempSync(join(tmpdir(), "managed-zap-mock-control-"));
  const io = new RecordingIo();
  const control = createManagedControlRuntime({
    adapters: [createZapMockManagedControlAdapter({ directory: root })],
    io: () => io,
  });
  try {
    const prepared = await control.prepare({
      ...identity("zap-mock"),
      provider: "zap_mock",
      launch: launch(["managed", "--instructions", "bounded mock packet"]),
    });
    assert.equal(prepared.ok, true);
    if (!prepared.ok) return;
    const inputPath = argument(prepared.value.launch.args, "--input");
    const sidebandPath = argument(prepared.value.launch.args, "--sideband");
    const assignment = ZapMockManagedAssignmentSchema.parse(
      JSON.parse(argument(prepared.value.launch.args, "--assignment")),
    );
    assert.equal(assignment.runId, "run.zap-mock");
    assert.equal(argument(prepared.value.launch.args, "--session"), "session.zap-mock");
    assert.equal((await control.activate(activation(prepared.value.target))).ok, true);
    appendFileSync(
      sidebandPath,
      `${JSON.stringify({ kind: "turn_settled", providerSessionId: "mock.saved", providerTurnId: "mock.turn.1", transportCorrelation: null })}\n`,
    );
    await new Promise((resolve) => setTimeout(resolve, 150));
    assert.equal(readiness(control, prepared.value.target), "idle");
    const offered = await control.offer({
      ...prepared.value.target,
      expectedAutomationControlEpoch: "1",
      deliveryId: "delivery.mock",
      bodyMarkdown: "mock late answer",
    });
    assert.equal(offered.ok && offered.value.observation, "provider_queued");
    const paused = await control.interrupt({
      ...prepared.value.target,
      reason: "project_pause",
    });
    assert.equal(paused.ok && paused.value.observation, "requested");
    const commands = readFileSync(inputPath, "utf8").trim().split(/\r?\n/);
    assert.equal(commands.length, 2);
    assert.match(commands[0] ?? "", /"kind":"offer"/);
    assert.match(commands[1] ?? "", /"kind":"interrupt"/);
    assert.deepEqual(io.inputs, []);
    assert.equal(io.interrupts, 0);
  } finally {
    control.close();
    rmSync(root, { recursive: true, force: true });
  }
});

function identity(suffix: string) {
  return {
    runId: RunIdSchema.parse(`run.${suffix}`),
    actorId: ActorIdSchema.parse(`actor.${suffix}`),
    sessionId: AgentSessionIdSchema.parse(`session.${suffix}`),
    terminalId: TerminalIdSchema.parse(`terminal.${suffix}`),
  };
}

function launch(args: readonly string[]) {
  return { executable: "C:/fixture/provider.exe", args, cwd: "C:/fixture", env: {} };
}

function activation(target: ManagedControlTarget) {
  return {
    target,
    access: WorkspaceAccessContextSchema.parse({
      principalId: PrincipalIdSchema.parse("principal.control.fixture"),
      actorId: null,
      clientId: ClientIdSchema.parse("client.control.fixture"),
      authorizedProjectIds: [ProjectIdSchema.parse("project.control.fixture")],
    }),
    leaseId: "lease.control.fixture",
    automationControlEpoch: "1",
  };
}

function readiness(
  control: ReturnType<typeof createManagedControlRuntime>,
  target: ManagedControlTarget,
) {
  const inspected = control.inspect(target);
  assert.equal(inspected.ok, true);
  return inspected.ok ? inspected.value.readiness : "unknown";
}

function argument(args: readonly string[], name: string): string {
  const value = args[args.indexOf(name) + 1];
  assert.notEqual(value, undefined);
  return value ?? "";
}

async function post(
  hook: z.infer<typeof HookSettingsSchema>["hooks"][string][number]["hooks"][number],
  body: Record<string, string>,
): Promise<void> {
  const response = await fetch(hook.url, {
    method: "POST",
    headers: { authorization: hook.headers.Authorization, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  assert.equal(response.status, 200);
}

class RecordingIo {
  readonly inputs: string[] = [];
  interrupts = 0;
  stops = 0;
  input(data: string) {
    this.inputs.push(data);
    return { ok: true as const, value: undefined };
  }
  interrupt() {
    this.interrupts += 1;
    return { ok: true as const, value: undefined };
  }
  stop() {
    this.stops += 1;
    return { ok: true as const, value: undefined };
  }
}
