/** Sigma owner for the unified workspace canvas. @scope spec://org.vibevm.zap/lens/PROP-010#unified-canvas */
import { component$, useSignal, useVisibleTask$, type NoSerialize, type QRL } from "@qwik.dev/core";
import Sigma from "sigma";
import { EdgeArrowProgram } from "sigma/rendering";
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
    const renderer = new Sigma(projection.graph, container, {
      allowInvalidContainer: false,
      renderEdgeLabels: false,
      labelDensity: 0.15,
      labelGridCellSize: 90,
      labelRenderedSizeThreshold: 9,
      defaultEdgeColor: theme.edge,
      defaultNodeColor: theme.tones.neutral,
      labelColor: { color: theme.labels },
      edgeProgramClasses: { arrow: EdgeArrowProgram },
      stagePadding: 40,
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
      const display = renderer.getNodeDisplayData(node);
      if (display !== undefined)
        void renderer
          .getCamera()
          .animate({ x: display.x, y: display.y, ratio: 0.35 }, { duration: 300 });
      void props.onActivate$(node);
    });
    cleanup(() => {
      camera.value = { fitEpoch, state: renderer.getCamera().getState() };
      renderer.kill();
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
