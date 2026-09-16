/** Reusable coordinator HTTP simulation document loader. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { CoordinatorHttpSimulationSchema } from "../mock-simulation/index.ts";

const path = fileURLToPath(new URL("./question-http.simulation.json", import.meta.url));
export const questionHttpBehavior = CoordinatorHttpSimulationSchema.parse(
  JSON.parse(readFileSync(path, "utf8")),
);
export const questionWakeScenario = questionHttpBehavior.scenarioFile.scenario;
