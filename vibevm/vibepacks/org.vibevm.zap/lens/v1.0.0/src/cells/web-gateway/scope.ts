/** Web-role view projection. @scope spec://org.vibevm.zap/lens/PROP-003#web-authority */
import type { ActionAvailability, QuicklensSnapshot } from "../quicklens-model/index.ts";
import type { WebOperation } from "./index.ts";

export function restrictSnapshot(
  snapshot: QuicklensSnapshot,
  operations: ReadonlySet<WebOperation>,
): QuicklensSnapshot {
  const access = (operation: WebOperation, current: ActionAvailability): ActionAvailability =>
    operations.has(operation)
      ? current
      : { enabled: false, reason: `Configured web role does not allow ${operation}.` };
  return {
    ...snapshot,
    questionAnswer: access("answer", snapshot.questionAnswer),
    plan:
      snapshot.plan === null
        ? null
        : {
            ...snapshot.plan,
            actions: {
              propose: access("intent", snapshot.plan.actions.propose),
              preview: access("preview", snapshot.plan.actions.preview),
              apply: access("apply", snapshot.plan.actions.apply),
              reconcile: access("reconcile", snapshot.plan.actions.reconcile),
              decide: access("decide", snapshot.plan.actions.decide),
            },
          },
  };
}
