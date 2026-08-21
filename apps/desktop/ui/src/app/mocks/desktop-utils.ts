import type {
  GameCardsQuery,
  GameCardsResult,
  GamesCatalogBootstrap,
  GameDetails,
  CoverArtworkResult,
  AutoScanResponse,
  GameSummary,
  RemoveGameFromCatalogResult,
} from '@entities/game';
import type { EffectiveExecutable, ExecutableCandidate } from '@features/nvapi-settings';
import type { CatalogSettingPayload } from '@entities/settings';
import type {
  ApplySwapResult,
  RollbackComponentResult,
  RollbackPlan,
  SwapPlan,
} from '@entities/operation';
import type {
  LibraryPackageMutation,
  LibraryPackageState,
  LibraryPackagesOutput,
} from '@entities/library';
import type {
  AddGameInspection,
  AddGameResult,
  ManifestRefreshReport,
} from '@features/scan-libraries';
import { fileNameFromPath } from '@shared/path';
import { isRecord, isString, requireNonBlankString } from '@shared/validation';

type PayloadRecord = Record<PropertyKey, unknown>;

export type DesktopCommandPayloadMap = {
  portable_trial_ready: undefined;
  inspect_game_install: { path: string };
  add_game: {
    selectedRoot: string;
    rootChoice: 'selected' | 'recommended';
    allowRootCorrection: boolean;
    chosenExecutable: string | null;
    inspectionFingerprint: string;
  };
  scan_auto_libraries: undefined;
  refresh_remote_manifests: undefined;
  refresh_catalog_capabilities: undefined;
  query_game_cards: { query: GameCardsQuery };
  bootstrap_games_catalog: { limit: number };
  start_background_refresh: undefined;
  get_game_details: { gameId: string };
  get_game_file_safety_assessment: { gameId: string };
  get_shared_vulkan_safety_assessment: undefined;
  list_game_executable_candidates: { gameId: string };
  resolve_game_executable: { gameId: string };
  set_game_executable_override: { gameId: string; absolutePath: string };
  clear_game_executable_override: { gameId: string };
  fetch_game_cover: { gameId: string };
  clear_game_cover: { gameId: string };
  set_game_cover: { gameId: string; sourcePath: string };
  set_game_favorite: { gameId: string; isFavorite: boolean };
  set_game_hidden: { gameId: string; isHidden: boolean };
  remove_game_from_catalog: { gameId: string };
  get_catalog_setting: { key: string };
  set_catalog_setting: { key: string; value: string };
  plan_swap: { gameId: string; componentId: string; artifactId: string };
  apply_swap: {
    gameId: string;
    componentId: string;
    artifactId: string;
    confirmationToken?: string | null;
    gameContextToken?: string;
  };
  plan_rollback: { gameId: string; componentId: string };
  rollback_component: { gameId: string; componentId: string };
  list_library_packages: undefined;
  download_library_package: { packageId: string };
  download_artifact: { artifactId: string };
  delete_library_package: { packageId: string };
  // Luma — preview stubs (see commands/addon-tools.ts)
  luma_availability: { gameId: string };
  luma_check_update: { gameId: string; deep?: boolean };
  luma_install: { gameId: string; gameContextToken?: string };
  luma_uninstall: { gameId: string };
  luma_update: { gameId: string; forceFull?: boolean };
  // RenoDX — preview stubs
  renodx_availability: { gameId: string };
  renodx_check_update: { gameId: string };
  renodx_install: {
    gameId: string;
    reshadeChannel: string;
    gameContextToken?: string;
    sharedVulkanContextToken?: string | null;
  };
  renodx_install_from_file: {
    gameId: string;
    filePath: string;
    reshadeChannel: string;
    gameContextToken?: string;
    sharedVulkanContextToken?: string | null;
  };
  renodx_uninstall: { gameId: string };
  renodx_update: { gameId: string };
  renodx_switch_reshade_channel: { gameId: string; reshadeChannel: string };
  renodx_install_dlss_fix: { gameId: string };
  renodx_update_dlss_fix: { gameId: string };
  renodx_retry_dlss_fix_recovery: { gameId: string };
  renodx_uninstall_dlss_fix: { gameId: string };
  renodx_dlss_fix_availability: { gameId: string };
  renodx_vulkan_layer_status: undefined;
  renodx_vulkan_layer_management_status: undefined;
  renodx_apply_vulkan_layer: { reshadeChannel: string };
  renodx_remove_vulkan_layer: undefined;
};

export type DesktopCommandResultMap = {
  portable_trial_ready: undefined;
  inspect_game_install: AddGameInspection;
  add_game: AddGameResult;
  scan_auto_libraries: AutoScanResponse;
  refresh_remote_manifests: ManifestRefreshReport;
  refresh_catalog_capabilities: { refreshed: boolean };
  query_game_cards: GameCardsResult;
  bootstrap_games_catalog: GamesCatalogBootstrap;
  start_background_refresh: { started: boolean; partialFailureCount: number };
  get_game_details: GameDetails;
  get_game_file_safety_assessment: unknown;
  get_shared_vulkan_safety_assessment: unknown;
  list_game_executable_candidates: ExecutableCandidate[];
  resolve_game_executable: EffectiveExecutable | null;
  set_game_executable_override: undefined;
  clear_game_executable_override: undefined;
  fetch_game_cover: CoverArtworkResult;
  clear_game_cover: { cleared: boolean };
  set_game_cover: CoverArtworkResult;
  set_game_favorite: { saved: boolean };
  set_game_hidden: { saved: boolean };
  remove_game_from_catalog: RemoveGameFromCatalogResult;
  get_catalog_setting: CatalogSettingPayload;
  set_catalog_setting: { saved: boolean };
  plan_swap: SwapPlan;
  apply_swap: ApplySwapResult;
  plan_rollback: RollbackPlan;
  rollback_component: RollbackComponentResult;
  list_library_packages: LibraryPackagesOutput;
  download_library_package: LibraryPackageMutation;
  download_artifact: LibraryPackageState;
  delete_library_package: LibraryPackageMutation;
  // Wire DTOs for Luma/RenoDX live in feature slices; mock results stay untyped
  // so `app` does not import feature internals (FSD boundaries).
  luma_availability: unknown;
  luma_check_update: unknown;
  luma_install: unknown;
  luma_uninstall: unknown;
  luma_update: unknown;
  renodx_availability: unknown;
  renodx_check_update: unknown;
  renodx_install: unknown;
  renodx_install_from_file: unknown;
  renodx_uninstall: unknown;
  renodx_update: unknown;
  renodx_switch_reshade_channel: unknown;
  renodx_install_dlss_fix: unknown;
  renodx_update_dlss_fix: unknown;
  renodx_retry_dlss_fix_recovery: unknown;
  renodx_uninstall_dlss_fix: unknown;
  renodx_dlss_fix_availability:
    | { kind: 'recovery_pending'; actions: ['retry_recovery'] }
    | { kind: 'binding'; state: string; actions: string[] };
  renodx_vulkan_layer_status: unknown;
  renodx_vulkan_layer_management_status: unknown;
  renodx_apply_vulkan_layer: unknown;
  renodx_remove_vulkan_layer: unknown;
};

export type DesktopCommand = keyof DesktopCommandPayloadMap & keyof DesktopCommandResultMap;

const ALL_DESKTOP_COMMANDS = [
  'portable_trial_ready',
  'inspect_game_install',
  'add_game',
  'scan_auto_libraries',
  'refresh_remote_manifests',
  'refresh_catalog_capabilities',
  'query_game_cards',
  'bootstrap_games_catalog',
  'start_background_refresh',
  'get_game_details',
  'get_game_file_safety_assessment',
  'get_shared_vulkan_safety_assessment',
  'list_game_executable_candidates',
  'resolve_game_executable',
  'set_game_executable_override',
  'clear_game_executable_override',
  'fetch_game_cover',
  'clear_game_cover',
  'set_game_cover',
  'set_game_favorite',
  'set_game_hidden',
  'remove_game_from_catalog',
  'get_catalog_setting',
  'set_catalog_setting',
  'plan_swap',
  'apply_swap',
  'plan_rollback',
  'rollback_component',
  'list_library_packages',
  'download_library_package',
  'download_artifact',
  'delete_library_package',
  'luma_availability',
  'luma_check_update',
  'luma_install',
  'luma_uninstall',
  'luma_update',
  'renodx_availability',
  'renodx_check_update',
  'renodx_install',
  'renodx_install_from_file',
  'renodx_uninstall',
  'renodx_update',
  'renodx_switch_reshade_channel',
  'renodx_install_dlss_fix',
  'renodx_update_dlss_fix',
  'renodx_retry_dlss_fix_recovery',
  'renodx_uninstall_dlss_fix',
  'renodx_dlss_fix_availability',
  'renodx_vulkan_layer_status',
  'renodx_vulkan_layer_management_status',
  'renodx_apply_vulkan_layer',
  'renodx_remove_vulkan_layer',
] as const satisfies readonly DesktopCommand[];

const DESKTOP_COMMAND_SET = new Set<string>(ALL_DESKTOP_COMMANDS);

export function isDesktopCommand(command: string): command is DesktopCommand {
  return DESKTOP_COMMAND_SET.has(command);
}

export function readStringFields(
  command: DesktopCommand,
  payload: unknown,
  ...fields: string[]
): Record<string, string> {
  const record = readPayloadRecord(command, payload);
  const result: Record<string, string> = {};

  for (const field of fields) {
    result[field] = readStringFieldFromRecord(command, record, field);
  }

  return result;
}

export function readStringField(
  command: DesktopCommand,
  payload: unknown,
  field: string,
  options?: { allowEmpty?: boolean },
): string {
  return readStringFieldFromRecord(command, readPayloadRecord(command, payload), field, options);
}

export function readOptionalStringField(
  command: DesktopCommand,
  payload: unknown,
  field: string,
): string | null {
  const record = readPayloadRecord(command, payload);
  if (!Object.hasOwn(record, field) || record[field] === null) {
    return null;
  }
  return readStringFieldFromRecord(command, record, field);
}

export function readObjectField(
  command: DesktopCommand,
  payload: unknown,
  field: string,
): PayloadRecord {
  const record = readPayloadRecord(command, payload);
  const value = readRequiredField(command, record, field);

  if (!isRecord(value)) {
    throw new Error(`Mock invoker: Field "${field}" for "${command}" must be an object.`);
  }

  return value;
}

export function readPayloadRecord(command: DesktopCommand, payload: unknown): PayloadRecord {
  if (!isRecord(payload)) {
    throw new Error(`Mock invoker: Payload for "${command}" must be an object.`);
  }

  return payload;
}

function readStringFieldFromRecord(
  command: DesktopCommand,
  record: PayloadRecord,
  field: string,
  options?: { allowEmpty?: boolean },
): string {
  const value = readRequiredField(command, record, field);

  if (!isString(value)) {
    throw new Error(`Mock invoker: Field "${field}" for "${command}" must be a string.`);
  }

  if (!options?.allowEmpty && value.trim().length === 0) {
    throw new Error(`Mock invoker: Field "${field}" for "${command}" must not be empty.`);
  }

  return value;
}

function readRequiredField(command: DesktopCommand, record: PayloadRecord, field: string): unknown {
  if (!Object.hasOwn(record, field)) {
    throw new Error(`Mock invoker: Missing required field "${field}" in payload for "${command}".`);
  }

  return record[field];
}

/** Mock-facing blank check with preview-oriented error text; returns trimmed text. */
export function requireNonEmptyText(value: string, label: string): string {
  try {
    requireNonBlankString(value, label);
  } catch {
    throw new Error(`Mock preview ${label} is required.`);
  }
  return value.trim();
}

export function assertNever(value: never): never {
  throw new Error(`Mock invoker: Unhandled command "${String(value)}".`);
}

export function lastPathSegment(path: string): string {
  return fileNameFromPath(path);
}

export function normalizeInstallPath(path: string): string {
  const normalized = path.replaceAll('\\', '/').trim().replace(/\/+$/, '');

  if (!normalized) {
    throw new Error('Mock preview manual scan path is required.');
  }

  return normalized;
}

export function normalizeCoverSourcePath(sourcePath: string): string {
  const normalized = sourcePath.replaceAll('\\', '/').trim();

  if (!normalized) {
    throw new Error('Mock preview cover source path is required.');
  }

  return normalized;
}

export function createInstallPathKey(path: string): string {
  return normalizeInstallPath(path).toLowerCase();
}

export function unique<T>(items: readonly T[]): T[] {
  return [...new Set(items)];
}

export function collectAvailableLibraries(cards: readonly GameSummary[]): string[] {
  const values = new Set<string>();

  for (const card of cards) {
    for (const library of card.library_tags) {
      values.add(library);
    }
  }

  return Array.from(values).sort((left, right) => left.localeCompare(right));
}

export function collectAvailableLaunchers(cards: readonly GameSummary[]): string[] {
  const values = new Set<string>();

  for (const card of cards) {
    const trimmed = card.launcher.trim();

    if (trimmed.length > 0) {
      values.add(trimmed);
    }
  }

  return Array.from(values).sort((left, right) => left.localeCompare(right));
}

export function compareCards(
  left: GameSummary,
  right: GameSummary,
  sort: GameCardsQuery['sort'],
): number {
  const direction = sort.direction === 'asc' ? 1 : -1;
  const byTitle = compareCardsByTitle(left, right);

  if (sort.field === 'title') {
    return byTitle * direction;
  }

  if (sort.field === 'updates') {
    const updatesDiff = left.update_count - right.update_count;
    return updatesDiff === 0 ? byTitle : updatesDiff * direction;
  }

  const riskDiff = getRiskSortValue(left.risk_level) - getRiskSortValue(right.risk_level);
  return riskDiff === 0 ? byTitle : riskDiff * direction;
}

function compareCardsByTitle(left: GameSummary, right: GameSummary): number {
  return left.title.localeCompare(right.title) || left.game_id.localeCompare(right.game_id);
}

const RISK_SORT_ORDER: Record<GameSummary['risk_level'], number> = {
  safe: 0,
  low: 1,
  medium: 2,
  high: 3,
  blocked: 4,
  unknown: 5,
};

function getRiskSortValue(riskLevel: GameSummary['risk_level']): number {
  return RISK_SORT_ORDER[riskLevel];
}

export function resolveMock<T>(factory: () => T): Promise<T> {
  try {
    return Promise.resolve(factory());
  } catch (error) {
    return Promise.reject(toError(error));
  }
}

function toError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }

  if (isString(error)) {
    return new Error(error);
  }

  return new Error('Mock preview command failed.');
}

export function clone<T>(value: T): T {
  if (value === undefined) {
    return value;
  }

  const serialized = JSON.stringify(value);

  if (!isString(serialized)) {
    throw new Error('Mock preview could not clone a non-serializable value.');
  }

  const parsed: unknown = JSON.parse(serialized);
  return parsed as T;
}
