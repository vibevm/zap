/** Authenticated agent planning route dispatch. @scope spec://org.vibevm.zap/lens/PROP-002#plan-control */
import { z } from "zod";
import type { PublicConnection, Result } from "../protocol/index.ts";
import { failure, type AdapterSessionId } from "../transport/index.ts";
import type { AgentPlanningPort } from "../workspace-planning/index.ts";

export async function executeAgentPlanning(
  port: AgentPlanningPort,
  operation: string,
  actor: PublicConnection,
  session: AdapterSessionId,
  body: unknown,
): Promise<Result<unknown>> {
  if (operation === "register") return port.register(actor, session, body);
  if (operation === "submit") return port.submit(actor, session, body);
  if (operation === "preview") return port.preview(actor, session, body);
  if (operation === "apply") return port.apply(actor, session, body);
  if (operation === "reconcile") return port.reconcile(actor, session, body);
  if (operation === "discover") return port.discover(actor, session);
  if (operation === "author") return port.author(actor, session, body);
  if (operation === "prepare-composite") return port.prepareComposite(actor, session, body);
  if (operation === "author-composite") return port.authorComposite(actor, session, body);
  if (operation === "prepare") {
    const parsed = z
      .object({ kind: z.enum(["bundle", "comparison", "projected_record"]), input: z.unknown() })
      .strict()
      .safeParse(body);
    return parsed.success
      ? port.prepare(actor, session, parsed.data.kind, parsed.data.input)
      : failure("invalid_input", "agent plan preparation request is malformed");
  }
  return failure("not_found", "agent planning route is unavailable");
}
