export { scanAutoLibrariesWithErrorRecovery } from './model/catalog-refresh';
export {
  publishAutomaticLibraryScanFailedNotification,
  publishAddGameWarnings,
  publishPartialLibraryScanWarning,
} from './model/notifications';

export { selectGameInstallFolder } from './model/scan-dialog';

export {
  addGame,
  inspectGameInstall,
  refreshRemoteManifests,
  refreshCatalogCapabilities,
} from './api/desktop';
export type {
  AddGameInspection,
  AddGameRequest,
  AddGameResult,
  AddGameConfirmation,
  AddGameCatalogAction,
  AddGameDecision,
  AddGameOption,
  AddGameRootChoice,
  AddGameUnavailableReason,
} from './model/add-game';
export { automaticAddGameConfirmation, decisionOptions } from './model/add-game';
export { default as AddGameDialog } from './ui/AddGameDialog.svelte';
export { createAddGameFlow } from './model/add-game-flow.svelte';
export type {
  AddGameDialogState,
  AddGameFlow,
  AddGameFlowDeps,
  AddGameFlowState,
} from './model/add-game-flow.svelte';
export type {
  ManifestKindStatus,
  ManifestRefreshOutcome,
  ManifestRefreshReport,
} from './model/manifest-refresh';
