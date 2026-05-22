import type { GameCardsQuery } from '@entities/game';
import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';
import { mockScanManualFolder, mockScanAutoLibraries } from './commands/scan';
import { mockQueryGameCards, mockGetGameDetails } from './commands/query';
import { mockFetchGameCover, mockClearGameCover, mockSetGameCover } from './commands/cover';
import { mockGetCatalogSetting, mockSetCatalogSetting } from './commands/settings';
import { mockApplySwap, mockRollbackComponent } from './commands/operations';
import { mockState, createMockState } from './desktop-state';
import { isDesktopCommand, type DesktopCommand } from './desktop-utils';

const mockInvokerImpl = async (command: DesktopCommand, payload: unknown): Promise<unknown> =>
  dispatchCommand(command, payload);

export const mockInvoker = mockInvokerImpl as DesktopInvoker<DesktopCommand>;

async function dispatchCommand(command: DesktopCommand, payload: unknown): Promise<unknown> {
  switch (command) {
    case 'scan_manual_folder': {
      const { path } = readScanManualFolderPayload(payload);
      return mockScanManualFolder(path);
    }

    case 'scan_auto_libraries':
      return mockScanAutoLibraries();

    case 'query_game_cards': {
      const { query } = readQueryGameCardsPayload(payload);
      return mockQueryGameCards(query);
    }

    case 'get_game_details': {
      const { gameId } = readGetGameDetailsPayload(payload);
      return mockGetGameDetails(gameId);
    }

    case 'fetch_game_cover': {
      const { gameId } = readFetchGameCoverPayload(payload);
      return mockFetchGameCover(gameId);
    }

    case 'clear_game_cover': {
      const { gameId } = readClearGameCoverPayload(payload);
      return mockClearGameCover(gameId);
    }

    case 'set_game_cover': {
      const { gameId, sourcePath } = readSetGameCoverPayload(payload);
      return mockSetGameCover(gameId, sourcePath);
    }

    case 'get_catalog_setting': {
      const { key } = readGetCatalogSettingPayload(payload);
      return mockGetCatalogSetting(key);
    }

    case 'set_catalog_setting': {
      const { key, value } = readSetCatalogSettingPayload(payload);
      return mockSetCatalogSetting(key, value);
    }

    case 'apply_swap': {
      const { gameId, componentId, artifactId } = readApplySwapPayload(payload);
      return mockApplySwap(gameId, componentId, artifactId);
    }

    case 'rollback_component': {
      const { gameId, componentId } = readRollbackComponentPayload(payload);
      return mockRollbackComponent(gameId, componentId);
    }

    default:
      return assertNever(command);
  }
}

function readScanManualFolderPayload(payload: unknown): { path: string } {
  return payload as { path: string };
}

function readQueryGameCardsPayload(payload: unknown): { query: GameCardsQuery } {
  return payload as { query: GameCardsQuery };
}

function readGetGameDetailsPayload(payload: unknown): { gameId: string } {
  return payload as { gameId: string };
}

function readFetchGameCoverPayload(payload: unknown): { gameId: string } {
  return payload as { gameId: string };
}

function readClearGameCoverPayload(payload: unknown): { gameId: string } {
  return payload as { gameId: string };
}

function readSetGameCoverPayload(payload: unknown): { gameId: string; sourcePath: string } {
  return payload as { gameId: string; sourcePath: string };
}

function readGetCatalogSettingPayload(payload: unknown): { key: string } {
  return payload as { key: string };
}

function readSetCatalogSettingPayload(payload: unknown): { key: string; value: string } {
  return payload as { key: string; value: string };
}

function readApplySwapPayload(payload: unknown): {
  gameId: string;
  componentId: string;
  artifactId: string;
} {
  return payload as { gameId: string; componentId: string; artifactId: string };
}

function readRollbackComponentPayload(payload: unknown): {
  gameId: string;
  componentId: string;
} {
  return payload as { gameId: string; componentId: string };
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

function assertNever(value: never): never {
  throw new Error(`Mock invoker: Unhandled command "${String(value)}".`);
}

export {
  mockScanManualFolder,
  mockScanAutoLibraries,
  mockQueryGameCards,
  mockGetGameDetails,
  mockFetchGameCover,
  mockClearGameCover,
  mockSetGameCover,
  mockGetCatalogSetting,
  mockSetCatalogSetting,
  mockApplySwap,
  mockRollbackComponent,
};
