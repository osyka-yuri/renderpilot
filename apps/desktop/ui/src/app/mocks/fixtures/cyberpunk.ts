import type { GameCandidateGroup, GameDetails, GameGraphicsComponent } from '@entities/game';

import { RENDERPILOT_LIBRARY_PATH } from './library-path';

/** Seed fixture: Cyberpunk 2077 with DLSS SR/G/R and two upgrade candidates. */
export function createCyberpunkDetails(): GameDetails {
  const components: GameGraphicsComponent[] = [
    {
      id: 'component:cp2077:dlss',
      game_id: 'steam:1091500',
      kind: 'NativeLibrary',
      technology: 'dlss_super_resolution',
      swappability: 'Swappable',
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
      files: [
        {
          path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlssd.dll',
          version: '3.5.0',
          sha256: '87caea2055c54a4a4eab8408c0f59ef7554cfa663093735dd57637b510b7a0b5',
        },
      ],
    },
  ];

  const candidateGroups: GameCandidateGroup[] = [
    {
      component_id: 'component:cp2077:dlss',
      technology: 'dlss_super_resolution',
      file_path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlss.dll',
      current_version: '3.5.10',
      candidates: [
        {
          artifact_id: 'artifact:dlss:3.7.20',
          file_name: 'nvngx_dlss.dll',
          file_path: `${RENDERPILOT_LIBRARY_PATH}/nvngx_dlss_3.7.20.dll`,
          version: '3.7.20',
          source_game_id: 'steam:1245620',
          comparison: 'newer_version',
          is_downloaded: true,
          is_debug: false,
          sha256: 'mock-sha256-dlss-3720',
        },
      ],
    },
    {
      component_id: 'component:cp2077:dlssg',
      technology: 'dlss_frame_generation',
      file_path: 'C:/Games/Cyberpunk 2077/bin/x64/nvngx_dlssg.dll',
      current_version: '3.5.0',
      candidates: [
        {
          artifact_id: 'artifact:dlssg:3.7.10',
          file_name: 'nvngx_dlssg.dll',
          file_path: `${RENDERPILOT_LIBRARY_PATH}/nvngx_dlssg_3.7.10.dll`,
          version: '3.7.10',
          source_game_id: 'steam:1716740',
          comparison: 'newer_version',
          is_downloaded: true,
          is_debug: false,
          sha256: 'mock-sha256-dlssg-3710',
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
