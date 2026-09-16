import { expectTypeOf, test } from "vitest";

import type { ActorId, Result, WorkspaceId } from "./index.ts";

/** @implements spec://org.vibevm.zap/lens/PROP-001#protocol */
test("semantic identifiers and typed failures stay distinct at compile time", () => {
  expectTypeOf<ActorId>().not.toEqualTypeOf<WorkspaceId>();
  expectTypeOf<Result<ActorId>>().toExtend<
    | { readonly ok: true; readonly value: ActorId }
    | { readonly ok: false; readonly error: { readonly code: string } }
  >();
});
