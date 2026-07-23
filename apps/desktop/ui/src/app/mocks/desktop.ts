import type { GameCardsQuery } from '@entities/game';
import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';
import { mockScanManualFolder, mockScanAutoLibraries } from './commands/scan';
import { mockQueryGameCards, mockGetGameDetails } from './commands/query';
import { mockFetchGameCover, mockClearGameCover, mockSetGameCover } from './commands/cover';
import { mockGetCatalogSetting, mockSetCatalogSetting } from './commands/settings';
import { mockApplySwap, mockRollbackComponent } from './commands/operations';
import {
  mockDeleteLibraryPackage,
  mockDownloadArtifact,
  mockDownloadLibraryPackage,
  mockListLibraryPackages,
} from './commands/libraries';
import { mockSetGameFavorite, mockSetGameHidden } from './commands/game-ui-state';
import { mockState, createMockState } from './desktop-state';
import {
  mockAddonWriteUnsupported,
  mockLumaUpdateReport,
  mockRenoDxUpdateReport,
  mockUnsupportedLumaAvailability,
  mockUnsupportedRenoDxAvailability,
  mockVulkanLayerManagementStatus,
  mockVulkanLayerStatus,
} from './commands/addon-tools';
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

    case 'refresh_remote_manifests':
      return {
        outcome: { kind: 'forced_fetched' },
        kinds: {
          libraries: { status: 'ok' },
          renodx: { status: 'ok' },
          luma: { status: 'ok' },
          reshade: { status: 'ok' },
        },
      };

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

    case 'list_library_packages':
      return mockListLibraryPackages();

    case 'download_library_package':
      return mockDownloadLibraryPackage(readStringField(command, payload, 'packageId'));

    case 'download_artifact':
      return mockDownloadArtifact(readStringField(command, payload, 'artifactId'));

    case 'delete_library_package':
      return mockDeleteLibraryPackage(readStringField(command, payload, 'packageId'));

    case 'luma_availability':
      readStringField(command, payload, 'gameId');
      return mockUnsupportedLumaAvailability();

    case 'luma_check_update':
      readStringField(command, payload, 'gameId');
      return mockLumaUpdateReport();

    case 'luma_install':
    case 'luma_uninstall':
    case 'luma_update':
      return mockAddonWriteUnsupported();

    case 'renodx_availability':
      readStringField(command, payload, 'gameId');
      return mockUnsupportedRenoDxAvailability();

    case 'renodx_check_update':
      readStringField(command, payload, 'gameId');
      return mockRenoDxUpdateReport();

    case 'renodx_dlss_fix_availability':
      readStringField(command, payload, 'gameId');
      return false;

    case 'renodx_vulkan_layer_status':
      return mockVulkanLayerStatus();

    case 'renodx_vulkan_layer_management_status':
      return mockVulkanLayerManagementStatus();

    case 'renodx_install':
    case 'renodx_install_from_file':
    case 'renodx_uninstall':
    case 'renodx_update':
    case 'renodx_switch_reshade_channel':
    case 'renodx_install_dlss_fix':
    case 'renodx_uninstall_dlss_fix':
    case 'renodx_apply_vulkan_layer':
    case 'renodx_remove_vulkan_layer':
      return mockAddonWriteUnsupported();

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
  mockListLibraryPackages,
  mockDownloadLibraryPackage,
  mockDownloadArtifact,
  mockDeleteLibraryPackage,
};
