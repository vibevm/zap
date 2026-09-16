/** Normal Zap Quick Lens startup and project registration. @scope spec://org.vibevm.zap/lens/PROP-010#start-and-projects */
export { createDynamicWorkspacePort } from "./dynamic-workspace.ts";
export {
  createProductAppService,
  type ProductAppService,
  type ProductExecutionConfigurationResolver,
} from "./service.ts";
export { startLocalProductUi, type LocalProductUi, type ProductUiResult } from "./local-ui.ts";
export {
  loadProductLocalSettings,
  ProductLocalSettingsSchema,
  ManagedWorkerTemplateSchema,
  ProductExecutionBindingSchema,
  type ManagedWorkerTemplate,
  type ProductExecutionBinding,
  type ProductLocalSettings,
} from "./settings.ts";
export {
  discoverLocalProductProviders,
  type LocalProductLaunchTemplate,
  type LocalProductProviders,
} from "./providers.ts";
export { createProtectedEnvironmentResolver } from "./environment.ts";
export {
  openProductAppRegistry,
  planRegistrationOf,
  registrationOf,
  type ProductAppRegistry,
  type ProductProjectEntry,
  type ProductPlanContextEntry,
  type ProductStoreResult,
} from "./store.ts";
export type {
  ProductProject,
  ProductPlanContext,
  ProductProjectRegistrationRequest,
  ProductProviderProfile,
  ProductSetupPort,
  ProductSetupRequest,
  ProductSetupResponse,
  ProductSetupResult,
  ProductSetupSnapshot,
} from "../workspace-model/index.ts";
