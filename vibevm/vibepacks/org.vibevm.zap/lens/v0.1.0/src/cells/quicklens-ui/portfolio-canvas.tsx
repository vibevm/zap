/** Sigma owner for the unified workspace canvas. @scope spec://org.vibevm.zap/lens/PROP-010#unified-canvas */
import { component$, useSignal, useVisibleTask$, type NoSerialize, type QRL } from "@qwik.dev/core";
import Sigma from "sigma";
import { EdgeArrowProgram, type NodeLabelDrawingFunction } from "sigma/rendering";
import {
  DARK_GRAPH_THEME,
  LIGHT_GRAPH_THEME,
  type PortfolioProjection,
} from "../quicklens-graph/index.ts";

export const PortfolioCanvas = component$<{
  readonly projection: NoSerialize<PortfolioProjection>;
  readonly sceneKey: string;
  readonly selectedKey: string | null;
  readonly theme: "light" | "dark";
  readonly fitEpoch: number;
  readonly onSelect$: QRL<(key: string) => void>;
  readonly onActivate$: QRL<(key: string) => void>;
}>((props) => {
  const host = useSignal<HTMLDivElement>();
  const camera = useSignal<CameraMemory>();

  useVisibleTask$(({ cleanup, track }) => {
    track(() => props.sceneKey);
    track(() => props.selectedKey);
    track(() => props.theme);
    const fitEpoch = track(() => props.fitEpoch);
    const container = host.value;
    const projection = props.projection;
    if (container === undefined || projection === undefined) return;
    const theme = props.theme === "dark" ? DARK_GRAPH_THEME : LIGHT_GRAPH_THEME;
    let renderer: Sigma | null = null;
    let cancelled = false;
    const mount = () => {
      const bounds = container.getBoundingClientRect();
      if (cancelled || renderer !== null || !container.isConnected) return;
      if (bounds.width <= 0 || bounds.height <= 0) return;
      renderer = new Sigma(projection.graph, container, {
        allowInvalidContainer: false,
        renderEdgeLabels: false,
        labelDensity: 0.15,
        labelGridCellSize: 110,
        labelRenderedSizeThreshold: 9,
        defaultEdgeColor: theme.edge,
        defaultNodeColor: theme.tones.neutral,
        labelColor: { color: theme.labels },
        edgeProgramClasses: { arrow: EdgeArrowProgram },
        stagePadding: 56,
        defaultDrawNodeLabel: wrappedLabelRenderer(theme.background),
        nodeReducer: (_node, data) => (data["forceLabel"] ? data : { ...data, label: "" }),
      });
      if (camera.value?.fitEpoch === fitEpoch) renderer.getCamera().setState(camera.value.state);
      renderer.on("clickNode", ({ node }) => {
        if (!projection.graph.getNodeAttribute(node, "selectable")) return;
        void props.onSelect$(node);
      });
      renderer.on("doubleClickNode", ({ node }) => {
        const attributes = projection.graph.getNodeAttributes(node);
        if (!attributes.selectable) return;
        const display = renderer?.getNodeDisplayData(node);
        if (display !== undefined)
          void renderer
            ?.getCamera()
            .animate({ x: display.x, y: display.y, ratio: 0.35 }, { duration: 300 });
        void props.onActivate$(node);
      });
    };
    const observer = new ResizeObserver(mount);
    observer.observe(container);
    const frame = requestAnimationFrame(mount);
    cleanup(() => {
      cancelled = true;
      cancelAnimationFrame(frame);
      observer.disconnect();
      if (renderer !== null) {
        camera.value = { fitEpoch, state: renderer.getCamera().getState() };
        renderer.kill();
      }
    });
  });

  return <div ref={host} class="portfolio-canvas" aria-label="Unified multi-project canvas" />;
});

interface CameraMemory {
  readonly fitEpoch: number;
  readonly state: {
    readonly x: number;
    readonly y: number;
    readonly angle: number;
    readonly ratio: number;
  };
}

function wrappedLabelRenderer(background: string): NodeLabelDrawingFunction {
  return (context, data, settings) => {
    if (data.label === null) return;
    const lines = labelLines(data.label, 18);
    context.save();
    context.font = `${settings.labelWeight} ${String(settings.labelSize)}px ${settings.labelFont}`;
    context.textBaseline = "middle";
    const lineHeight = settings.labelSize + 2;
    const labelX = data.x + data.size + 4;
    const startY = data.y - ((lines.length - 1) * lineHeight) / 2;
    const width = Math.max(...lines.map((line) => context.measureText(line).width));
    context.globalAlpha = 0.94;
    context.fillStyle = background;
    context.fillRect(
      labelX - 2,
      startY - lineHeight / 2 - 1,
      width + 4,
      lines.length * lineHeight + 2,
    );
    context.globalAlpha = 1;
    context.fillStyle = "color" in settings.labelColor ? settings.labelColor.color : data.color;
    lines.forEach((line, index) => {
      context.fillText(line, labelX, startY + index * lineHeight);
    });
    context.restore();
  };
}

function labelLines(label: string, maximum: number): readonly string[] {
  if (label.length <= maximum) return [label];
  const words = label.split(/\s+/);
  const first: string[] = [];
  while (words.length > 0 && [...first, words[0]].join(" ").length <= maximum)
    first.push(words.shift() ?? "");
  const remainder = words.join(" ");
  return [
    first.join(" ") || label.slice(0, maximum),
    remainder.length <= maximum ? remainder : `${remainder.slice(0, maximum - 1)}…`,
  ];
}
