import { expect, test } from "vitest";

import { createQuicklensDemoDataSource } from "../quicklens-demo/index.ts";
import { QuicklensRefSchema } from "../quicklens-model/index.ts";
import {
  DARK_GRAPH_THEME,
  filterQuicklensObjects,
  layoutQuicklensGraph,
  LIGHT_GRAPH_THEME,
  projectQuicklensGraph,
} from "./index.ts";

function contrastRatio(foreground: string, background: string): number {
  const luminance = (hex: string): number => {
    const channels = [1, 3, 5].map((offset) => Number.parseInt(hex.slice(offset, offset + 2), 16));
    const linear = channels.map((value) => {
      const normalized = value / 255;
      return normalized <= 0.04045 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
    });
    const [red = 0, green = 0, blue = 0] = linear;
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
  };
  const foregroundLuminance = luminance(foreground);
  const backgroundLuminance = luminance(background);
  return (
    (Math.max(foregroundLuminance, backgroundLuminance) + 0.05) /
    (Math.min(foregroundLuminance, backgroundLuminance) + 0.05)
  );
}

async function demoSnapshot() {
  const result = await createQuicklensDemoDataSource().read({
    signal: new AbortController().signal,
  });
  if (!result.ok) throw new Error(result.error.message);
  return result.value;
}

/** @implements spec://org.vibevm.zap/lens/PROP-004#goal-layout */
test("goal view anchors the explicit goal and preserves semantic edge classes", async () => {
  const snapshot = await demoSnapshot();
  const projection = projectQuicklensGraph(
    snapshot,
    { search: "", categories: [], statuses: [] },
    LIGHT_GRAPH_THEME,
    null,
  );
  expect(projection.graph.multi).toBe(true);
  expect(projection.graph.type).toBe("directed");
  expect(projection.graph.getNodeAttribute("outcome.quicklens", "x")).toBe(0);
  expect(projection.graph.getNodeAttribute("outcome.quicklens", "y")).toBe(0);
  expect(projection.graph.getNodeAttribute("outcome.quicklens", "label")).toContain("Goal ·");
  expect(
    projection.graph.getEdgeAttribute("edge.renderer.quicklens.contributes", "edgeClass"),
  ).toBe("hierarchy");
  expect(projection.graph.hasEdge("edge.strategy.renderer.contains")).toBe(false);
  expect(projection.graph.getEdgeAttribute("edge.broker.protocol.supports", "edgeClass")).toBe(
    "reference",
  );
  expect(projection.graph.getNodeAttribute("fact.host-limit", "forceLabel")).toBe(true);
  const layout = layoutQuicklensGraph(snapshot, "goal");
  const communication = layout.positions.get(QuicklensRefSchema.parse("milestone.communication"));
  const protocol = layout.positions.get(QuicklensRefSchema.parse("task.protocol"));
  const quicklens = layout.positions.get(QuicklensRefSchema.parse("milestone.quicklens"));
  const renderer = layout.positions.get(QuicklensRefSchema.parse("task.renderer"));
  expect(communication?.x).toBeLessThan(0);
  expect(protocol?.x).toBeLessThan(communication?.x ?? 0);
  expect(quicklens?.x).toBeGreaterThan(0);
  expect(renderer?.x).toBeGreaterThan(quicklens?.x ?? 0);
  expect(layout.roles.get(QuicklensRefSchema.parse("task.integration"))).toBe("unassigned");
  expect(layout.roles.get(QuicklensRefSchema.parse("strategy.quicklens"))).toBe("strategy");
  projection.graph.forEachNode((_node, attributes) => {
    expect(Number.isFinite(attributes.x)).toBe(true);
    expect(Number.isFinite(attributes.y)).toBe(true);
  });
});

test("search and broad category filters retain original semantic types", async () => {
  const projection = projectQuicklensGraph(
    await demoSnapshot(),
    { search: "idle wake", categories: ["other"], statuses: [] },
    LIGHT_GRAPH_THEME,
    null,
  );
  expect(projection.graph.order).toBe(1);
  expect(projection.graph.getNodeAttribute("fact.host-limit", "category")).toBe("other");
  expect(projection.graph.getNodeAttribute("fact.host-limit", "semanticType")).toBe(
    "verified_host_fact",
  );
});

test("selection uses a theme-safe node treatment without Sigma's white hover card", async () => {
  const projection = projectQuicklensGraph(
    await demoSnapshot(),
    { search: "", categories: [], statuses: [] },
    LIGHT_GRAPH_THEME,
    QuicklensRefSchema.parse("task.renderer"),
  );
  expect(projection.graph.getNodeAttribute("task.renderer", "color")).toBe(
    LIGHT_GRAPH_THEME.selectedRing,
  );
  expect(projection.graph.getNodeAttribute("task.renderer", "forceLabel")).toBe(true);
  expect(projection.graph.getNodeAttribute("task.renderer", "highlighted")).toBe(false);
});

test("visible-object feedback uses the same filters as the graph projection", async () => {
  const snapshot = await demoSnapshot();
  const filters = { search: "no such planning object", categories: [], statuses: [] } as const;
  expect(filterQuicklensObjects(snapshot, filters)).toHaveLength(0);
  expect(projectQuicklensGraph(snapshot, filters, LIGHT_GRAPH_THEME, null).graph.order).toBe(0);
});

test("light and dark graph labels retain readable contrast", () => {
  expect(
    contrastRatio(LIGHT_GRAPH_THEME.labels, LIGHT_GRAPH_THEME.background),
  ).toBeGreaterThanOrEqual(4.5);
  expect(
    contrastRatio(DARK_GRAPH_THEME.labels, DARK_GRAPH_THEME.background),
  ).toBeGreaterThanOrEqual(4.5);
});
