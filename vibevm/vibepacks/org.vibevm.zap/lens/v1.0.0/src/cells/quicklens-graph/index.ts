/**
 * Pure graph projection for Sigma; coordinates are presentation only.
 * @scope spec://org.vibevm.zap/lens/PROP-002#semantic-map
 */
import { MultiDirectedGraph } from "graphology";

import type {
  ObjectCategory,
  QuicklensRef,
  QuicklensSnapshot,
  SemanticObject,
  StatusTone,
} from "../quicklens-model/index.ts";
import {
  classifyGraphEdge,
  graphEdgeLabel,
  graphNodeLabel,
  layoutQuicklensGraph,
  relationshipVisibleInGraph,
  type GraphEdgeClass,
  type GraphLayoutDiagnostic,
  type GraphView,
} from "./layout.ts";

export type { GraphLayout, GraphLayoutDiagnostic, GraphView } from "./layout.ts";
export {
  classifyGraphEdge,
  graphNodeLabel,
  layoutQuicklensGraph,
  relationshipVisibleInGraph,
} from "./layout.ts";
export {
  projectPortfolioGraph,
  type PortfolioCard,
  type PortfolioEdgeAttributes,
  type PortfolioGraph,
  type PortfolioNodeAttributes,
  type PortfolioNodeKind,
  type PortfolioProjection,
  type PortfolioViewState,
} from "./portfolio.ts";

export interface GraphFilters {
  readonly search: string;
  readonly categories: readonly ObjectCategory[];
  readonly statuses: readonly string[];
}

export interface GraphTheme {
  readonly background: string;
  readonly edge: string;
  readonly mutedEdge: string;
  readonly labels: string;
  readonly selectedRing: string;
  readonly edgeClasses: Readonly<Record<GraphEdgeClass, string>>;
  readonly tones: Readonly<Record<StatusTone, string>>;
}

export interface QuicklensNodeAttributes extends Record<string, unknown> {
  readonly label: string;
  readonly x: number;
  readonly y: number;
  readonly size: number;
  readonly color: string;
  readonly category: ObjectCategory;
  readonly semanticType: string;
  readonly statusCode: string;
  readonly statusLabel: string;
  readonly highlighted: boolean;
  readonly forceLabel: boolean;
}

export interface QuicklensEdgeAttributes extends Record<string, unknown> {
  readonly label: string;
  readonly color: string;
  readonly size: number;
  readonly semanticType: string;
  readonly edgeClass: GraphEdgeClass;
  readonly type: "arrow" | "line";
}

export type QuicklensGraph = MultiDirectedGraph<
  QuicklensNodeAttributes,
  QuicklensEdgeAttributes,
  Record<string, unknown>
>;

export interface GraphProjection {
  readonly graph: QuicklensGraph;
  readonly visibleRefs: readonly QuicklensRef[];
  readonly diagnostics: readonly GraphLayoutDiagnostic[];
}

export function relationshipsShownInGraph(
  snapshot: QuicklensSnapshot,
  view: GraphView,
): readonly QuicklensSnapshot["relationships"][number][] {
  const contributedWork = new Set(
    snapshot.relationships
      .filter((relationship) => normalizedKind(relationship.semanticType) === "contributes_to")
      .map((relationship) => relationship.source),
  );
  return snapshot.relationships.filter(
    (relationship) =>
      relationshipVisibleInGraph(relationship, view) &&
      !(
        view === "goal" &&
        normalizedKind(relationship.semanticType) === "contains" &&
        (relationship.source === snapshot.navigation?.currentStrategyRef ||
          contributedWork.has(relationship.target))
      ),
  );
}

export const LIGHT_GRAPH_THEME: GraphTheme = {
  background: "#f0eee6",
  edge: "#8e8d85",
  mutedEdge: "#c9c6ba",
  labels: "#1f1e1b",
  selectedRing: "#d97757",
  edgeClasses: {
    execution: "#3d63e9",
    hierarchy: "#7d8a5f",
    resource: "#d4a27f",
    reference: "#8e8d85",
    other: "#b8b6ad",
  },
  tones: {
    neutral: "#8e8d85",
    active: "#3d63e9",
    success: "#3e7b47",
    warning: "#d4a27f",
    danger: "#c15f3c",
  },
};

export const DARK_GRAPH_THEME: GraphTheme = {
  background: "#40403e",
  edge: "#b8b6ad",
  mutedEdge: "#63625b",
  labels: "#faf9f5",
  selectedRing: "#d97757",
  edgeClasses: {
    execution: "#7b7ee0",
    hierarchy: "#a8b49a",
    resource: "#d4a27f",
    reference: "#b8b6ad",
    other: "#63625b",
  },
  tones: {
    neutral: "#b8b6ad",
    active: "#7b7ee0",
    success: "#7d9b76",
    warning: "#d4a27f",
    danger: "#ebcbbc",
  },
};

export function projectQuicklensGraph(
  snapshot: QuicklensSnapshot,
  filters: GraphFilters,
  theme: GraphTheme,
  selectedRef: QuicklensRef | null,
  view: GraphView = "goal",
): GraphProjection {
  const graph: QuicklensGraph = new MultiDirectedGraph({ allowSelfLoops: true });
  const layout = layoutQuicklensGraph(snapshot, view);
  const visibleObjects = filterQuicklensObjects(snapshot, filters);
  const visibleRefs = new Set(visibleObjects.map((object) => object.ref));
  const shownRelationships = relationshipsShownInGraph(snapshot, view);
  const boundedLabels = visibleObjects.length > 24;
  visibleObjects.forEach((object) => {
    const position = layout.positions.get(object.ref) ?? { x: 0, y: 0 };
    const role = layout.roles.get(object.ref) ?? "context";
    const explanatory = role !== "context";
    graph.addNode(object.ref, {
      label: graphNodeLabel(object, layout),
      x: position.x,
      y: position.y,
      size:
        role === "goal"
          ? 17
          : object.ref === selectedRef
            ? 13
            : role === "member" || role === "focus"
              ? 11
              : explanatory
                ? 9
                : 6,
      color: object.ref === selectedRef ? theme.selectedRing : theme.tones[object.status.tone],
      category: object.category,
      semanticType: object.semanticType,
      statusCode: object.status.code,
      statusLabel: object.status.label,
      highlighted: false,
      forceLabel: object.ref === selectedRef || explanatory || !boundedLabels,
    });
  });
  const pairCounts = new Map<string, number>();
  for (const relationship of shownRelationships) {
    const pair = `${relationship.source}\u0000${relationship.target}`;
    pairCounts.set(pair, (pairCounts.get(pair) ?? 0) + 1);
  }
  const renderedPairs = new Set<string>();
  for (const relationship of shownRelationships) {
    if (!visibleRefs.has(relationship.source) || !visibleRefs.has(relationship.target)) continue;
    const pair = `${relationship.source}\u0000${relationship.target}`;
    const parallelCount = pairCounts.get(pair) ?? 1;
    const semanticLabel = graphEdgeLabel(relationship, layout);
    const label =
      parallelCount === 1
        ? semanticLabel
        : renderedPairs.has(pair)
          ? ""
          : `${String(parallelCount)} relationships · ${semanticLabel}`;
    renderedPairs.add(pair);
    const edgeClass = classifyGraphEdge(relationship);
    graph.addDirectedEdgeWithKey(relationship.ref, relationship.source, relationship.target, {
      label,
      color: theme.edgeClasses[edgeClass],
      size: edgeClass === "execution" ? 2.6 : edgeClass === "hierarchy" ? 1.8 : 1.2,
      semanticType: relationship.semanticType,
      edgeClass,
      type: edgeClass === "execution" ? "arrow" : "line",
    });
  }
  if (view === "goal") addNavigationEdges(graph, snapshot, visibleRefs, theme);
  return { graph, visibleRefs: [...visibleRefs], diagnostics: layout.diagnostics };
}

function normalizedKind(value: string): string {
  return value.trim().toLocaleLowerCase().replaceAll("-", "_");
}

function addNavigationEdges(
  graph: QuicklensGraph,
  snapshot: QuicklensSnapshot,
  visibleRefs: ReadonlySet<QuicklensRef>,
  theme: GraphTheme,
): void {
  const goal = snapshot.navigation?.activeOutcomeRef;
  if (goal === null || goal === undefined || !visibleRefs.has(goal)) return;
  const targets = [
    ...(snapshot.navigation?.members.map((member) => ({
      ref: member.ref,
    })) ?? []),
  ];
  targets.forEach((target, index) => {
    if (!visibleRefs.has(target.ref)) return;
    graph.addDirectedEdgeWithKey(
      `navigation:${String(index)}:${goal}:${target.ref}`,
      goal,
      target.ref,
      {
        label: "",
        color: theme.edgeClasses.hierarchy,
        size: 2,
        semanticType: "navigation_membership",
        edgeClass: "hierarchy",
        type: "line",
      },
    );
  });
}

/** @implements spec://org.vibevm.zap/lens/PROP-002#semantic-map */
export function filterQuicklensObjects(
  snapshot: QuicklensSnapshot,
  filters: GraphFilters,
): readonly SemanticObject[] {
  return snapshot.objects.filter((object) => matches(object, filters));
}

function matches(object: SemanticObject, filters: GraphFilters): boolean {
  const query = filters.search.trim().toLocaleLowerCase();
  const searchable = [
    object.title,
    object.description ?? "",
    object.purpose ?? "",
    object.expectedResult ?? "",
    object.acceptance ?? "",
    object.reasons.join(" "),
    object.blockerSummary ?? "",
    object.semanticType,
    object.status.label,
  ]
    .join(" ")
    .toLocaleLowerCase();
  const categoryMatch =
    filters.categories.length === 0 || filters.categories.includes(object.category);
  const statusMatch =
    filters.statuses.length === 0 || filters.statuses.includes(object.status.code);
  return categoryMatch && statusMatch && (query.length === 0 || searchable.includes(query));
}
