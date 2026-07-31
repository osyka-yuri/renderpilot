import type { GameDetails } from '@entities/game';

import { RENDERPILOT_LIBRARY_PATH } from './library-path';

/** Details for a manually scanned install path in preview mode. */
export function createManualPreviewDetails(
  gameId: string,
  title: string,
  installPath: string,
): GameDetails {
  const dlssPath = `${installPath}/nvngx_dlss.dll`;

  return {
    game: {
      identity: {
        id: gameId,
        title,
        launcher: 'Manual',
        external_id: null,
      },
      platform: 'Windows',
      runtime: 'NativeWindows',
      install_path: installPath,
      can_remove_from_catalog: true,
    },
    components: [
      {
        id: `${gameId}:dlss`,
        game_id: gameId,
        kind: 'NativeLibrary',
        technology: 'dlss_super_resolution',
        swappability: 'Swappable',
        rollback_available: false,
        d3d12_executable_status: null,
        files: [
          {
            path: dlssPath,
            version: '3.5.10',
            sha256: 'preview-manual-dlss',
          },
        ],
      },
    ],
    candidate_groups: [
      {
        component_id: `${gameId}:dlss`,
        technology: 'dlss_super_resolution',
        file_path: dlssPath,
        version_report: {
          kind: 'known',
          technical_version: '3.5.10',
          release_label: null,
          catalog_release: null,
        },
        automatic_candidate_artifact_id: `artifact:${gameId}:dlss-preview`,
        candidates: [
          {
            artifact_id: `artifact:${gameId}:dlss-preview`,
            file_name: 'nvngx_dlss.dll',
            file_path: `${RENDERPILOT_LIBRARY_PATH}/nvngx_dlss_preview.dll`,
            technical_version: '3.7.20',
            release_label: null,
            source_game_id: null,
            comparison: 'newer_version',
            catalog_package: {
              package_id: 'nvidia-dlss-3.7.20',
              release: { version: '3.7.20', channel: 'stable', label: null },
              availability: 'available',
              automatic_selection_allowed: true,
              presentation: null,
            },
            is_downloaded: true,
            is_debug: false,
            sha256: 'mock-sha256-manual-dlss-preview',
            d3d12_executable_action: null,
          },
        ],
      },
    ],
    operations: [],
    addon_capabilities: [],
  };
}
