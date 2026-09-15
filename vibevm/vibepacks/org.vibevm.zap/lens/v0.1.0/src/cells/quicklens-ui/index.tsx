/**
 * Qwik CSR renderer seam shared by browser and Electron shells.
 * @scope spec://org.vibevm.zap/lens/PROP-002#shared-client
 */
import { noSerialize, render } from "@qwik.dev/core";

import type { QuicklensDataSource } from "../quicklens-model/index.ts";
import { QuicklensApp } from "./app.tsx";

export { QuicklensApp } from "./app.tsx";

export function mountQuicklens(container: Document | Element, source: QuicklensDataSource) {
  return render(container, <QuicklensApp source={noSerialize(source)} />);
}
