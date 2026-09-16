import { expect, test } from "vitest";

import { createQuicklensDemoDataSource } from "../quicklens-demo/index.ts";
import { QuicklensRefSchema, QuicklensSnapshotSchema } from "../quicklens-model/index.ts";
import {
  graphNodeLabel,
  layoutQuicklensGraph,
  LIGHT_GRAPH_THEME,
  projectQuicklensGraph,
} from "./index.ts";

async function demoSnapshot() {
  const result = await createQuicklensDemoDataSource().read({
    signal: new AbortController().signal,
  });
  if (!result.ok) throw new Error(result.error.message);
  return result.value;
}

/** @implements spec://org.vibevm.zap/lens/PROP-004#execution-layout */
test("work order follows prerequisite to dependent and keeps unordered branches in one stage", async () => {
  const snapshot = await demoSnapshot();
  const layout = layoutQuicklensGraph(snapshot, "work_order");
  expect(layout.stages.get(QuicklensRefSchema.parse("task.protocol"))).toBe(0);
  expect(layout.stages.get(QuicklensRefSchema.parse("task.renderer"))).toBe(1);
  expect(layout.stages.get(QuicklensRefSchema.parse("task.plan-adapter"))).toBe(1);
  expect(layout.stages.get(QuicklensRefSchema.parse("task.integration"))).toBe(2);
  expect(layout.positions.get(QuicklensRefSchema.parse("task.renderer"))?.x).toBe(
    layout.positions.get(QuicklensRefSchema.parse("task.plan-adapter"))?.x,
  );
  expect(layout.positions.get(QuicklensRefSchema.parse("task.protocol"))?.x).toBeLessThan(
    layout.positions.get(QuicklensRefSchema.parse("task.renderer"))?.x ?? -1,
  );
  const renderer = snapshot.objects.find((object) => object.ref === "task.renderer");
  expect(renderer === undefined ? "" : graphNodeLabel(renderer, layout)).toContain(
    "Current · Step 2",
  );
  const projection = projectQuicklensGraph(
    snapshot,
    { search: "", categories: [], statuses: [] },
    LIGHT_GRAPH_THEME,
    null,
    "work_order",
  );
  expect(
    projection.graph.getEdgeAttribute("edge.protocol.renderer.prerequisite", "edgeClass"),
  ).toBe("execution");
  expect(projection.graph.hasEdge("edge.strategy.renderer.contains")).toBe(false);
});

/** @implements spec://org.vibevm.zap/lens/PROP-004#partial-and-stability */
test("filtering preserves full-snapshot stages and missing prerequisites stay unresolved", async () => {
  const snapshot = await demoSnapshot();
  const full = projectQuicklensGraph(
    snapshot,
    { search: "", categories: [], statuses: [] },
    LIGHT_GRAPH_THEME,
    null,
    "work_order",
  );
  const filtered = projectQuicklensGraph(
    snapshot,
    { search: "reusable renderer", categories: [], statuses: [] },
    LIGHT_GRAPH_THEME,
    null,
    "work_order",
  );
  expect(filtered.graph.getNodeAttribute("task.renderer", "x")).toBe(
    full.graph.getNodeAttribute("task.renderer", "x"),
  );

  const incomplete = QuicklensSnapshotSchema.parse({
    ...snapshot,
    phase: "partial",
    objects: snapshot.objects.filter((object) => object.ref === "task.renderer"),
    relationships: [
      {
        ref: "edge.missing.renderer",
        source: "work:not-loaded",
        target: "task.renderer",
        semanticType: "work_prerequisite",
        label: "prerequisite",
        status: null,
        provenance: [],
      },
    ],
    navigation: {
      ...snapshot.navigation,
      activeOutcomeRef: null,
      currentStrategyRef: null,
      adoptedPlanRef: null,
      members: [],
      focusRef: null,
      currentWorkRefs: [],
      dependencyDirection: "prerequisite_to_dependent",
      completeness: "partial",
      reassessmentReason: "A prerequisite endpoint is not loaded.",
    },
  });
  const missing = layoutQuicklensGraph(incomplete, "work_order");
  expect(missing.roles.get(QuicklensRefSchema.parse("task.renderer"))).toBe("unresolved");
  expect(missing.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
    "missing_prerequisite",
  );
});

test("prerequisite cycles are labelled without an invented order", async () => {
  const source = await demoSnapshot();
  const tasks = source.objects.filter(
    (object) => object.ref === "task.renderer" || object.ref === "task.plan-adapter",
  );
  const snapshot = QuicklensSnapshotSchema.parse({
    ...source,
    objects: tasks,
    relationships: [
      {
        ref: "edge.renderer.adapter",
        source: "task.renderer",
        target: "task.plan-adapter",
        semanticType: "work_prerequisite",
        label: "prerequisite",
        status: null,
        provenance: [],
      },
      {
        ref: "edge.adapter.renderer",
        source: "task.plan-adapter",
        target: "task.renderer",
        semanticType: "work_prerequisite",
        label: "prerequisite",
        status: null,
        provenance: [],
      },
    ],
    navigation: {
      ...source.navigation,
      activeOutcomeRef: null,
      currentStrategyRef: null,
      adoptedPlanRef: null,
      members: [],
      focusRef: null,
      currentWorkRefs: [],
      dependencyDirection: "prerequisite_to_dependent",
      completeness: "complete",
      reassessmentReason: null,
    },
  });
  const layout = layoutQuicklensGraph(snapshot, "work_order");
  expect(layout.stages.size).toBe(0);
  expect(layout.diagnostics.map((diagnostic) => diagnostic.code)).toContain("cycle");
  expect(layout.roles.get(QuicklensRefSchema.parse("task.renderer"))).toBe("cycle");
});

test("parallel prerequisite meanings count as one ordering arc", async () => {
  const source = await demoSnapshot();
  const snapshot = QuicklensSnapshotSchema.parse({
    ...source,
    objects: source.objects.filter(
      (object) => object.ref === "task.protocol" || object.ref === "task.renderer",
    ),
    relationships: [
      {
        ref: "edge.preparation",
        source: "task.protocol",
        target: "task.renderer",
        semanticType: "milestone_preparation_prerequisite",
        label: "preparation prerequisite",
        status: null,
        provenance: [],
      },
      {
        ref: "edge.achievement",
        source: "task.protocol",
        target: "task.renderer",
        semanticType: "milestone_achievement_prerequisite",
        label: "achievement prerequisite",
        status: null,
        provenance: [],
      },
    ],
  });
  const layout = layoutQuicklensGraph(snapshot, "work_order");
  expect(layout.stages.get(QuicklensRefSchema.parse("task.protocol"))).toBe(0);
  expect(layout.stages.get(QuicklensRefSchema.parse("task.renderer"))).toBe(1);
  expect(layout.diagnostics.map((diagnostic) => diagnostic.code)).not.toContain("cycle");
});

test("a DAG edge into a cycle cannot leave a provisional step label", async () => {
  const source = await demoSnapshot();
  const snapshot = QuicklensSnapshotSchema.parse({
    ...source,
    objects: source.objects.filter((object) =>
      ["task.protocol", "task.renderer", "task.plan-adapter"].includes(object.ref),
    ),
    relationships: [
      {
        ref: "edge.root.renderer",
        source: "task.protocol",
        target: "task.renderer",
        semanticType: "work_prerequisite",
        label: "prerequisite",
        status: null,
        provenance: [],
      },
      {
        ref: "edge.renderer.adapter",
        source: "task.renderer",
        target: "task.plan-adapter",
        semanticType: "work_prerequisite",
        label: "prerequisite",
        status: null,
        provenance: [],
      },
      {
        ref: "edge.adapter.renderer",
        source: "task.plan-adapter",
        target: "task.renderer",
        semanticType: "work_prerequisite",
        label: "prerequisite",
        status: null,
        provenance: [],
      },
    ],
  });
  const layout = layoutQuicklensGraph(snapshot, "work_order");
  expect(layout.stages.get(QuicklensRefSchema.parse("task.protocol"))).toBe(0);
  expect(layout.stages.has(QuicklensRefSchema.parse("task.renderer"))).toBe(false);
  expect(layout.stages.has(QuicklensRefSchema.parse("task.plan-adapter"))).toBe(false);
  const projection = projectQuicklensGraph(
    snapshot,
    { search: "", categories: [], statuses: [] },
    LIGHT_GRAPH_THEME,
    null,
    "work_order",
  );
  expect(projection.graph.getEdgeAttribute("edge.root.renderer", "label")).toContain(
    "order unresolved",
  );
});
