import { invoke } from '@tauri-apps/api/core';

import { normalizeCommandError } from './errors';
import { normalizeGameCardsQuery } from './game-cards-query';
import {
  isDesktopPreviewMode as shouldUseDesktopPreviewMode,
  mockApplyOperationPlan,
  mockBuildSwapPlan,
  mockClearGameCover,
  mockFetchGameCover,
  mockGetCatalogSetting,
  mockQueryGameCards,
  mockGetGameDetails,
  mockRollbackOperation,
  mockScanAutoLibraries,
  mockScanManualFolder,
  mockSetCatalogSetting,
  mockSetGameCover,
} from './mock-desktop';
import type {
  ApplyOperationResult,
  AutoScanResponse,
  CatalogSettingPayload,
  CoverArtworkResult,
  GameCardsQuery,
  GameCardsResult,
  GameDetails,
  RollbackOperationResult,
  SwapPlan,
} from './types';

export {
  COVERS_GOG_CDN_SETTING_KEY,
  COVERS_STEAM_CDN_SETTING_KEY,
  COVERS_STEAMGRIDDB_REMOTE_SETTING_KEY,
  STEAMGRIDDB_SETTING_KEY,
} from '@shared/catalog/catalog-setting-keys';

export { isDesktopPreviewMode } from './mock-desktop';

export type ScanManualFolderResult = {
  games: GameDetails[];
};

/** Windows WebView2 resolves registered scheme `rp-cover` under this origin. */
export const GAME_COVER_ORIGIN = 'http://rp-cover.localhost' as const;

type DesktopCommandContract = {
  scan_manual_folder: {
    payload: { path: string };
    result: ScanManualFolderResult;
  };
  scan_auto_libraries: {
    payload: undefined;
    result: AutoScanResponse;
  };
  query_game_cards: {
    payload: { query: GameCardsQuery };
    result: GameCardsResult;
  };
  get_game_details: {
    payload: { gameId: string };
    result: GameDetails;
  };
  fetch_game_cover: {
    payload: { gameId: string };
    result: CoverArtworkResult;
  };
  clear_game_cover: {
    payload: { gameId: string };
    result: { cleared: boolean };
  };
  set_game_cover: {
    payload: { gameId: string; sourcePath: string };
    result: CoverArtworkResult;
  };
  get_catalog_setting: {
    payload: { key: string };
    result: CatalogSettingPayload;
  };
  set_catalog_setting: {
    payload: { key: string; value: string };
    result: { saved: boolean };
  };
  build_swap_plan: {
    payload: { gameId: string; componentId: string; artifactId: string };
    result: SwapPlan;
  };
  apply_operation_plan: {
    payload: { operationId: string; confirmationToken: string };
    result: ApplyOperationResult;
  };
  rollback_operation: {
    payload: { operationId: string };
    result: RollbackOperationResult;
  };
};

type DesktopCommand = keyof DesktopCommandContract;

type CommandPayload<Command extends DesktopCommand> = DesktopCommandContract[Command]['payload'];

type CommandResult<Command extends DesktopCommand> = DesktopCommandContract[Command]['result'];

type CommandArgs<Command extends DesktopCommand> =
  CommandPayload<Command> extends undefined ? [] : [payload: CommandPayload<Command>];

type PreviewHandlers = {
  readonly [Command in DesktopCommand]: (
    payload: CommandPayload<Command>,
  ) => Promise<CommandResult<Command>>;
};

const DESKTOP_COMMAND = {
  scanManualFolder: 'scan_manual_folder',
  scanAutoLibraries: 'scan_auto_libraries',
  queryGameCards: 'query_game_cards',
  getGameDetails: 'get_game_details',
  fetchGameCover: 'fetch_game_cover',
  clearGameCover: 'clear_game_cover',
  setGameCover: 'set_game_cover',
  getCatalogSetting: 'get_catalog_setting',
  setCatalogSetting: 'set_catalog_setting',
  buildSwapPlan: 'build_swap_plan',
  applyOperationPlan: 'apply_operation_plan',
  rollbackOperation: 'rollback_operation',
} as const satisfies Record<string, DesktopCommand>;

const previewHandlers: PreviewHandlers = {
  scan_manual_folder: ({ path }) => mockScanManualFolder(path),

  scan_auto_libraries: () => mockScanAutoLibraries(),

  query_game_cards: ({ query }) => mockQueryGameCards(query),

  get_game_details: ({ gameId }) => mockGetGameDetails(gameId),

  fetch_game_cover: ({ gameId }) => mockFetchGameCover(gameId),

  clear_game_cover: ({ gameId }) => mockClearGameCover(gameId),

  set_game_cover: ({ gameId, sourcePath }) => mockSetGameCover(gameId, sourcePath),

  get_catalog_setting: ({ key }) => mockGetCatalogSetting(key),

  set_catalog_setting: ({ key, value }) => mockSetCatalogSetting(key, value),

  build_swap_plan: ({ gameId, componentId, artifactId }) =>
    mockBuildSwapPlan(gameId, componentId, artifactId),

  apply_operation_plan: ({ operationId, confirmationToken }) =>
    mockApplyOperationPlan(operationId, confirmationToken),

  rollback_operation: ({ operationId }) => mockRollbackOperation(operationId),
};

function requireString(value: string, fieldName: string): string {
  if (typeof value !== 'string') {
    throw new TypeError(`${fieldName} must be a string.`);
  }

  return value;
}

function requireNonBlankString(value: string, fieldName: string): string {
  requireString(value, fieldName);

  if (value.trim().length === 0) {
    throw new TypeError(`${fieldName} must be a non-empty string.`);
  }

  return value;
}

function requireValidTimestampMs(value: number, fieldName: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new TypeError(`${fieldName} must be a finite non-negative number.`);
  }

  return value;
}

export function gameCoverAssetSrc(gameId: string): string {
  const safeGameId = requireNonBlankString(gameId, 'gameId');

  return `${GAME_COVER_ORIGIN}/${encodeURIComponent(safeGameId)}`;
}

/** Cache-busting query for WebView after cover bytes change at the same protocol URL. */
export function gameCoverAssetSrcWithVersion(gameId: string, updatedAtMs: number): string {
  const base = gameCoverAssetSrc(gameId);
  const safeUpdatedAtMs = requireValidTimestampMs(updatedAtMs, 'updatedAtMs');

  return `${base}?v=${encodeURIComponent(String(safeUpdatedAtMs))}`;
}

function invokeDesktopPreview<Command extends DesktopCommand>(
  command: Command,
  payload: CommandPayload<Command>,
): Promise<CommandResult<Command>> {
  return previewHandlers[command](payload);
}

function invokeTauriCommand<Command extends DesktopCommand>(
  command: Command,
  payload: CommandPayload<Command>,
): Promise<CommandResult<Command>> {
  if (payload === undefined) {
    return invoke<CommandResult<Command>>(command);
  }

  return invoke<CommandResult<Command>>(command, payload);
}

async function invokeDesktop<Command extends DesktopCommand>(
  command: Command,
  ...args: CommandArgs<Command>
): Promise<CommandResult<Command>> {
  const payload = args[0];

  try {
    if (shouldUseDesktopPreviewMode()) {
      return await invokeDesktopPreview(command, payload);
    }

    return await invokeTauriCommand(command, payload);
  } catch (error) {
    throw normalizeCommandError(error);
  }
}

export async function scanManualFolder(path: string): Promise<ScanManualFolderResult> {
  return invokeDesktop(DESKTOP_COMMAND.scanManualFolder, {
    path: requireNonBlankString(path, 'path'),
  });
}

export async function scanAutoLibraries(): Promise<AutoScanResponse> {
  return invokeDesktop(DESKTOP_COMMAND.scanAutoLibraries);
}

export async function queryGameCards(query: GameCardsQuery): Promise<GameCardsResult> {
  return invokeDesktop(DESKTOP_COMMAND.queryGameCards, { query: normalizeGameCardsQuery(query) });
}

export async function getGameDetails(gameId: string): Promise<GameDetails> {
  return invokeDesktop(DESKTOP_COMMAND.getGameDetails, {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function fetchGameCover(gameId: string): Promise<CoverArtworkResult> {
  return invokeDesktop(DESKTOP_COMMAND.fetchGameCover, {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function clearGameCover(gameId: string): Promise<{ cleared: boolean }> {
  return invokeDesktop(DESKTOP_COMMAND.clearGameCover, {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function setGameCover(
  gameId: string,
  sourcePath: string,
): Promise<CoverArtworkResult> {
  return invokeDesktop(DESKTOP_COMMAND.setGameCover, {
    gameId: requireNonBlankString(gameId, 'gameId'),
    sourcePath: requireNonBlankString(sourcePath, 'sourcePath'),
  });
}

export async function getCatalogSetting(key: string): Promise<CatalogSettingPayload> {
  return invokeDesktop(DESKTOP_COMMAND.getCatalogSetting, {
    key: requireNonBlankString(key, 'key'),
  });
}

export async function setCatalogSetting(key: string, value: string): Promise<{ saved: boolean }> {
  return invokeDesktop(DESKTOP_COMMAND.setCatalogSetting, {
    key: requireNonBlankString(key, 'key'),
    // Empty/blank values intentionally clear the persisted setting on backend.
    value: requireString(value, 'value'),
  });
}

export async function buildSwapPlan(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<SwapPlan> {
  return invokeDesktop(DESKTOP_COMMAND.buildSwapPlan, {
    gameId: requireNonBlankString(gameId, 'gameId'),
    componentId: requireNonBlankString(componentId, 'componentId'),
    artifactId: requireNonBlankString(artifactId, 'artifactId'),
  });
}

export async function applyOperationPlan(
  operationId: string,
  confirmationToken: string,
): Promise<ApplyOperationResult> {
  return invokeDesktop(DESKTOP_COMMAND.applyOperationPlan, {
    operationId: requireNonBlankString(operationId, 'operationId'),
    confirmationToken: requireNonBlankString(confirmationToken, 'confirmationToken'),
  });
}

export async function rollbackOperation(operationId: string): Promise<RollbackOperationResult> {
  return invokeDesktop(DESKTOP_COMMAND.rollbackOperation, {
    operationId: requireNonBlankString(operationId, 'operationId'),
  });
}
