import type { GameDetails, GameGraphicsComponent } from '@entities/game';

import { RENDERPILOT_LIBRARY_PATH } from './library-path';

/** Seed fixture: Alan Wake 2 with Streamline bundle + read-only DLSS-G. */
export function createAlanWakeDetails(): GameDetails {
  const components: GameGraphicsComponent[] = [
    {
      id: 'component:aw2:streamline',
      game_id: 'epic:alanwake2',
      kind: 'StreamlineComponent',
      technology: 'nvidia_streamline',
      swappability: 'BundleOnly',
      rollback_available: false,
      d3d12_executable_status: null,
      files: [
        {
          path: 'D:/Epic Games/Alan Wake 2/sl.common.dll',
          version: '2.4.0',
          sha256: '50ec2acc82864a0bdb834e1b7b5fa4d95af31026ec5f7862d443cb358638efde',
        },
        {
          path: 'D:/Epic Games/Alan Wake 2/sl.interposer.dll',
          version: '2.4.0',
          sha256: '0d5e790027df75d5105560075d10cce8b506c13337961237fe06b4a44f2ab341',
        },
      ],
    },
    {
      id: 'component:aw2:dlssg',
      game_id: 'epic:alanwake2',
      kind: 'NativeLibrary',
      technology: 'dlss_frame_generation',
      swappability: 'ReadOnly',
      rollback_available: false,
      d3d12_executable_status: null,
      files: [
        {
          path: 'D:/Epic Games/Alan Wake 2/nvngx_dlssg.dll',
          version: '3.1.0',
          sha256: '2755ccd61f4af89f66c89017f9ab8bd6c1f1fbe58e550cef48fe6e4a1c727a2d',
        },
      ],
    },
  ];

  return {
    game: {
      identity: {
        id: 'epic:alanwake2',
        title: 'Alan Wake 2',
        launcher: 'Epic',
        external_id: null,
      },
      platform: 'Windows',
      runtime: 'NativeWindows',
      install_path: 'D:/Epic Games/Alan Wake 2',
      can_remove_from_catalog: false,
    },
    components,
    candidate_groups: [
      {
        component_id: 'component:aw2:streamline',
        technology: 'nvidia_streamline',
        file_path: 'D:/Epic Games/Alan Wake 2/sl.common.dll',
        version_report: {
          kind: 'known',
          technical_version: '2.4.0',
          release_label: null,
          catalog_release: null,
        },
        candidates: [
          // Multi-file package candidate (matched Streamline release).
          {
            artifact_id: 'artifact:streamline:2.5.1',
            file_name: 'sl.common.dll',
            file_path: `${RENDERPILOT_LIBRARY_PATH}/sl.common_2.5.1.dll`,
            technical_version: '2.5.1',
            release_label: null,
            source_game_id: null,
            comparison: 'newer_version',
            catalog_package: {
              package_id: 'nvidia-streamline-2.5.1',
              release: { version: '2.5.1', channel: 'stable', label: null },
              availability: 'available',
              automatic_selection_allowed: true,
            },
            is_downloaded: true,
            is_debug: false,
            sha256: 'mock-sha256-streamline-251-package',
            d3d12_executable_action: null,
          },
        ],
      },
    ],
    operations: [],
    addon_capabilities: [],
  };
}
