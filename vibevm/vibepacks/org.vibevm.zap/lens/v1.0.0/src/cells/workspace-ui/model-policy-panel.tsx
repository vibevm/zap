/** Compact shared model-policy view. @scope spec://org.vibevm.zap/lens/PROP-008#policy-interface */
import { component$, useSignal, type QRL } from "@qwik.dev/core";

import {
  previewWorkspaceModelPolicy,
  updateWorkspaceModelPolicy,
  type ModelPolicyWorkspaceView,
  type WorkspaceClientPort,
} from "../workspace-client/index.ts";
import {
  ModelPolicySchema,
  ModelTierSchema,
  ModelSelectionRequestSchema,
  TaskClassSchema,
  TaskPurposeSchema,
  type ModelPolicy,
} from "../model-policy/index.ts";
import { ClientRequestIdSchema, DecimalSchema } from "../protocol/index.ts";
import type { ProjectId, WorkContextId } from "../workspace-model/index.ts";

export const ModelPolicyPanel = component$<{
  readonly view: ModelPolicyWorkspaceView | null;
  readonly unavailableReason: string | null;
  readonly port: WorkspaceClientPort | undefined;
  readonly projectId: ProjectId | null;
  readonly contextId: WorkContextId | null;
  readonly onSaved$: QRL<() => void>;
}>((props) => {
  const draft = useSignal<ModelPolicy | null>(props.view?.policy.policy ?? null);
  const status = useSignal<string | null>(null);
  const preview = useSignal<string | null>(null);
  const edit = (next: ModelPolicy): void => {
    const parsed = ModelPolicySchema.safeParse(next);
    if (parsed.success) draft.value = parsed.data;
  };
  return (
    <details class="workspace-panel model-policy-panel">
      <summary>Model policy settings</summary>
      <div class="workspace-panel-heading">
        <div>
          <p class="eyebrow">Model policy</p>
          <h2>{props.view?.policy.policy.policyId ?? "Unavailable"}</h2>
        </div>
        {props.view === null ? null : (
          <span class="count-chip">rev {props.view.policy.policy.revision}</span>
        )}
      </div>
      {props.view === null ? (
        <p class="workspace-muted">Model policy settings are unavailable for this project.</p>
      ) : (
        <>
          <div class="model-policy-bindings">
            {props.view.policy.policy.tierBindings.map((binding, index) => (
              <div key={binding.tier}>
                <strong>{binding.tier}</strong>
                <input
                  aria-label={`${binding.tier} model`}
                  value={draft.value?.tierBindings[index]?.modelId ?? binding.modelId}
                  onInput$={(_, element) => {
                    const bindings = [...(draft.value?.tierBindings ?? [])];
                    const current = bindings[index];
                    if (current === undefined) return;
                    bindings[index] = { ...current, modelId: element.value };
                    if (draft.value !== null) edit({ ...draft.value, tierBindings: bindings });
                  }}
                />
                <input
                  aria-label={`${binding.tier} profile`}
                  value={draft.value?.tierBindings[index]?.profileId ?? binding.profileId}
                  onInput$={(_, element) => {
                    const bindings = [...(draft.value?.tierBindings ?? [])];
                    const current = bindings[index];
                    if (current === undefined) return;
                    bindings[index] = { ...current, profileId: element.value };
                    if (draft.value !== null) edit({ ...draft.value, tierBindings: bindings });
                  }}
                />
              </div>
            ))}
          </div>
          <div class="model-policy-rules">
            {props.view.policy.policy.taskRules.map((rule, index) => (
              <div class="model-policy-rule" key={rule.ruleId}>
                <strong>{rule.ruleId}</strong>
                <select
                  aria-label={`${rule.ruleId} purpose`}
                  value={
                    draft.value?.taskRules[index]?.match.purposes?.[0] ??
                    rule.match.purposes?.[0] ??
                    "other"
                  }
                  onChange$={(_, element) => {
                    if (draft.value === null) return;
                    const purpose = TaskPurposeSchema.safeParse(element.value);
                    const rules = [...draft.value.taskRules];
                    const current = rules[index];
                    if (!purpose.success || current === undefined) return;
                    rules[index] = {
                      ...current,
                      match: { ...current.match, purposes: [purpose.data] },
                    };
                    edit({ ...draft.value, taskRules: rules });
                  }}
                >
                  {TaskPurposeSchema.options.map((purpose) => (
                    <option value={purpose} key={purpose}>
                      {purpose}
                    </option>
                  ))}
                </select>
                <select
                  aria-label={`${rule.ruleId} task class`}
                  value={
                    draft.value?.taskRules[index]?.match.taskClasses?.[0] ??
                    rule.match.taskClasses?.[0] ??
                    "verification"
                  }
                  onChange$={(_, element) => {
                    if (draft.value === null) return;
                    const taskClass = TaskClassSchema.safeParse(element.value);
                    const rules = [...draft.value.taskRules];
                    const current = rules[index];
                    if (!taskClass.success || current === undefined) return;
                    rules[index] = {
                      ...current,
                      match: { ...current.match, taskClasses: [taskClass.data] },
                    };
                    edit({ ...draft.value, taskRules: rules });
                  }}
                >
                  {TaskClassSchema.options.map((taskClass) => (
                    <option value={taskClass} key={taskClass}>
                      {taskClass}
                    </option>
                  ))}
                </select>
                <select
                  aria-label={`${rule.ruleId} tier`}
                  value={draft.value?.taskRules[index]?.tier ?? rule.tier}
                  onChange$={(_, element) => {
                    if (draft.value === null) return;
                    const rules = [...draft.value.taskRules];
                    const current = rules[index];
                    if (current === undefined) return;
                    const tier = ModelTierSchema.safeParse(element.value);
                    if (tier.success) {
                      rules[index] = { ...current, tier: tier.data };
                      edit({ ...draft.value, taskRules: rules });
                    }
                  }}
                >
                  {(["ultra", "big", "medium", "small"] as const).map((tier) => (
                    <option value={tier} key={tier}>
                      {tier}
                    </option>
                  ))}
                </select>
                <select
                  aria-label={`${rule.ruleId} effort`}
                  value={draft.value?.taskRules[index]?.effort.mode ?? rule.effort.mode}
                  onChange$={(_, element) => {
                    if (draft.value === null) return;
                    const rules = [...draft.value.taskRules];
                    const current = rules[index];
                    if (current === undefined) return;
                    const effort =
                      element.value === "explicit"
                        ? { mode: "explicit" as const, value: "low" as const }
                        : element.value === "inherit"
                          ? { mode: "inherit" as const }
                          : { mode: "unspecified" as const };
                    rules[index] = { ...current, effort };
                    edit({ ...draft.value, taskRules: rules });
                  }}
                >
                  <option value="explicit">explicit low</option>
                  <option value="inherit">inherit</option>
                  <option value="unspecified">unspecified</option>
                </select>
              </div>
            ))}
          </div>
          <div class="model-policy-actions">
            <button
              class="button secondary"
              onClick$={() => {
                if (draft.value === null) return;
                edit({
                  ...draft.value,
                  taskRules: [
                    ...draft.value.taskRules,
                    {
                      ruleId: `rule.workspace.${crypto.randomUUID()}`,
                      priority: 50,
                      match: { purposes: ["other"], taskClasses: ["verification"] },
                      tier: "small",
                      effort: { mode: "explicit", value: "low" },
                      selectionReason: "Added from Zap Quick Lens settings.",
                    },
                  ],
                });
              }}
            >
              Add task rule
            </button>
            <button
              class="button secondary"
              onClick$={async () => {
                if (
                  props.port === undefined ||
                  props.projectId === null ||
                  props.contextId === null ||
                  draft.value === null
                )
                  return;
                const rule = draft.value.taskRules[0];
                if (rule === undefined) return;
                const request = ModelSelectionRequestSchema.parse({
                  selectionRef: "selection.policy-preview",
                  purpose: rule.match.purposes?.[0] ?? "other",
                  taskClass: rule.match.taskClasses?.[0] ?? "verification",
                  role: "worker",
                  executionMode: "native",
                  invocationScope: "native_subagent",
                  productId: "codex",
                  productVersion: "0.152.1",
                  override: null,
                });
                const result = await previewWorkspaceModelPolicy(props.port, {
                  operation: "model-policy.preview.v1",
                  projectId: props.projectId,
                  contextId: props.contextId,
                  request,
                });
                preview.value = !result.ok
                  ? result.error.message
                  : result.value.result.ok
                    ? `${result.value.result.value.modelId} · ${result.value.result.value.effectiveEffort.state}`
                    : result.value.result.error.message;
              }}
            >
              Preview selection
            </button>
            <button
              class="button primary"
              onClick$={async () => {
                const currentView = props.view;
                if (
                  props.port === undefined ||
                  props.projectId === null ||
                  props.contextId === null ||
                  draft.value === null ||
                  currentView === null
                )
                  return;
                const result = await updateWorkspaceModelPolicy(props.port, {
                  operation: "model-policy.update.v1",
                  clientRequestId: ClientRequestIdSchema.parse(
                    `request.workspace.policy.${crypto.randomUUID()}`,
                  ),
                  projectId: props.projectId,
                  contextId: props.contextId,
                  sourceEventId: `workspace-policy.${crypto.randomUUID()}`,
                  expectedRevision: currentView.policy.policy.revision,
                  policy: {
                    ...draft.value,
                    revision: DecimalSchema.parse(
                      String(BigInt(currentView.policy.policy.revision) + 1n),
                    ),
                  },
                });
                status.value = result.ok
                  ? "Policy saved."
                  : result.error.code === "stale_revision"
                    ? "Conflict: refresh before saving."
                    : result.error.message;
                if (result.ok) void props.onSaved$();
              }}
            >
              Save policy
            </button>
          </div>
          {preview.value === null ? null : <p class="workspace-notice">Preview: {preview.value}</p>}
          {status.value === null ? null : <p class="operation-message">{status.value}</p>}
          <p class="workspace-muted">
            {props.view.policy.policy.taskRules.length} task rules · {props.view.versions.length}{" "}
            revisions · {props.view.changes.length} recorded changes
          </p>
          <p class="workspace-muted">
            Requested tier, effective effort, and host observation remain separate in each
            selection.
          </p>
        </>
      )}
    </details>
  );
});
