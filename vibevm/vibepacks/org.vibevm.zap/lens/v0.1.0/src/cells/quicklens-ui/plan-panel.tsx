/** @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { component$, useSignal, type NoSerialize, type QRL } from "@qwik.dev/core";

import type {
  AgentTargetView,
  PlanPreviewView,
  PlanStatus,
  QuicklensDataSource,
  QuicklensRef,
} from "../quicklens-model/index.ts";

export interface PlanPanelProps {
  readonly plan: PlanStatus | null;
  readonly targets: readonly AgentTargetView[];
  readonly source: NoSerialize<QuicklensDataSource>;
  readonly onRefresh$: QRL<() => void>;
}

export const PlanPanel = component$<PlanPanelProps>((props) => {
  const intent = useSignal("");
  const targetRef = useSignal<QuicklensRef | null>(null);
  const intentRef = useSignal<QuicklensRef | null>(null);
  const previewRef = useSignal<QuicklensRef | null>(null);
  const previewOperationRef = useSignal<QuicklensRef | null>(null);
  const preview = useSignal<PlanPreviewView | null>(null);
  const operationRef = useSignal<QuicklensRef | null>(null);
  const decisionChoice = useSignal<"approve" | "reject" | "revise" | "defer">("approve");
  const decisionReason = useSignal("");
  const message = useSignal<string | null>(null);
  const plan = props.plan;
  const eligibleTargets = props.targets.filter((target) => target.eligible);
  const selectedTarget =
    targetRef.value ?? (eligibleTargets.length === 1 ? (eligibleTargets[0]?.ref ?? null) : null);
  const disabledReasons = [
    ...new Set(
      Object.entries(plan?.actions ?? {})
        .filter(
          ([name, availability]) =>
            (name !== "decide" || plan?.state === "held") &&
            !availability.enabled &&
            availability.reason !== null,
        )
        .map(([, availability]) => availability)
        .map((availability) => availability.reason),
    ),
  ];

  if (plan === null) {
    return (
      <section class="panel plan-panel">
        <p class="eyebrow">Plan control</p>
        <h2>No active plan</h2>
        <p class="unknown-copy">
          The data source did not provide a discoverable active outcome, strategy, and plan.
        </p>
      </section>
    );
  }

  return (
    <section class="panel plan-panel">
      <div class="panel-title-row">
        <div>
          <p class="eyebrow">Plan control</p>
          <h2>{plan.planLabel}</h2>
        </div>
        <span class={`plan-state plan-state-${plan.state}`}>{plan.state}</span>
      </div>
      <dl class="plan-context">
        <div>
          <dt>Outcome</dt>
          <dd>{plan.outcomeLabel}</dd>
        </div>
        <div>
          <dt>Strategy</dt>
          <dd>{plan.strategyLabel}</dd>
        </div>
        <div>
          <dt>Revision</dt>
          <dd>{plan.basis.revision}</dd>
        </div>
      </dl>
      {plan.detail === null ? null : <p class="plan-detail">{plan.detail}</p>}
      <label class="field-label" for="plan-target">
        Route plan intent to
      </label>
      <select
        id="plan-target"
        value={selectedTarget ?? ""}
        onChange$={(_, element) => {
          targetRef.value =
            props.targets.find((target) => target.ref === element.value)?.ref ?? null;
        }}
      >
        <option value="" selected={selectedTarget === null}>
          Choose an eligible coordinator…
        </option>
        {props.targets.map((target) => (
          <option
            value={target.ref}
            disabled={!target.eligible}
            selected={target.ref === selectedTarget}
            key={target.ref}
          >
            {`${target.label} · ${target.host} · ${target.state}${
              target.eligible || target.reason === null ? "" : ` · ${target.reason}`
            }`}
          </option>
        ))}
      </select>
      <label class="field-label" for="plan-intent">
        Proposed revision
      </label>
      <textarea
        id="plan-intent"
        rows={3}
        placeholder="Describe the plan change in ordinary language"
        value={intent.value}
        onInput$={(_, element) => {
          intent.value = element.value;
        }}
      />
      <div class="plan-actions">
        <PlanAction
          label="Propose"
          availability={plan.actions.propose}
          disabled={intent.value.trim().length === 0 || selectedTarget === null}
          onRun$={async () => {
            const source = props.source;
            const targetActorRef = selectedTarget;
            if (source === undefined || targetActorRef === null) return;
            const result = await source.proposePlanIntent({
              text: intent.value,
              basis: plan.basis,
              targetActorRef,
            });
            if (result.ok) intentRef.value = result.value.operationRef;
            message.value = result.ok ? result.value.message : result.error.message;
          }}
        />
        <PlanAction
          label="Preview"
          availability={plan.actions.preview}
          disabled={intentRef.value === null}
          onRun$={async () => {
            const source = props.source;
            const ref = intentRef.value;
            if (source === undefined || ref === null) return;
            const result = await source.previewPlan({ intentRef: ref, basis: plan.basis });
            if (result.ok) {
              previewOperationRef.value = result.value.operationRef;
              previewRef.value = result.value.previewRef;
              preview.value = result.value.preview;
            }
            message.value = result.ok ? result.value.message : result.error.message;
          }}
        />
        <PlanAction
          label="Apply"
          availability={plan.actions.apply}
          disabled={previewRef.value === null || previewOperationRef.value === null}
          onRun$={async () => {
            const source = props.source;
            const preparedPreview = previewRef.value;
            const preparedOperation = previewOperationRef.value;
            if (source === undefined || preparedPreview === null || preparedOperation === null)
              return;
            const result = await source.applyPlan({
              operationRef: preparedOperation,
              previewRef: preparedPreview,
              basis: plan.basis,
            });
            if (result.ok) operationRef.value = result.value.operationRef;
            message.value = result.ok ? result.value.message : result.error.message;
            if (result.ok) void props.onRefresh$();
          }}
        />
        <PlanAction
          label="Reconcile"
          availability={plan.actions.reconcile}
          disabled={operationRef.value === null}
          onRun$={async () => {
            const source = props.source;
            const operation = operationRef.value;
            if (source === undefined || operation === null) return;
            const result = await source.reconcilePlan({
              operationRef: operation,
              basis: plan.basis,
            });
            message.value = result.ok ? result.value.message : result.error.message;
            if (result.ok) void props.onRefresh$();
          }}
        />
      </div>
      {plan.state !== "held" ? null : (
        <div class="decision-card">
          <strong>Owner decision required</strong>
          {plan.decision === null ? (
            <p class="unknown-copy">The source did not provide a valid held-operation binding.</p>
          ) : (
            <>
              <label class="field-label" for="plan-decision">
                Decision
              </label>
              <select
                id="plan-decision"
                value={decisionChoice.value}
                onChange$={(_, element) => {
                  const choice = element.value;
                  if (
                    choice === "approve" ||
                    choice === "reject" ||
                    choice === "revise" ||
                    choice === "defer"
                  ) {
                    decisionChoice.value = choice;
                  }
                }}
              >
                <option value="approve">Approve</option>
                <option value="reject">Reject</option>
                <option value="revise">Request revision</option>
                <option value="defer">Defer</option>
              </select>
              <label class="field-label" for="decision-reason">
                Reason
              </label>
              <textarea
                id="decision-reason"
                rows={2}
                maxLength={4_096}
                value={decisionReason.value}
                onInput$={(_, element) => {
                  decisionReason.value = element.value;
                }}
              />
              <PlanAction
                label="Submit owner decision"
                availability={plan.actions.decide}
                disabled={decisionReason.value.trim().length === 0}
                onRun$={async () => {
                  const source = props.source;
                  const decision = plan.decision;
                  if (source === undefined || decision === null) return;
                  const result = await source.decidePlan({
                    operationRef: decision.operationRef,
                    holdRef: decision.holdRef,
                    basis: plan.basis,
                    choice: decisionChoice.value,
                    reason: decisionReason.value,
                  });
                  message.value = result.ok ? result.value.message : result.error.message;
                  if (result.ok) void props.onRefresh$();
                }}
              />
            </>
          )}
        </div>
      )}
      {disabledReasons.length === 0 ? null : (
        <div class="disabled-reasons">
          <strong>Unavailable in this context</strong>
          <ul>
            {disabledReasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        </div>
      )}
      {preview.value === null ? null : (
        <div class="preview-card">
          <div class="preview-heading">
            <strong>Prepared changes</strong>
            <span>Basis revision {preview.value.basis.revision}</span>
          </div>
          <ul>
            {preview.value.changes.map((change) => (
              <li key={`${change.subjectLabel}:${change.changeKind}`}>
                <strong>{change.subjectLabel}</strong>
                <span>{change.changeKind.replaceAll("_", " ")}</span>
                <small>{change.beforeSummary}</small>
                <b>→</b>
                <small>{change.afterSummary}</small>
              </li>
            ))}
          </ul>
        </div>
      )}
      {message.value === null ? null : <p class="operation-message">{message.value}</p>}
    </section>
  );
});

interface PlanActionProps {
  readonly label: string;
  readonly availability: { readonly enabled: boolean; readonly reason: string | null };
  readonly disabled: boolean;
  readonly onRun$: QRL<() => void | Promise<void>>;
}

const PlanAction = component$<PlanActionProps>((props) => {
  const disabled = !props.availability.enabled || props.disabled;
  return (
    <div class="action-wrap">
      <button class="button secondary" disabled={disabled} onClick$={props.onRun$}>
        {props.label}
      </button>
    </div>
  );
});
