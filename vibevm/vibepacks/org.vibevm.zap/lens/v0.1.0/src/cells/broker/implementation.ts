/**
 * Final assembly of the bounded broker operation groups.
 * @scope spec://org.vibevm.zap/lens/PROP-001#broker
 */
import { BindingAuthSchema, type BindingAuth, type Result } from "../protocol/index.ts";
import { fail, ok } from "./core.ts";
import { BrokerDatabase } from "./database.ts";
import { DeliveryOperations } from "./delivery.ts";
import type { LensBroker, OpenBrokerOptions } from "./index.ts";

class SQLiteLensBroker extends DeliveryOperations implements LensBroker {
  executePlan(auth: BindingAuth): Result<never> {
    return this.safe(() => {
      const actor = this.binding(BindingAuthSchema.parse(auth));
      if (!actor.ok) return actor;
      return fail(
        "unsupported_operation",
        "authority",
        "plan execution is not implemented in communication stage one",
        "request a preview only and wait for the admitted ZAP adapter stage",
      );
    });
  }

  close(): Result<null> {
    return this.closeDatabase();
  }
}

/** @implements spec://org.vibevm.zap/lens/PROP-001#broker */
export function openBroker(options: OpenBrokerOptions): Result<LensBroker> {
  if (options.databasePath.length === 0) {
    return fail(
      "invalid_input",
      "broker",
      "database path is empty",
      "provide :memory: or a user-local SQLite path",
    );
  }
  try {
    return ok(
      new SQLiteLensBroker(
        new BrokerDatabase(options.databasePath),
        options.clock ?? (() => new Date()),
      ),
    );
  } catch {
    return fail(
      "storage_failure",
      "broker",
      "broker store could not be opened",
      "verify the user-local directory is writable and inspect private diagnostics",
    );
  }
}
