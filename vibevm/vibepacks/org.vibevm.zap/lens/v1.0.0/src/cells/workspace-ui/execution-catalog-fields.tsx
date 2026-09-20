/** Human-first execution catalog fields. @scope spec://org.vibevm.zap/lens/PROP-015#catalog */
import { component$, type QRL } from "@qwik.dev/core";
import {
  ExecutionConfigurationRecordSchema,
  type ExecutionConfigurationRecord,
  type ExecutionConnectionRecord,
  type UsageObservation,
} from "../execution-catalog/index.ts";
import type { ReasoningEffort } from "../model-policy/index.ts";

export const ConnectionEditor = component$<{
  readonly connection: ExecutionConnectionRecord;
  readonly usage: readonly UsageObservation[];
  readonly sampledAt: string;
  readonly freshnessSeconds: number;
  readonly canAdmin: boolean;
  readonly refreshing: boolean;
  readonly onChange$: QRL<(value: ExecutionConnectionRecord) => void>;
  readonly onArchive$: QRL<() => void>;
  readonly onRefreshUsage$?: QRL<() => void | Promise<void>>;
}>((props) => (
  <article class="execution-catalog-card">
    <label class="field-label execution-name">
      Name
      <input
        value={props.connection.displayName}
        disabled={!props.canAdmin}
        onInput$={(_, element) =>
          props.onChange$({ ...props.connection, displayName: element.value })
        }
      />
    </label>
    <dl class="execution-metadata">
      <Metadata label="Agent" value={agentLabel(props.connection.agentProduct)} />
      <Metadata label="Provider/account" value={props.connection.providerId} />
      <Metadata label="Protected binding" value={props.connection.launchBindingId} />
      <Metadata label="Status" value={props.connection.enabled ? "Enabled" : "Disabled"} />
    </dl>
    <p class="workspace-muted">{props.connection.setupGuidance}</p>
    <div class="execution-actions">
      {props.onRefreshUsage$ === undefined ? null : (
        <button
          class="button secondary"
          disabled={props.refreshing}
          onClick$={props.onRefreshUsage$}
        >
          {props.refreshing ? "Refreshing usage…" : "Refresh usage"}
        </button>
      )}
      <button
        class="button secondary danger"
        disabled={!props.canAdmin}
        onClick$={props.onArchive$}
      >
        Archive account
      </button>
    </div>
    <div class="execution-usage-list">
      {props.usage.length === 0 ? (
        <span class="workspace-muted">Usage unavailable · unknown, not 0%</span>
      ) : (
        props.usage.map((observation) => (
          <UsageRow
            key={observation.observationId}
            observation={observation}
            stale={isStale(observation, props.sampledAt, props.freshnessSeconds)}
          />
        ))
      )}
    </div>
  </article>
));

export const ConfigurationEditor = component$<{
  readonly configuration: ExecutionConfigurationRecord;
  readonly connectionName: string;
  readonly usage: readonly UsageObservation[];
  readonly canAdmin: boolean;
  readonly onChange$: QRL<(value: ExecutionConfigurationRecord) => void>;
  readonly onArchive$: QRL<() => void>;
}>((props) => (
  <article class="execution-catalog-card execution-configuration">
    <label class="field-label execution-name">
      Name
      <input
        value={props.configuration.displayName}
        disabled={!props.canAdmin}
        onInput$={(_, element) =>
          props.onChange$({ ...props.configuration, displayName: element.value })
        }
      />
    </label>
    <dl class="execution-metadata">
      <Metadata label="Agent" value={agentLabel(props.configuration.agentProduct)} />
      <Metadata
        label="Provider/account"
        value={props.configuration.providerId + " · " + props.connectionName}
      />
      <Metadata
        label="Model"
        value={props.configuration.modelFamilyId + " · " + props.configuration.modelId}
      />
      <Metadata label="Effort" value={effortSummary(props.configuration)} />
      <Metadata label="Context" value={contextSummary(props.configuration)} />
    </dl>
    {props.configuration.adapterEffort.mode !== "configurable" ||
    props.configuration.effort.mode !== "configurable" ? (
      <p class="workspace-muted">
        Effort is {props.configuration.adapterEffort.mode} for this adapter.
      </p>
    ) : (
      <fieldset class="execution-choice-set" disabled={!props.canAdmin}>
        <legend>Allowed effort choices</legend>
        {props.configuration.adapterEffort.allowedValues.map((effort) => {
          const selected =
            props.configuration.effort.mode === "configurable" &&
            props.configuration.effort.allowedValues.includes(effort);
          return (
            <label key={effort}>
              <input
                type="checkbox"
                checked={selected}
                onChange$={(_, element) => {
                  updateEffort(props.configuration, effort, element.checked, props.onChange$);
                }}
              />
              {friendly(effort)}
            </label>
          );
        })}
        <label class="execution-default-choice">
          Default effort
          <select
            value={props.configuration.effort.defaultValue ?? ""}
            onChange$={(_, element) => {
              updateDefaultEffort(props.configuration, element.value, props.onChange$);
            }}
          >
            <option value="">Provider default</option>
            {props.configuration.effort.allowedValues.map((effort) => (
              <option key={effort} value={effort}>
                {friendly(effort)}
              </option>
            ))}
          </select>
          <small>Used when a task does not request a specific effort.</small>
        </label>
      </fieldset>
    )}
    {props.configuration.adapterContext.mode !== "configurable" ||
    props.configuration.context.mode !== "configurable" ? (
      <p class="workspace-muted">
        Context is {props.configuration.adapterContext.mode}. {contextDetail(props.configuration)}
      </p>
    ) : (
      <fieldset class="execution-choice-set" disabled={!props.canAdmin}>
        <legend>Allowed context choices</legend>
        {props.configuration.adapterContext.allowedTokens.map((tokens) => (
          <label key={tokens}>
            <input
              type="checkbox"
              checked={
                props.configuration.context.mode === "configurable" &&
                props.configuration.context.allowedTokens.includes(tokens)
              }
              onChange$={(_, element) => {
                updateContexts(props.configuration, tokens, element.checked, props.onChange$);
              }}
            />
            {formatTokens(tokens)}
          </label>
        ))}
      </fieldset>
    )}
    <fieldset class="execution-choice-set" disabled={!props.canAdmin}>
      <legend>Quota meters used for this configuration</legend>
      {props.usage.length === 0 ? (
        <span class="workspace-muted">No observed meter is available for this connection.</span>
      ) : (
        uniqueUsageBuckets(props.usage).map((observation) => (
          <label key={observation.bucketId}>
            <input
              type="checkbox"
              checked={props.configuration.usageBucketIds.includes(observation.bucketId)}
              onChange$={(_, element) => {
                updateUsageBucket(
                  props.configuration,
                  observation.bucketId,
                  element.checked,
                  props.onChange$,
                );
              }}
            />
            {observation.bucketLabel} · {observation.applicability.kind.replaceAll("_", " ")} ·{" "}
            {bucketWindows(props.usage, observation.bucketId)}
          </label>
        ))
      )}
      {props.configuration.usageBucketIds.length === 0 ? (
        <small>
          No applicable meter is selected. Quota remains unknown and cannot trigger the threshold.
        </small>
      ) : null}
    </fieldset>
    <details class="execution-score-editor">
      <summary>Specialization priorities</summary>
      <div class="execution-score-table">
        {props.configuration.scores.map((score) => (
          <fieldset key={score.specialization} disabled={!props.canAdmin}>
            <legend>{friendly(score.specialization)}</legend>
            <ScoreInput
              label="Suitability"
              value={score.suitability}
              maximum={100}
              onChange$={(value) => {
                updateScore(
                  props.configuration,
                  score.specialization,
                  "suitability",
                  value,
                  props.onChange$,
                );
              }}
            />
            <ScoreInput
              label="Quality"
              value={score.quality}
              maximum={100}
              onChange$={(value) => {
                updateScore(
                  props.configuration,
                  score.specialization,
                  "quality",
                  value,
                  props.onChange$,
                );
              }}
            />
            <ScoreInput
              label="Economy"
              value={score.economy}
              maximum={100}
              onChange$={(value) => {
                updateScore(
                  props.configuration,
                  score.specialization,
                  "economy",
                  value,
                  props.onChange$,
                );
              }}
            />
            <ScoreInput
              label="Priority order"
              value={score.preferenceOrder}
              maximum={10_000}
              onChange$={(value) => {
                updateScore(
                  props.configuration,
                  score.specialization,
                  "preferenceOrder",
                  value,
                  props.onChange$,
                );
              }}
            />
            <small>{score.rationale}</small>
          </fieldset>
        ))}
      </div>
    </details>
    <p class="workspace-muted">
      {props.configuration.synthetic ? "Synthetic fixture identity" : "Configured identity"} ·{" "}
      {props.configuration.modalities.map(friendly).join(", ")}
    </p>
    <div class="execution-actions">
      <button
        class="button secondary danger"
        disabled={!props.canAdmin}
        onClick$={props.onArchive$}
      >
        Archive configuration
      </button>
    </div>
  </article>
));

export const ArchivedConnectionCard = component$<{
  readonly connection: ExecutionConnectionRecord;
  readonly canAdmin: boolean;
  readonly onRestore$: QRL<() => void>;
}>((props) => (
  <article class="execution-catalog-card execution-archived-card">
    <strong>{props.connection.displayName}</strong>
    <span>{agentLabel(props.connection.agentProduct)}</span>
    <small>{props.connection.launchBindingId}</small>
    <button class="button secondary" disabled={!props.canAdmin} onClick$={props.onRestore$}>
      Restore account
    </button>
  </article>
));

export const ArchivedConfigurationCard = component$<{
  readonly configuration: ExecutionConfigurationRecord;
  readonly connectionName: string;
  readonly connectionEnabled: boolean;
  readonly canAdmin: boolean;
  readonly onRestore$: QRL<() => void>;
}>((props) => (
  <article class="execution-catalog-card execution-archived-card">
    <strong>{props.configuration.displayName}</strong>
    <span>{props.configuration.modelId}</span>
    <small>{props.connectionName}</small>
    {!props.connectionEnabled ? (
      <small>Restore the account connection before restoring this configuration.</small>
    ) : null}
    <button
      class="button secondary"
      disabled={!props.canAdmin || !props.connectionEnabled}
      onClick$={props.onRestore$}
    >
      Restore configuration
    </button>
  </article>
));

const Metadata = component$<{ readonly label: string; readonly value: string }>((props) => (
  <div>
    <dt>{props.label}</dt>
    <dd>{props.value}</dd>
  </div>
));

const UsageRow = component$<{
  readonly observation: UsageObservation;
  readonly stale: boolean;
}>((props) => (
  <div class={"execution-usage " + (props.stale ? "stale" : "")}>
    <strong>{props.observation.bucketLabel}</strong>
    <span>{usageValue(props.observation)}</span>
    <small>
      {props.observation.status}
      {props.stale ? " · stale" : ""}
      {" · observed " + new Date(props.observation.observedAt).toLocaleString()}
      {props.observation.window.resetsAt === null
        ? ""
        : " · resets " + new Date(props.observation.window.resetsAt).toLocaleString()}
    </small>
    <small>{props.observation.detail}</small>
  </div>
));

const ScoreInput = component$<{
  readonly label: string;
  readonly value: number;
  readonly maximum: number;
  readonly onChange$: QRL<(value: number) => void>;
}>((props) => (
  <label>
    {props.label}
    <input
      type="number"
      min={0}
      max={props.maximum}
      value={props.value}
      onInput$={(_, element) => props.onChange$(Number(element.value))}
    />
  </label>
));

function updateEffort(
  configuration: ExecutionConfigurationRecord,
  effort: ReasoningEffort,
  checked: boolean,
  change: QRL<(value: ExecutionConfigurationRecord) => void>,
): void {
  if (
    configuration.adapterEffort.mode !== "configurable" ||
    configuration.effort.mode !== "configurable" ||
    !configuration.adapterEffort.allowedValues.includes(effort)
  )
    return;
  const allowed = checked
    ? [...new Set([...configuration.effort.allowedValues, effort])]
    : configuration.effort.allowedValues.filter((candidate) => candidate !== effort);
  if (allowed.length === 0) return;
  const defaultValue =
    configuration.effort.defaultValue !== null &&
    allowed.includes(configuration.effort.defaultValue)
      ? configuration.effort.defaultValue
      : (allowed[0] ?? null);
  void change(
    ExecutionConfigurationRecordSchema.parse({
      ...configuration,
      effort: { mode: "configurable", allowedValues: allowed, defaultValue },
    }),
  );
}

function updateContexts(
  configuration: ExecutionConfigurationRecord,
  tokens: number,
  checked: boolean,
  change: QRL<(value: ExecutionConfigurationRecord) => void>,
): void {
  if (
    configuration.adapterContext.mode !== "configurable" ||
    configuration.context.mode !== "configurable" ||
    !configuration.adapterContext.allowedTokens.includes(tokens)
  )
    return;
  const allowedTokens = checked
    ? [...new Set([...configuration.context.allowedTokens, tokens])].sort(
        (left, right) => left - right,
      )
    : configuration.context.allowedTokens.filter((candidate) => candidate !== tokens);
  if (allowedTokens.length === 0) return;
  const defaultTokens =
    configuration.context.defaultTokens !== null &&
    allowedTokens.includes(configuration.context.defaultTokens)
      ? configuration.context.defaultTokens
      : (allowedTokens[0] ?? null);
  void change({
    ...configuration,
    context: { ...configuration.context, allowedTokens, defaultTokens },
  });
}

function updateDefaultEffort(
  configuration: ExecutionConfigurationRecord,
  value: string,
  change: QRL<(value: ExecutionConfigurationRecord) => void>,
): void {
  if (configuration.effort.mode !== "configurable") return;
  const defaultValue =
    value === ""
      ? null
      : configuration.effort.allowedValues.find((candidate) => candidate === value);
  if (value !== "" && defaultValue === undefined) return;
  void change({
    ...configuration,
    effort: { ...configuration.effort, defaultValue: defaultValue ?? null },
  });
}

function updateUsageBucket(
  configuration: ExecutionConfigurationRecord,
  bucketId: string,
  checked: boolean,
  change: QRL<(value: ExecutionConfigurationRecord) => void>,
): void {
  const usageBucketIds = checked
    ? [...new Set([...configuration.usageBucketIds, bucketId])]
    : configuration.usageBucketIds.filter((candidate) => candidate !== bucketId);
  void change({ ...configuration, usageBucketIds });
}

function uniqueUsageBuckets(usage: readonly UsageObservation[]): readonly UsageObservation[] {
  return [...new Map(usage.map((observation) => [observation.bucketId, observation])).values()];
}

function bucketWindows(usage: readonly UsageObservation[], bucketId: string): string {
  const windows = [
    ...new Set(
      usage
        .filter((observation) => observation.bucketId === bucketId)
        .map((observation) => observation.window.kind),
    ),
  ];
  return windows.join(", ");
}

function updateScore(
  configuration: ExecutionConfigurationRecord,
  specialization: ExecutionConfigurationRecord["scores"][number]["specialization"],
  field: "suitability" | "quality" | "economy" | "preferenceOrder",
  value: number,
  change: QRL<(value: ExecutionConfigurationRecord) => void>,
): void {
  const maximum = field === "preferenceOrder" ? 10_000 : 100;
  if (!Number.isInteger(value) || value < 0 || value > maximum) return;
  void change({
    ...configuration,
    scores: configuration.scores.map((score) =>
      score.specialization === specialization
        ? { ...score, [field]: value, provenance: "owner" }
        : score,
    ),
  });
}

function usageValue(observation: UsageObservation): string {
  if (observation.status !== "observed") return "Usage unavailable · unknown, not 0%";
  if (observation.remainingPercent !== null)
    return String(observation.remainingPercent) + "% remaining";
  if (observation.exactRemainingTokens !== null)
    return observation.exactRemainingTokens + " tokens remaining";
  return "Remaining amount unavailable · unknown, not 0%";
}

function isStale(observation: UsageObservation, now: string, freshnessSeconds: number): boolean {
  return Date.parse(observation.observedAt) < Date.parse(now) - freshnessSeconds * 1_000;
}

function effortSummary(configuration: ExecutionConfigurationRecord): string {
  const effort = configuration.effort;
  return effort.mode === "configurable"
    ? effort.allowedValues.map(friendly).join(", ") +
        " · default " +
        (effort.defaultValue ?? "none")
    : effort.mode;
}

function contextSummary(configuration: ExecutionConfigurationRecord): string {
  const context = configuration.context;
  if (context.mode === "configurable")
    return (
      context.allowedTokens.map(formatTokens).join(", ") +
      " · default " +
      (context.defaultTokens === null ? "none" : formatTokens(context.defaultTokens))
    );
  if (context.mode === "fixed")
    return context.tokens === null
      ? "Fixed · size unavailable"
      : "Fixed · " + formatTokens(context.tokens);
  return context.mode;
}

function contextDetail(configuration: ExecutionConfigurationRecord): string {
  const context = configuration.context;
  return context.mode === "fixed" || context.mode === "inherited"
    ? context.source
    : context.mode === "unknown"
      ? context.reason
      : "";
}

function formatTokens(tokens: number): string {
  return tokens >= 1_000 ? String(tokens / 1_000) + "k" : String(tokens);
}

function friendly(value: string): string {
  return value.replaceAll("_", " ");
}

function agentLabel(value: ExecutionConnectionRecord["agentProduct"]): string {
  return value === "claude_code"
    ? "Claude Code"
    : value === "opencode"
      ? "OpenCode"
      : value === "qwen_code"
        ? "Qwen Code"
        : value === "zap_mock"
          ? "ZapMock synthetic"
          : "Codex";
}
