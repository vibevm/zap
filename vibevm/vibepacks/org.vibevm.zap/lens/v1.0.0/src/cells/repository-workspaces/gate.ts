/** Exclusive local target-writer gate. @scope spec://org.vibevm.zap/lens/PROP-014#integration */
import type { RepositoryWorkspaceResult } from "./contracts.ts";

export interface RepositoryWriterTarget {
  readonly repositoryId: string;
  readonly executionHostId: string;
  readonly planId: string;
  readonly targetWorktreeId: string;
  readonly expectedHead: string;
  readonly operationId: string;
}
export interface RepositoryWriterLease {
  readonly leaseId: string;
  readonly epoch: string;
  readonly target: RepositoryWriterTarget;
}
export interface RepositoryWriterGate {
  acquire(
    target: RepositoryWriterTarget,
  ): Promise<RepositoryWorkspaceResult<RepositoryWriterLease>>;
  validate(lease: RepositoryWriterLease): Promise<RepositoryWorkspaceResult<null>>;
  current(targetWorktreeId: string): RepositoryWriterLease | null;
  release(lease: RepositoryWriterLease): Promise<void>;
}

export function createUnavailableWriterGate(): RepositoryWriterGate {
  const denied = (): RepositoryWorkspaceResult<never> => ({
    ok: false,
    error: { code: "denied", message: "no repository writer authority is configured" },
  });
  return {
    acquire: () => Promise.resolve(denied()),
    validate: () => Promise.resolve(denied()),
    current: () => null,
    release: () => Promise.resolve(),
  };
}

export interface LocalIdleWriterGateOptions {
  readonly observeActiveWriters: (target: RepositoryWriterTarget) => Promise<number>;
  readonly createLeaseId: () => string;
}

export function createLocalIdleWriterGate(
  options: LocalIdleWriterGateOptions,
): RepositoryWriterGate {
  const leases = new Map<string, RepositoryWriterLease>();
  let epoch = 0n;
  return {
    async acquire(target) {
      if (await options.observeActiveWriters(target))
        return { ok: false, error: { code: "denied", message: "target has an active writer" } };
      if (
        [...leases.values()].some(
          (lease) => lease.target.targetWorktreeId === target.targetWorktreeId,
        )
      )
        return {
          ok: false,
          error: { code: "denied", message: "target already has a writer lease" },
        };
      epoch += 1n;
      const lease: RepositoryWriterLease = {
        leaseId: options.createLeaseId(),
        epoch: epoch.toString(),
        target,
      };
      leases.set(lease.leaseId, lease);
      return { ok: true, value: lease };
    },
    async validate(lease) {
      const current = leases.get(lease.leaseId);
      if (current?.epoch !== lease.epoch)
        return {
          ok: false,
          error: { code: "denied", message: "writer lease is no longer current" },
        };
      if (await options.observeActiveWriters(lease.target))
        return { ok: false, error: { code: "denied", message: "target gained an active writer" } };
      return { ok: true, value: null };
    },
    current(targetWorktreeId) {
      return (
        [...leases.values()].find((lease) => lease.target.targetWorktreeId === targetWorktreeId) ??
        null
      );
    },
    release(lease) {
      if (leases.get(lease.leaseId)?.epoch === lease.epoch) leases.delete(lease.leaseId);
      return Promise.resolve();
    },
  };
}
