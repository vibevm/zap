/** Public zero-LLM simulation corpus seam. @scope spec://org.vibevm.zap/lens/PROP-013#scenario-corpus */
export { defaultMockSimulationRoot, loadMockSimulationCorpus } from "./corpus.ts";
export type { MockCorpusResult, MockSimulationCorpus } from "./corpus.ts";
export { runMockSimulations } from "./runner.ts";
export type {
  MockSimulationEvidenceKind,
  MockSimulationRunInput,
  MockSimulationRunReceipt,
  MockSimulationScenarioReceipt,
} from "./runner.ts";
export {
  CoordinatorHttpSimulationSchema,
  ManagedProductSimulationSchema,
  MockSimulationDocumentSchema,
  MockSimulationRegistrySchema,
  MockSimulationRunnerIdSchema,
  ModelReducerSimulationSchema,
  WorkspaceStoreSimulationSchema,
  AnnotationsProductSimulationSchema,
  ModelPolicyProductSimulationSchema,
  RepositoryProductSimulationSchema,
  RepositoryManagedProductSimulationSchema,
  ProviderProjectionSimulationSchema,
} from "./schemas.ts";
export type {
  MockSimulationDocument,
  MockSimulationRegistry,
  MockSimulationRunnerId,
} from "./schemas.ts";
