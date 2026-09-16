/** Authenticated managed/native work HTTP dispatch. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import type { ManagedWorkAgentPort, NativeWorkAgentPort } from "../managed-work/index.ts";
import type { Result } from "../protocol/index.ts";
import { failure, type AdapterSessionId } from "../transport/index.ts";

export async function executeWorkCommand(
  path: string,
  body: unknown,
  session: AdapterSessionId,
  ports: {
    readonly managedWork?: () => ManagedWorkAgentPort | undefined;
    readonly nativeWork?: () => NativeWorkAgentPort | undefined;
  },
): Promise<Result<unknown> | null> {
  if (path.startsWith("/v1/managed-work/")) {
    const managed = ports.managedWork?.();
    if (managed === undefined)
      return failure("unsupported_operation", "managed work runtime is not configured");
    const operation = path.slice("/v1/managed-work/".length);
    if (operation === "profiles") return managed.profiles(session);
    if (operation === "create") return managed.create(session, body);
    if (operation === "start") return managed.start(session, body);
    if (operation === "read") return managed.read(session, body);
    if (operation === "report") return managed.report(session, body);
    if (operation === "attachment-ack") return managed.acknowledgeAttachment(session, body);
    return failure("not_found", "managed work actor command is unavailable");
  }
  if (!path.startsWith("/v1/native-work/")) return null;
  const native = ports.nativeWork?.();
  if (native === undefined)
    return failure("unsupported_operation", "native work attachments are not configured");
  const operation = path.slice("/v1/native-work/".length);
  if (operation === "before") return native.beforeWork(session, body);
  if (operation === "read") return native.read(session, body);
  if (operation === "attachment-ack") return native.acknowledgeAttachment(session, body);
  return failure("not_found", "native work actor command is unavailable");
}
