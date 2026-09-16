/** @scope spec://org.vibevm.zap/lens/PROP-001#verification */
import { expectTypeOf, test } from "vitest";
import type { Result } from "../protocol/index.ts";
import { routeSafePointHook } from "./index.ts";
import type {
  ActorRouteBinding,
  AdapterDiagnostic,
  DeliveryObservation,
  HookRoute,
  SafePointOffer,
} from "./index.ts";

test("public adapter seam preserves branded routing and tri-state evidence", () => {
  const result = routeSafePointHook("codex", {}, []);
  expectTypeOf(result).toEqualTypeOf<Result<HookRoute, AdapterDiagnostic>>();
  expectTypeOf<keyof ActorRouteBinding>().toEqualTypeOf<"actor" | "handle" | "host">();
  expectTypeOf<
    SafePointOffer["deliveries"][number]["observations"]["actorAcknowledged"]
  >().toEqualTypeOf<DeliveryObservation>();
});
