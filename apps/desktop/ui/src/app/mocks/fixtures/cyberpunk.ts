import type { GameCandidateGroup, GameDetails, GameGraphicsComponent } from '@entities/game';

import { RENDERPILOT_LIBRARY_PATH } from './library-path';

/** Seed fixture: Cyberpunk 2077 with DLSS SR/G/R plus a managed D3D12 candidate. */
export function createCyberpunkDetails(): GameDetails {
  const components: GameGraphicsComponent[] = [
    {
      id: 'component:cp2077:dlss',
      game_id: 'steam:1091500',
      kind: 'NativeLibrary',
      technology: 'dlss_super_resolution',
      swappability: 'Swappable',
      rollback_available: false,
      d3d12_executable_status: null,
      files: [
        {
          path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlss.dll',
          version: '3.5.10',
          sha256: '2fca0a355ceefc1ce2be77f2406f9d3af7e3f939ff4ef53e2f8ac3f4519c4fab',
        },
      ],
    },
    {
      id: 'component:cp2077:dlssg',
      game_id: 'steam:1091500',
      kind: 'NativeLibrary',
      technology: 'dlss_frame_generation',
      swappability: 'Swappable',
      rollback_available: false,
      d3d12_executable_status: null,
      files: [
        {
          path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlssg.dll',
          version: '3.5.0',
          sha256: '715ff57263a275c06af04a8e6e6fbc4e3a306af2987b41569460e85807ab9125',
        },
      ],
    },
    {
      id: 'component:cp2077:dlssd',
      game_id: 'steam:1091500',
      kind: 'NativeLibrary',
      technology: 'dlss_ray_reconstruction',
      swappability: 'Swappable',
      rollback_available: false,
      d3d12_executable_status: null,
      files: [
        {
          path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlssd.dll',
          version: '3.5.0',
          sha256: '87caea2055c54a4a4eab8408c0f59ef7554cfa663093735dd57637b510b7a0b5',
        },
      ],
    },
    {
      id: 'component:cp2077:d3d12',
      game_id: 'steam:1091500',
      kind: 'NativeLibrary',
      technology: 'd3d12_agility',
      swappability: 'Swappable',
      rollback_available: false,
      d3d12_executable_status: {
        status: 'original',
        selection_locked: false,
        executable_path: 'C:/Games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe',
        backup_path: 'C:/Games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe.bak',
        backup_exists: false,
        original_sdk_version: 606,
        current_sdk_version: 606,
      },
      files: [
        {
          path: 'C:/Games/Cyberpunk 2077/bin/x64/D3D12Core.dll',
          version: '1.606.4',
          sha256: '58c16d5e2f12f0723dc948ca6dc70cae681eae54fd5c35d3f25a2e4f83dcdbd4',
        },
      ],
    },
  ];

  const candidateGroups: GameCandidateGroup[] = [
    {
      component_id: 'component:cp2077:dlss',
      technology: 'dlss_super_resolution',
      file_path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlss.dll',
      version_report: {
        kind: 'known',
        technical_version: '3.5.10',
        release_label: null,
        catalog_release: null,
      },
      candidates: [
        {
          artifact_id: 'artifact:dlss:3.7.20',
          file_name: 'nvngx_dlss.dll',
          file_path: `${RENDERPILOT_LIBRARY_PATH}/nvngx_dlss_3.7.20.dll`,
          technical_version: '3.7.20',
          release_label: null,
          source_game_id: 'steam:1245620',
          comparison: 'newer_version',
          catalog_package: {
            package_id: 'nvidia-dlss-3.7.20',
            release: { version: '3.7.20', channel: 'stable', label: null },
            availability: 'available',
            automatic_selection_allowed: true,
          },
          is_downloaded: true,
          is_debug: false,
          sha256: 'mock-sha256-dlss-3720',
          d3d12_executable_action: null,
        },
      ],
    },
    {
      component_id: 'component:cp2077:dlssg',
      technology: 'dlss_frame_generation',
      file_path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlssg.dll',
      version_report: {
        kind: 'known',
        technical_version: '3.5.0',
        release_label: null,
        catalog_release: null,
      },
      candidates: [
        {
          artifact_id: 'artifact:dlssg:3.7.10',
          file_name: 'nvngx_dlssg.dll',
          file_path: `${RENDERPILOT_LIBRARY_PATH}/nvngx_dlssg_3.7.10.dll`,
          technical_version: '3.7.10',
          release_label: null,
          source_game_id: 'steam:1716740',
          comparison: 'newer_version',
          catalog_package: {
            package_id: 'nvidia-dlssg-3.7.10',
            release: { version: '3.7.10', channel: 'stable', label: null },
            availability: 'available',
            automatic_selection_allowed: true,
          },
          is_downloaded: true,
          is_debug: false,
          sha256: 'mock-sha256-dlssg-3710',
          d3d12_executable_action: null,
        },
      ],
    },
    {
      component_id: 'component:cp2077:d3d12',
      technology: 'd3d12_agility',
      file_path: 'C:/Games/Cyberpunk 2077/bin/x64/D3D12Core.dll',
      version_report: {
        kind: 'known',
        technical_version: '1.606.4',
        release_label: null,
        catalog_release: null,
      },
      candidates: [
        {
          artifact_id: 'artifact:d3d12:1.619.1',
          file_name: 'D3D12Core.dll',
          file_path: `${RENDERPILOT_LIBRARY_PATH}/D3D12Core_1.619.1.dll`,
          technical_version: '1.619.1',
          release_label: null,
          source_game_id: null,
          comparison: 'newer_version',
          catalog_package: {
            package_id: 'Microsoft.Direct3D.D3D12:1.619.1',
            release: { version: '1.619.1', channel: 'stable', label: null },
            availability: 'available',
            automatic_selection_allowed: true,
          },
          is_downloaded: true,
          is_debug: false,
          sha256: 'mock-sha256-d3d12-16191',
          d3d12_executable_action: {
            kind: 'patch',
            executable_path: 'C:/Games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe',
            backup_path: 'C:/Games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe.bak',
            backup_exists: false,
            original_sdk_version: 606,
            current_sdk_version: 606,
            target_sdk_version: 619,
            requires_confirmation: true,
          },
        },
      ],
    },
  ];

  return {
    game: {
      identity: {
        id: 'steam:1091500',
        title: 'Cyberpunk 2077',
        launcher: 'Steam',
        external_id: '1091500',
      },
      platform: 'Windows',
      runtime: 'NativeWindows',
      install_path: 'C:/Games/Cyberpunk 2077',
      executable_candidates: ['C:/Games/Cyberpunk 2077/bin/x64/Cyberpunk2077.exe'],
    },
    components,
    candidate_groups: candidateGroups,
    operations: [],
    addon_capabilities: [],
  };
}
