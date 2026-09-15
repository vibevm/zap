/** @scope spec://org.vibevm.zap/lens/PROP-002#semantic-map */
import { component$ } from "@qwik.dev/core";

import type { Metric, SemanticObject, SemanticRelationship } from "../quicklens-model/index.ts";

export interface ObjectCardProps {
  readonly object: SemanticObject | null;
  readonly relationships: readonly SemanticRelationship[];
  readonly titles: Readonly<Record<string, string>>;
}

export const ObjectCard = component$<ObjectCardProps>(({ object, relationships, titles }) => {
  if (object === null) {
    return (
      <section class="panel detail-empty">
        <p class="eyebrow">Selection</p>
        <h2>Choose an item</h2>
        <p>
          Select a node to inspect its purpose, status, relationships, acceptance, and provenance.
        </p>
      </section>
    );
  }
  return (
    <section class="panel detail-card">
      <div class="card-heading">
        <div>
          <p class="eyebrow">{object.semanticType.replaceAll("_", " ")}</p>
          <h2>{object.title}</h2>
        </div>
        <span class={`status status-${object.status.tone}`}>{object.status.label}</span>
      </div>
      <DetailText
        title="Description"
        value={object.description}
        missing="Description not supplied."
      />
      <DetailText title="Purpose" value={object.purpose} missing="Purpose not supplied." />
      <DetailText
        title="Expected result"
        value={object.expectedResult}
        missing="Expected result not supplied."
      />
      <h3>Planning signals</h3>
      <div class="metric-grid">
        <MetricView label="Complexity" metric={object.metrics.complexity} />
        <MetricView label="Difficulty" metric={object.metrics.difficulty} />
        <MetricView label="Effort" metric={object.metrics.effort} />
        <MetricView label="Waiting" metric={object.metrics.waiting} />
        <MetricView label="Uncertainty" metric={object.metrics.uncertainty} />
      </div>
      <h3>Acceptance</h3>
      <p class={object.acceptance === null ? "unknown-copy" : ""}>
        {object.acceptance ?? "No acceptance meaning supplied for this object."}
      </p>
      <h3>Reasons</h3>
      {object.reasons.length === 0 ? (
        <p class="unknown-copy">No reasons supplied.</p>
      ) : (
        <ul class="semantic-list">
          {object.reasons.map((reason) => (
            <li key={reason}>{reason}</li>
          ))}
        </ul>
      )}
      <h3>Blockers</h3>
      {object.blockerSummary === null ? null : <p>{object.blockerSummary}</p>}
      {object.blockers.length === 0 ? null : (
        <ul class="semantic-list">
          {object.blockers.map((blocker) => (
            <li key={blocker.ref}>{titles[blocker.ref] ?? blocker.fallbackLabel}</li>
          ))}
        </ul>
      )}
      <h3>Relationships</h3>
      {relationships.length === 0 ? (
        <p class="unknown-copy">No visible relationships.</p>
      ) : (
        <ul class="relationship-list">
          {relationships.map((relationship) => {
            const outward = relationship.source === object.ref;
            const other = outward ? relationship.target : relationship.source;
            return (
              <li key={relationship.ref}>
                <span>{outward ? "→" : "←"}</span>
                <strong>{relationship.label}</strong>
                <span>{titles[other] ?? "Related item"}</span>
                <small>{relationship.semanticType.replaceAll("_", " ")}</small>
              </li>
            );
          })}
        </ul>
      )}
      <h3>Provenance</h3>
      {object.provenance.length === 0 ? (
        <p class="unknown-copy">No provenance supplied.</p>
      ) : (
        <ul class="provenance-list">
          {object.provenance.map((item) => (
            <li key={`${item.sourceType}:${item.label}`}>
              <strong>{item.label}</strong>
              <span>{item.sourceType.replaceAll("_", " ")}</span>
              {item.detail === null ? null : <p>{item.detail}</p>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
});

const DetailText = component$<{
  readonly title: string;
  readonly value: string | null;
  readonly missing: string;
}>(({ title, value, missing }) => (
  <>
    <h3>{title}</h3>
    <p class={value === null ? "unknown-copy" : "detail-copy"}>{value ?? missing}</p>
  </>
));

const MetricView = component$<{ readonly label: string; readonly metric: Metric }>(
  ({ label, metric }) => (
    <div class="metric">
      <span>{label}</span>
      {metric.state === "known" ? (
        <>
          <strong>
            {metric.value}
            {metric.unit === null ? "" : ` ${metric.unit}`}
          </strong>
          {metric.explanation === null ? null : <small>{metric.explanation}</small>}
        </>
      ) : (
        <>
          <strong>Unknown</strong>
          <small>{metric.reason}</small>
        </>
      )}
    </div>
  ),
);
