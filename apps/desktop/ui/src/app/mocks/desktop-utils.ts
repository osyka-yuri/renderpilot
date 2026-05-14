import type {
  GameCardsQuery,
  GameCardsResult,
  GameDetails,
  CoverArtworkResult,
  ScanManualFolderResult,
  AutoScanResponse,
  GameSummary,
} from '@entities/game';
import type { CatalogSettingPayload } from '@entities/settings';
import type { SwapPlan, ApplyOperationResult, RollbackOperationResult } from '@entities/operation';
import { isRecord, isString } from '@shared/validation';

type PayloadRecord = Record<PropertyKey, unknown>;

export type DesktopCommandPayloadMap = {
  scan_manual_folder: { path: string };
  scan_auto_libraries: undefined;
  query_game_cards: { query: GameCardsQuery };
  get_game_details: { gameId: string };
  fetch_game_cover: { gameId: string };
  clear_game_cover: { gameId: string };
  set_game_cover: { gameId: string; sourcePath: string };
  get_catalog_setting: { key: string };
  set_catalog_setting: { key: string; value: string };
  build_swap_plan: { gameId: string; componentId: string; artifactId: string };
  apply_operation_plan: { operationId: string; confirmationToken: string };
  rollback_operation: { operationId: string };
};

export type DesktopCommandResultMap = {
  scan_manual_folder: ScanManualFolderResult;
  scan_auto_libraries: AutoScanResponse;
  query_game_cards: GameCardsResult;
  get_game_details: GameDetails;
  fetch_game_cover: CoverArtworkResult;
  clear_game_cover: { cleared: boolean };
  set_game_cover: CoverArtworkResult;
  get_catalog_setting: CatalogSettingPayload;
  set_catalog_setting: { saved: boolean };
  build_swap_plan: SwapPlan;
  apply_operation_plan: ApplyOperationResult;
  rollback_operation: RollbackOperationResult;
};

export type DesktopCommand = keyof DesktopCommandPayloadMap & keyof DesktopCommandResultMap;

const ALL_DESKTOP_COMMANDS = [
  'scan_manual_folder',
  'scan_auto_libraries',
  'query_game_cards',
  'get_game_details',
  'fetch_game_cover',
  'clear_game_cover',
  'set_game_cover',
  'get_catalog_setting',
  'set_catalog_setting',
  'build_swap_plan',
  'apply_operation_plan',
  'rollback_operation',
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
  if (!Object.prototype.hasOwnProperty.call(record, field)) {
    throw new Error(`Mock invoker: Missing required field "${field}" in payload for "${command}".`);
  }

  return record[field];
}

export function requireNonEmptyText(value: string, label: string): string {
  const normalized = value.trim();

  if (!normalized) {
    throw new Error(`Mock preview ${label} is required.`);
  }

  return normalized;
}

export function assertNever(value: never): never {
  throw new Error(`Mock invoker: Unhandled command "${String(value)}".`);
}

export function lastPathSegment(path: string): string {
  const segments = normalizeWindowsSlashes(path).split('/').filter(Boolean);

  if (segments.length === 0) {
    return '';
  }

  return segments[segments.length - 1];
}

export function normalizeInstallPath(path: string): string {
  const normalized = normalizeWindowsSlashes(path.trim()).replace(/\/+$/, '');

  if (!normalized) {
    throw new Error('Mock preview manual scan path is required.');
  }

  return normalized;
}

export function normalizeCoverSourcePath(sourcePath: string): string {
  const normalized = normalizeWindowsSlashes(sourcePath.trim());

  if (!normalized) {
    throw new Error('Mock preview cover source path is required.');
  }

  return normalized;
}

function normalizeWindowsSlashes(path: string): string {
  return path.replace(/\\/g, '/');
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

  return [...values].sort((left, right) => left.localeCompare(right));
}

export function collectAvailableLaunchers(cards: readonly GameSummary[]): string[] {
  const values = new Set<string>();

  for (const card of cards) {
    const trimmed = card.launcher.trim();

    if (trimmed.length > 0) {
      values.add(trimmed);
    }
  }

  return [...values].sort((left, right) => left.localeCompare(right));
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

function getRiskSortValue(riskLevel: GameSummary['risk_level']): number {
  switch (riskLevel) {
    case 'low':
      return 0;

    case 'medium':
      return 1;

    case 'high':
      return 2;

    default:
      return 3;
  }
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
