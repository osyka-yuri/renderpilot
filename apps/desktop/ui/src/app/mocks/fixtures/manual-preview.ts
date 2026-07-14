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
      executable_candidates: [],
    },
    components: [
      {
        id: `${gameId}:dlss`,
        game_id: gameId,
        kind: 'NativeLibrary',
        technology: 'dlss_super_resolution',
        swappability: 'Swappable',
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
        version_report: { kind: 'known', version: '3.5.10' },
        candidates: [
          {
            artifact_id: `artifact:${gameId}:dlss-preview`,
            file_name: 'nvngx_dlss.dll',
            file_path: `${RENDERPILOT_LIBRARY_PATH}/nvngx_dlss_preview.dll`,
            version: '3.7.20',
            source_game_id: null,
            comparison: 'newer_version',
            is_downloaded: true,
            is_debug: false,
            sha256: 'mock-sha256-manual-dlss-preview',
          },
        ],
      },
    ],
    operations: [],
    addon_capabilities: [],
  };
}
