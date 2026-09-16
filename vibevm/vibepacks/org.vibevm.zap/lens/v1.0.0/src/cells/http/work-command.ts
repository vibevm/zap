/** Authenticated managed/native work HTTP dispatch. @scope spec://org.vibevm.zap/lens/PROP-010#managed-work */
import type {
  ManagedWorkAgentPort,
  NativeWorkAgentPort,
  RepositoryWorkspaceAgentPort,
} from "../managed-work/index.ts";
import type { Result } from "../protocol/index.ts";
import { failure, type AdapterSessionId } from "../transport/index.ts";

export async function executeWorkCommand(
  path: string,
  body: unknown,
  session: AdapterSessionId,
  ports: {
    readonly managedWork?: () => ManagedWorkAgentPort | undefined;
    readonly nativeWork?: () => NativeWorkAgentPort | undefined;
    readonly repositoryWork?: () => RepositoryWorkspaceAgentPort | undefined;
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
  if (path.startsWith("/v1/repository-work/")) {
    const repository = ports.repositoryWork?.();
    if (repository === undefined)
      return failure("unsupported_operation", "repository workspace runtime is not configured");
    const operation = path.slice("/v1/repository-work/".length);
    if (operation === "plan-list") return repository.planList(session, body);
    if (operation === "worktree-list") return repository.worktreeList(session, body);
    if (operation === "worktree-get") return repository.worktreeGet(session, body);
    if (operation === "integration-list") return repository.integrationList(session, body);
    if (operation === "integration-get") return repository.integrationGet(session, body);
    if (operation === "integration-diff") return repository.integrationDiff(session, body);
    if (operation === "integration-prepare") return repository.integrationPrepare(session, body);
    if (operation === "integration-test") return repository.integrationTest(session, body);
    return failure("not_found", "repository actor command is unavailable");
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
