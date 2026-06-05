import type { GameDetails, GameSummary } from '@entities/game';
import type { CandidateGroup, GraphicsComponent } from '@entities/component';
import { isKnownLibrary } from '@shared/graphics';
import { createInstallPathKey, unique } from './desktop-utils';
type ComponentFile = GraphicsComponent['files'][number];

export type GameSummaryBuildOverrides = Pick<
  GameSummary,
  'risk_level' | 'rollback_available' | 'last_operation_status'
>;

export type GameSummaryPatch = Partial<
  Pick<
    GameSummary,
    'rollback_available' | 'last_operation_status' | 'operation_count' | 'cover_updated_at_ms'
  >
>;

export type MockState = {
  games: GameSummary[];
  detailsByGameId: Map<string, GameDetails>;
  autoGameIds: Set<string>;
  manualGameIdByInstallPath: Map<string, string>;
  manualCounter: number;
  catalogSettings: Map<string, string>;
};

export const RENDERPILOT_LIBRARY_PATH = 'C:/RenderPilot/Library';

export const mockState: MockState = createMockState();

export function createMockState(): MockState {
  const cyberpunk = createCyberpunkDetails();
  const alanWake = createAlanWakeDetails();

  const seedGames = [
    {
      details: cyberpunk,
      card: createGameSummaryFromDetails(cyberpunk, {
        risk_level: 'low',
        rollback_available: true,
        last_operation_status: getLatestOperationStatus(cyberpunk),
      }),
    },
    {
      details: alanWake,
      card: createGameSummaryFromDetails(alanWake, {
        risk_level: 'medium',
        rollback_available: false,
        last_operation_status: getLatestOperationStatus(alanWake),
      }),
    },
  ];

  return {
    games: seedGames.map(({ card }) => card),
    detailsByGameId: new Map(
      seedGames.map(({ details }) => [details.game.identity.id, details] as const),
    ),
    autoGameIds: new Set(seedGames.map(({ details }) => details.game.identity.id)),
    manualGameIdByInstallPath: new Map(),
    manualCounter: 0,
    catalogSettings: new Map(),
  };
}

export function findGameSummary(gameId: string): GameSummary | undefined {
  return mockState.games.find((game) => game.game_id === gameId);
}

export function updateGameSummary(gameId: string, patch: GameSummaryPatch): void {
  const index = mockState.games.findIndex((game) => game.game_id === gameId);

  if (index === -1) {
    throw new Error(`Mock preview could not find game summary ${gameId}.`);
  }

  const nextGames = [...mockState.games];

  nextGames[index] = {
    ...nextGames[index],
    ...patch,
  };

  mockState.games = nextGames;
}

export function upsertGameSummary(card: GameSummary): void {
  mockState.games = [card, ...mockState.games.filter((game) => game.game_id !== card.game_id)];
}

export function requireGameDetails(gameId: string): GameDetails {
  const details = mockState.detailsByGameId.get(gameId);

  if (!details) {
    throw new Error(`Mock preview could not find game ${gameId}.`);
  }

  return details;
}

export function requireComponent(details: GameDetails, componentId: string): GraphicsComponent {
  const component = details.components.find((item) => item.id === componentId);

  if (!component) {
    throw new Error(
      `Mock preview could not find component ${componentId} for ${details.game.identity.id}.`,
    );
  }

  return component;
}

export function requireCandidateGroup(details: GameDetails, componentId: string): CandidateGroup {
  const candidateGroup = details.candidate_groups.find(
    (group) => group.component_id === componentId,
  );

  if (!candidateGroup) {
    throw new Error(`Mock preview could not find candidate group for component ${componentId}.`);
  }

  return candidateGroup;
}

export function requireFirstComponentFile(component: GraphicsComponent): ComponentFile {
  if (component.files.length === 0) {
    throw new Error(`Mock preview component ${component.id} does not contain any files.`);
  }

  return component.files[0];
}

export function updateCandidateGroupCurrentVersion(
  details: GameDetails,
  componentId: string,
  version: string | null,
): void {
  const candidateGroup = details.candidate_groups.find(
    (group) => group.component_id === componentId,
  );

  if (candidateGroup) {
    candidateGroup.current_version = version;
  }
}

export function getLatestOperationStatus(
  details: GameDetails,
): GameSummary['last_operation_status'] {
  if (details.operations.length === 0) {
    return null;
  }

  return details.operations[0].status;
}

export function getOrCreateManualGameId(installPath: string): string {
  const key = createInstallPathKey(installPath);
  const existingGameId = mockState.manualGameIdByInstallPath.get(key);

  if (existingGameId) {
    return existingGameId;
  }

  mockState.manualCounter += 1;

  const gameId = `manual:preview:${mockState.manualCounter}`;

  mockState.manualGameIdByInstallPath.set(key, gameId);

  return gameId;
}

export function createGameSummaryFromDetails(
  details: GameDetails,
  overrides: GameSummaryBuildOverrides,
): GameSummary {
  const visibleComponents = details.components.filter((component) =>
    isKnownLibrary(component.technology),
  );
  const visibleComponentIds = new Set(visibleComponents.map((component) => component.id));
  const visibleCandidateGroups = details.candidate_groups.filter((group) =>
    visibleComponentIds.has(group.component_id),
  );

  return {
    game_id: details.game.identity.id,
    title: details.game.identity.title,
    launcher: details.game.identity.launcher,
    platform: details.game.platform,
    runtime: details.game.runtime,
    install_path: details.game.install_path,
    external_id: details.game.identity.external_id,
    library_tags: unique(visibleComponents.map((component) => component.technology.trim())),
    component_count: visibleComponents.length,
    updates_available: hasAvailableUpdates(visibleCandidateGroups),
    update_count: countAvailableUpdates(visibleCandidateGroups),
    risk_level: overrides.risk_level,
    rollback_available: overrides.rollback_available,
    operation_count: details.operations.length,
    last_operation_status: overrides.last_operation_status,
    cover_updated_at_ms: null,
    is_favorite: false,
    is_hidden: false,
  };
}

function hasAvailableUpdates(candidateGroups: readonly CandidateGroup[]): boolean {
  return countAvailableUpdates(candidateGroups) > 0;
}

function countAvailableUpdates(candidateGroups: readonly CandidateGroup[]): number {
  return candidateGroups.filter((group) =>
    group.candidates.some((candidate) => candidate.comparison === 'newer_version'),
  ).length;
}

function createCyberpunkDetails(): GameDetails {
  const components: GraphicsComponent[] = [
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

  const candidateGroups: CandidateGroup[] = [
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
          warning: null,
          is_downloaded: true,
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
          warning: null,
          is_downloaded: true,
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
  };
}

function createAlanWakeDetails(): GameDetails {
  const components: GraphicsComponent[] = [
    {
      id: 'component:aw2:streamline',
      game_id: 'epic:alanwake2',
      kind: 'StreamlineComponent',
      technology: 'nvidia_streamline',
      swappability: 'BundleOnly',
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
      executable_candidates: ['D:/Epic Games/Alan Wake 2/AlanWake2.exe'],
    },
    components,
    candidate_groups: [
      {
        component_id: 'component:aw2:streamline',
        technology: 'nvidia_streamline',
        file_path: 'D:/Epic Games/Alan Wake 2/sl.common.dll',
        current_version: '2.4.0',
        candidates: [
          {
            artifact_id: 'artifact:streamline:2.5.1',
            file_name: 'sl.common.dll',
            file_path: `${RENDERPILOT_LIBRARY_PATH}/sl.common_2.5.1.dll`,
            version: '2.5.1',
            source_game_id: 'steam:1888930',
            comparison: 'newer_version',
            is_downloaded: true,
          },
        ],
      },
    ],
    operations: [],
  };
}

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
        current_version: '3.5.10',
        candidates: [
          {
            artifact_id: `artifact:${gameId}:dlss-preview`,
            file_name: 'nvngx_dlss.dll',
            file_path: `${RENDERPILOT_LIBRARY_PATH}/nvngx_dlss_preview.dll`,
            version: '3.7.20',
            source_game_id: null,
            comparison: 'newer_version',
            is_downloaded: true,
          },
        ],
      },
    ],
    operations: [],
  };
}
