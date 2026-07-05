import type { GameCardsQuery } from '@entities/game';
import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';
import { mockScanManualFolder, mockScanAutoLibraries } from './commands/scan';
import { mockQueryGameCards, mockGetGameDetails } from './commands/query';
import { mockFetchGameCover, mockClearGameCover, mockSetGameCover } from './commands/cover';
import { mockGetCatalogSetting, mockSetCatalogSetting } from './commands/settings';
import { mockApplySwap, mockRollbackComponent } from './commands/operations';
import { mockSetGameFavorite, mockSetGameHidden } from './commands/game-ui-state';
import { mockState, createMockState } from './desktop-state';
import {
  assertNever,
  isDesktopCommand,
  readObjectField,
  readPayloadRecord,
  readStringField,
  type DesktopCommand,
} from './desktop-utils';

const mockInvokerImpl = async (command: DesktopCommand, payload: unknown): Promise<unknown> =>
  dispatchCommand(command, payload);

export const mockInvoker = mockInvokerImpl as DesktopInvoker<DesktopCommand>;

async function dispatchCommand(command: DesktopCommand, payload: unknown): Promise<unknown> {
  switch (command) {
    case 'scan_manual_folder':
      return mockScanManualFolder(readStringField(command, payload, 'path'));

    case 'scan_auto_libraries':
      return mockScanAutoLibraries();

    case 'query_game_cards': {
      const query = readObjectField(command, payload, 'query') as unknown as GameCardsQuery;
      return mockQueryGameCards(query);
    }

    case 'get_game_details':
      return mockGetGameDetails(readStringField(command, payload, 'gameId'));

    case 'fetch_game_cover':
      return mockFetchGameCover(readStringField(command, payload, 'gameId'));

    case 'clear_game_cover':
      return mockClearGameCover(readStringField(command, payload, 'gameId'));

    case 'set_game_cover':
      return mockSetGameCover(
        readStringField(command, payload, 'gameId'),
        readStringField(command, payload, 'sourcePath'),
      );

    case 'set_game_favorite':
      return mockSetGameFavorite(
        readStringField(command, payload, 'gameId'),
        readBooleanField(command, payload, 'isFavorite'),
      );

    case 'set_game_hidden':
      return mockSetGameHidden(
        readStringField(command, payload, 'gameId'),
        readBooleanField(command, payload, 'isHidden'),
      );

    case 'get_catalog_setting':
      return mockGetCatalogSetting(readStringField(command, payload, 'key'));

    case 'set_catalog_setting':
      return mockSetCatalogSetting(
        readStringField(command, payload, 'key'),
        readStringField(command, payload, 'value', { allowEmpty: true }),
      );

    case 'apply_swap':
      return mockApplySwap(
        readStringField(command, payload, 'gameId'),
        readStringField(command, payload, 'componentId'),
        readStringField(command, payload, 'artifactId'),
      );

    case 'rollback_component':
      return mockRollbackComponent(
        readStringField(command, payload, 'gameId'),
        readStringField(command, payload, 'componentId'),
      );

    default:
      return assertNever(command);
  }
}

function readBooleanField(command: DesktopCommand, payload: unknown, field: string): boolean {
  const record = readPayloadRecord(command, payload);
  if (!Object.prototype.hasOwnProperty.call(record, field)) {
    throw new Error(`Mock invoker: Missing required field "${field}" in payload for "${command}".`);
  }
  const value = record[field];
  if (typeof value !== 'boolean') {
    throw new Error(`Mock invoker: Field "${field}" for "${command}" must be a boolean.`);
  }
  return value;
}

async function previewInvoker(command: string, payload: unknown): Promise<unknown> {
  if (!isDesktopCommand(command)) {
    throw new Error(`Mock invoker: Unknown command "${command}".`);
  }

  return mockInvokerImpl(command, payload);
}

export function registerMockInvoker(): void {
  registerPreviewInvoker(previewInvoker as DesktopInvoker);
}

export function resetMockDesktopState(): void {
  Object.assign(mockState, createMockState());
}

export {
  mockScanManualFolder,
  mockScanAutoLibraries,
  mockQueryGameCards,
  mockGetGameDetails,
  mockFetchGameCover,
  mockClearGameCover,
  mockSetGameCover,
  mockSetGameFavorite,
  mockSetGameHidden,
  mockGetCatalogSetting,
  mockSetCatalogSetting,
  mockApplySwap,
  mockRollbackComponent,
};
