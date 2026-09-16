/**
 * Sigma lifecycle owner for the Quicklens renderer.
 * @scope spec://org.vibevm.zap/lens/PROP-002#semantic-map
 */
import { component$, useSignal, useVisibleTask$, type QRL } from "@qwik.dev/core";
import Sigma from "sigma";
import {
  drawDiscNodeLabel,
  EdgeArrowProgram,
  type NodeLabelDrawingFunction,
  type NodeHoverDrawingFunction,
} from "sigma/rendering";

import {
  DARK_GRAPH_THEME,
  LIGHT_GRAPH_THEME,
  projectQuicklensGraph,
  type GraphFilters,
  type GraphView,
} from "../quicklens-graph/index.ts";
import {
  QuicklensRefSchema,
  type QuicklensRef,
  type QuicklensSnapshot,
} from "../quicklens-model/index.ts";

export interface GraphCanvasProps {
  readonly snapshot: QuicklensSnapshot;
  readonly filters: GraphFilters;
  readonly selectedRef: QuicklensRef | null;
  readonly theme: "light" | "dark";
  readonly view: GraphView;
  readonly onSelect$: QRL<(ref: QuicklensRef) => void>;
}

export const GraphCanvas = component$<GraphCanvasProps>((props) => {
  const host = useSignal<HTMLDivElement>();
  const cameraMemory = useSignal<CameraMemory>();

  useVisibleTask$(({ cleanup, track }) => {
    track(() => props.snapshot.revision);
    track(() => props.filters.search);
    track(() => props.filters.categories.join(","));
    track(() => props.filters.statuses.join(","));
    track(() => props.selectedRef);
    track(() => props.theme);
    track(() => props.view);
    const container = host.value;
    if (container === undefined) return;
    const theme = props.theme === "dark" ? DARK_GRAPH_THEME : LIGHT_GRAPH_THEME;
    const projection = projectQuicklensGraph(
      props.snapshot,
      props.filters,
      theme,
      props.selectedRef,
      props.view,
    );
    const renderer = new Sigma(projection.graph, container, {
      allowInvalidContainer: false,
      renderEdgeLabels: projection.graph.size < 80,
      labelDensity: 0.1,
      labelGridCellSize: 80,
      labelRenderedSizeThreshold: 7,
      defaultEdgeColor: theme.edge,
      defaultNodeColor: theme.tones.neutral,
      labelColor: { color: theme.labels },
      edgeLabelColor: { color: theme.labels },
      edgeProgramClasses: { arrow: EdgeArrowProgram },
      defaultDrawNodeLabel: semanticNodeLabel(theme.labels),
      defaultDrawNodeHover: themedNodeHover(theme.background, theme.selectedRing),
      stagePadding: 210,
    });
    const contextKey = graphContextKey(projection.graph);
    const previousCamera = cameraMemory.value;
    if (previousCamera?.contextKey === contextKey) {
      renderer.getCamera().setState(previousCamera.state);
    }
    renderer.on("clickNode", ({ node }) => {
      const parsed = QuicklensRefSchema.safeParse(node);
      if (parsed.success) void props.onSelect$(parsed.data);
    });
    cleanup(() => {
      cameraMemory.value = { contextKey, state: renderer.getCamera().getState() };
      renderer.kill();
    });
  });

  return (
    <div
      ref={host}
      class="graph-canvas"
      aria-label={
        props.view === "goal" ? "Goal decomposition graph" : "Prerequisite work-order graph"
      }
    />
  );
});

interface CameraMemory {
  readonly contextKey: string;
  readonly state: {
    readonly x: number;
    readonly y: number;
    readonly angle: number;
    readonly ratio: number;
  };
}

function graphContextKey(graph: ReturnType<typeof projectQuicklensGraph>["graph"]): string {
  return JSON.stringify({
    nodes: graph
      .nodes()
      .sort()
      .map((node) => [node, graph.getNodeAttribute(node, "x"), graph.getNodeAttribute(node, "y")]),
    edges: graph
      .edges()
      .sort()
      .map((edge) => [edge, graph.source(edge), graph.target(edge)]),
  });
}

function themedNodeHover(background: string, ring: string): NodeHoverDrawingFunction {
  return (context, data, settings) => {
    context.save();
    context.beginPath();
    context.arc(data.x, data.y, data.size + 3, 0, Math.PI * 2);
    context.fillStyle = background;
    context.fill();
    context.lineWidth = 2;
    context.strokeStyle = ring;
    context.stroke();
    drawDiscNodeLabel(context, data, settings);
    context.restore();
  };
}

function semanticNodeLabel(color: string): NodeLabelDrawingFunction {
  return (context, data) => {
    if (data.label === null) return;
    const [prefix = "", ...titleParts] = data.label.split(" · ");
    const title = titleParts.join(" · ");
    const structured = title.length > 0;
    const goal = prefix === "Goal";
    const offset = data.size + 7;
    context.save();
    context.fillStyle = color;
    context.textBaseline = "middle";
    if (goal) {
      context.font = '650 13px Inter, "Segoe UI", sans-serif';
      const lines = measuredLines(context, title, 220, 2);
      context.textAlign = "center";
      context.font = '700 10px Inter, "Segoe UI", sans-serif';
      const top = data.y - data.size - 9 - lines.length * 14;
      context.fillText(prefix.toUpperCase(), data.x, top);
      context.font = '650 13px Inter, "Segoe UI", sans-serif';
      drawTextLines(context, lines, data.x, top + 14);
    } else if (structured) {
      context.font = '600 12px Inter, "Segoe UI", sans-serif';
      const contextLabel = ["Context", "Strategy context", "Unassigned"].includes(prefix);
      const lines = measuredLines(context, title, contextLabel ? 130 : 180, 2);
      const placement = fittedPlacement(context, data.x, offset, lines, contextLabel);
      context.textAlign = placement.alignment;
      context.font = '700 9px Inter, "Segoe UI", sans-serif';
      context.fillText(prefix.toUpperCase(), placement.x, data.y - lines.length * 7);
      context.font = '600 12px Inter, "Segoe UI", sans-serif';
      drawTextLines(context, lines, placement.x, data.y - (lines.length - 2) * 7 + 7);
    } else {
      context.font = '600 12px Inter, "Segoe UI", sans-serif';
      const lines = measuredLines(context, data.label, 180, 2);
      const placement = fittedPlacement(context, data.x, offset, lines, false);
      context.textAlign = placement.alignment;
      drawTextLines(context, lines, placement.x, data.y - ((lines.length - 1) * 14) / 2);
    }
    context.restore();
  };
}

function measuredLines(
  context: CanvasRenderingContext2D,
  text: string,
  maximumWidth: number,
  maximumLines: number,
): string[] {
  const words = text.trim().split(/\s+/).filter(Boolean);
  const lines: string[] = [];
  let current = "";
  for (const word of words) {
    const candidate = current.length === 0 ? word : `${current} ${word}`;
    if (context.measureText(candidate).width <= maximumWidth) {
      current = candidate;
      continue;
    }
    if (current.length > 0) lines.push(current);
    current = fitToken(context, word, maximumWidth);
    if (lines.length === maximumLines - 1) break;
  }
  if (current.length > 0 && lines.length < maximumLines) lines.push(current);
  const represented = lines.join(" ");
  if (represented.length < text.trim().length && lines.length > 0) {
    lines[lines.length - 1] = fitToken(context, `${lines.at(-1) ?? ""}…`, maximumWidth);
  }
  return lines.length === 0 ? [""] : lines;
}

function fitToken(context: CanvasRenderingContext2D, value: string, maximumWidth: number): string {
  if (context.measureText(value).width <= maximumWidth) return value;
  let fitted = value;
  while (fitted.length > 1 && context.measureText(`${fitted}…`).width > maximumWidth) {
    fitted = fitted.slice(0, -1);
  }
  return `${fitted}…`;
}

function fittedPlacement(
  context: CanvasRenderingContext2D,
  nodeX: number,
  offset: number,
  lines: readonly string[],
  inward: boolean,
): { readonly x: number; readonly alignment: "left" | "right" } {
  const width = Math.max(...lines.map((line) => context.measureText(line).width));
  const canvasWidth = context.canvas.width;
  const preferLabelOnRight = inward ? nodeX < canvasWidth / 2 : nodeX >= canvasWidth / 2;
  const rightX = nodeX + offset;
  const leftX = nodeX - offset;
  if (preferLabelOnRight && rightX + width <= canvasWidth - 8) {
    return { x: rightX, alignment: "left" };
  }
  if (!preferLabelOnRight && leftX - width >= 8) return { x: leftX, alignment: "right" };
  return preferLabelOnRight ? { x: leftX, alignment: "right" } : { x: rightX, alignment: "left" };
}

function drawTextLines(
  context: CanvasRenderingContext2D,
  lines: readonly string[],
  x: number,
  firstY: number,
): void {
  lines.forEach((line, index) => {
    context.fillText(line, x, firstY + index * 14);
  });
}
