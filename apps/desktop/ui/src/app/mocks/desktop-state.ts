import type {
  GameCandidateGroup,
  GameDetails,
  GameLibraryComponent,
  GameSummary,
} from '@entities/game';
import type { LibraryPackageSummary } from '@entities/library';

import { createGameSummaryFromDetails, getLatestOperationStatus } from './build-game-summary';
import { createInstallPathKey } from './desktop-utils';
import {
  createAlanWakeDetails,
  createCyberpunkDetails,
  createMockLibraryPackages,
} from './fixtures';

export type GameSummaryPatch = Partial<
  Pick<
    GameSummary,
    | 'rollback_available'
    | 'last_operation_status'
    | 'operation_count'
    | 'cover_updated_at_ms'
    | 'is_favorite'
    | 'is_hidden'
  >
>;

export type MockState = {
  games: GameSummary[];
  detailsByGameId: Map<string, GameDetails>;
  componentBaselinesByGameId: Map<string, Map<string, ComponentFile[]>>;
  autoGameIds: Set<string>;
  manualGameIdByInstallPath: Map<string, string>;
  manualCounter: number;
  operationSequence: number;
  catalogSettings: Map<string, string>;
  libraryPackages: LibraryPackageSummary[];
};

type ComponentFile = GameLibraryComponent['files'][number];

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
    componentBaselinesByGameId: new Map(),
    autoGameIds: new Set(seedGames.map(({ details }) => details.game.identity.id)),
    manualGameIdByInstallPath: new Map(),
    manualCounter: 0,
    operationSequence: 0,
    catalogSettings: new Map(),
    libraryPackages: createMockLibraryPackages(),
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

export function requireComponent(details: GameDetails, componentId: string): GameLibraryComponent {
  const component = details.components.find((item) => item.id === componentId);

  if (!component) {
    throw new Error(
      `Mock preview could not find component ${componentId} for ${details.game.identity.id}.`,
    );
  }

  return component;
}

export function requireCandidateGroup(
  details: GameDetails,
  componentId: string,
): GameCandidateGroup {
  const candidateGroup = details.candidate_groups.find(
    (group) => group.component_id === componentId,
  );

  if (!candidateGroup) {
    throw new Error(`Mock preview could not find candidate group for component ${componentId}.`);
  }

  return candidateGroup;
}

export function requireFirstComponentFile(component: GameLibraryComponent): ComponentFile {
  if (component.files.length === 0) {
    throw new Error(`Mock preview component ${component.id} does not contain any files.`);
  }

  return component.files[0];
}

export function captureComponentBaseline(gameId: string, component: GameLibraryComponent): void {
  let baselines = mockState.componentBaselinesByGameId.get(gameId);
  if (!baselines) {
    baselines = new Map();
    mockState.componentBaselinesByGameId.set(gameId, baselines);
  }
  if (!baselines.has(component.id)) {
    baselines.set(
      component.id,
      component.files.map((file) => ({ ...file })),
    );
  }
}

export function hasComponentBaseline(gameId: string, componentId: string): boolean {
  return mockState.componentBaselinesByGameId.get(gameId)?.has(componentId) ?? false;
}

export function consumeComponentBaseline(gameId: string, componentId: string): ComponentFile[] {
  const baselines = mockState.componentBaselinesByGameId.get(gameId);
  const baseline = baselines?.get(componentId);
  if (!baseline) {
    throw new Error(`Mock preview has no rollback baseline for component ${componentId}.`);
  }

  baselines?.delete(componentId);
  if (baselines?.size === 0) {
    mockState.componentBaselinesByGameId.delete(gameId);
  }
  return baseline.map((file) => ({ ...file }));
}

export function nextMockOperationId(kind: string): string {
  mockState.operationSequence += 1;
  return `mock-op:${kind}:${mockState.operationSequence}`;
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
    candidateGroup.version_report = version
      ? { kind: 'known', technical_version: version, release_label: null, catalog_release: null }
      : { kind: 'unknown' };
  }
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

/** Prepends an operation summary and keeps summary counters in sync. */
export function recordOperation(
  details: GameDetails,
  operation: GameDetails['operations'][number],
): void {
  details.operations = [operation, ...details.operations];
  updateGameSummary(details.game.identity.id, {
    operation_count: details.operations.length,
    last_operation_status: operation.status,
  });
}
