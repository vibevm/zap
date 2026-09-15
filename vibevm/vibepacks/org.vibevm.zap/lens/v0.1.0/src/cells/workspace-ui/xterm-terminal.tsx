/** @scope spec://org.vibevm.zap/lens/PROP-006#terminal-evidence */
/** Lazy xterm renderer for a real managed-terminal output stream. */
import {
  component$,
  noSerialize,
  useSignal,
  useVisibleTask$,
  type NoSerialize,
  type QRL,
} from "@qwik.dev/core";
import type { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

export const XtermTerminal = component$<{
  readonly enabled: boolean;
  readonly output: string;
  readonly inputEnabled?: boolean | undefined;
  readonly onInput$?: QRL<(data: string) => void> | undefined;
  readonly onResize$?: QRL<(columns: number, rows: number) => void> | undefined;
}>((props) => {
  const host = useSignal<HTMLElement>();
  const terminal = useSignal<NoSerialize<Terminal>>();
  const written = useSignal(0);
  useVisibleTask$(({ cleanup }) => {
    if (!props.enabled || host.value === undefined) return;
    let observer: ResizeObserver | undefined;
    void Promise.all([import("@xterm/xterm"), import("@xterm/addon-fit")]).then(
      ([{ Terminal }, { FitAddon }]) => {
        if (host.value === undefined) return;
        const instance = new Terminal({ convertEol: true, cursorBlink: false, disableStdin: true });
        const fit = new FitAddon();
        instance.loadAddon(fit);
        instance.open(host.value);
        fit.fit();
        terminal.value = noSerialize(instance);
        observer = new ResizeObserver(() => {
          fit.fit();
          if (props.onResize$ !== undefined) {
            void props.onResize$(instance.cols, instance.rows);
          }
        });
        observer.observe(host.value);
      },
    );
    cleanup(() => {
      observer?.disconnect();
      terminal.value?.dispose();
      terminal.value = undefined;
    });
  });
  useVisibleTask$(({ track }) => {
    const output = track(() => props.output);
    const instance = track(() => terminal.value);
    if (instance === undefined || output.length <= written.value) return;
    instance.write(output.slice(written.value));
    written.value = output.length;
  });
  useVisibleTask$(({ track, cleanup }) => {
    const instance = track(() => terminal.value);
    const inputEnabled = track(() => props.inputEnabled === true);
    if (instance === undefined) return;
    instance.options.disableStdin = !inputEnabled;
    if (!inputEnabled || props.onInput$ === undefined) return;
    const subscription = instance.onData((data) => {
      void props.onInput$?.(data);
    });
    cleanup(() => {
      subscription.dispose();
    });
  });
  return <div class="xterm-terminal-host" ref={host} aria-label="Managed terminal output" />;
});
