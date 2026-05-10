import type {
  ApplyOperationResult,
  AutoScanResponse,
  CandidateGroup,
  CatalogSettingPayload,
  CoverArtworkResult,
  GameCard,
  GameCardsQuery,
  GameCardsResult,
  GameDetails,
  GraphicsComponent,
  RollbackOperationResult,
  SwapPlan,
} from './types';
import { normalizeGameCardsQuery } from './game-cards-query';

type ScanManualFolderResult = {
  games: GameDetails[];
};

type GameOperation = GameDetails['operations'][number];

type GameCardBuildOverrides = Pick<
  GameCard,
  'risk_level' | 'backup_available' | 'last_operation_status' | 'update_count'
>;

type GameCardPatch = Partial<
  Pick<
    GameCard,
    'backup_available' | 'last_operation_status' | 'operation_count' | 'cover_updated_at_ms'
  >
>;

type OperationTarget = {
  details: GameDetails;
  operation: GameOperation;
};

type PendingSwapPlan = {
  plan: SwapPlan;
  componentId: string;
};

type AppliedOperationSnapshot = {
  gameId: string;
  componentId: string;
  targetPath: string;
  originalVersion: string | null;
  originalSha256: string | null;
  backupPath: string;
};

type MockState = {
  games: GameCard[];
  detailsByGameId: Map<string, GameDetails>;
  autoGameIds: Set<string>;
  pendingPlansByOperationId: Map<string, PendingSwapPlan>;
  appliedOperationsById: Map<string, AppliedOperationSnapshot>;
  manualGameIdByInstallPath: Map<string, string>;
  rolledBackOperationIds: Set<string>;
  manualCounter: number;
  operationCounter: number;
  catalogSettings: Map<string, string>;
};

const RENDERPILOT_LIBRARY_PATH = 'C:/RenderPilot/Library';
const BACKUP_SUFFIX = '.renderpilot-backup';

const mockState: MockState = createMockState();

export function isDesktopPreviewMode(): boolean {
  return !hasTauriBridge();
}

export function mockScanManualFolder(path: string): Promise<ScanManualFolderResult> {
  return resolveMock(() => {
    const installPath = normalizeInstallPath(path);
    const gameId = getOrCreateManualGameId(installPath);
    const title = lastPathSegment(installPath) || 'Manual Game';

    const previousDetails = mockState.detailsByGameId.get(gameId);
    const details = createManualPreviewDetails(gameId, title, installPath);

    details.operations = previousDetails ? clone(previousDetails.operations) : [];

    mockState.detailsByGameId.set(gameId, details);

    upsertGameCard(
      createGameCardFromDetails(details, {
        update_count: details.candidate_groups.length,
        risk_level: 'medium',
        backup_available: hasAvailableBackup(details),
        last_operation_status: getLatestOperationStatus(details),
      }),
    );

    return {
      games: [clone(details)],
    };
  });
}

export function mockScanAutoLibraries(): Promise<AutoScanResponse> {
  return resolveMock(() => {
    const games = [...mockState.autoGameIds].map((gameId) => requireGameDetails(gameId));

    return {
      games: clone(games),
      errors: [],
    };
  });
}

export function mockQueryGameCards(query: GameCardsQuery): Promise<GameCardsResult> {
  return resolveMock(() => {
    const normalizedQuery = normalizeGameCardsQuery(query);
    const allCards = clone(mockState.games);
    const availableLibraries = collectAvailableLibraries(allCards);
    const selectedLibraries = normalizedQuery.selectedLibraries.filter((library) =>
      availableLibraries.includes(library),
    );

    let filtered = allCards.filter((card) =>
      normalizedQuery.searchQuery.length === 0
        ? true
        : card.title.toLowerCase().includes(normalizedQuery.searchQuery.toLowerCase()),
    );

    if (selectedLibraries.length > 0) {
      const selected = new Set(selectedLibraries);
      filtered = filtered.filter((card) =>
        card.library_tags.some((library) => selected.has(library)),
      );
    }

    filtered.sort((left, right) => compareCards(left, right, normalizedQuery.sort));
    const total = filtered.length;
    const items = filtered.slice(
      normalizedQuery.page.offset,
      normalizedQuery.page.offset + normalizedQuery.page.limit,
    );

    return {
      items,
      total,
      availableLibraries,
      queryFingerprint: JSON.stringify(normalizedQuery),
    };
  });
}

export function mockGetGameDetails(gameId: string): Promise<GameDetails> {
  return resolveMock(() => clone(requireGameDetails(gameId)));
}

export function mockFetchGameCover(gameId: string): Promise<CoverArtworkResult> {
  return resolveMock(() => {
    requireGameDetails(gameId);
    const updated_at_ms = Date.now();
    updateGameCard(gameId, { cover_updated_at_ms: updated_at_ms });

    return {
      file_name: `cover-${gameId}-mock.png`,
      updated_at_ms,
    };
  });
}

export function mockClearGameCover(gameId: string): Promise<{ cleared: boolean }> {
  return resolveMock(() => {
    requireGameDetails(gameId);
    updateGameCard(gameId, { cover_updated_at_ms: null });

    return { cleared: true };
  });
}

export function mockSetGameCover(gameId: string, sourcePath: string): Promise<CoverArtworkResult> {
  return resolveMock(() => {
    void sourcePath;
    requireGameDetails(gameId);
    const updated_at_ms = Date.now();
    updateGameCard(gameId, { cover_updated_at_ms: updated_at_ms });

    return {
      file_name: `cover-${gameId}-picked.png`,
      updated_at_ms,
    };
  });
}

export function mockGetCatalogSetting(key: string): Promise<CatalogSettingPayload> {
  return resolveMock(() => ({
    value: mockState.catalogSettings.get(key) ?? null,
  }));
}

export function mockSetCatalogSetting(key: string, value: string): Promise<{ saved: boolean }> {
  return resolveMock(() => {
    if (value.trim().length === 0) {
      mockState.catalogSettings.delete(key);
    } else {
      mockState.catalogSettings.set(key, value);
    }

    return { saved: true };
  });
}

export function resetMockDesktopState(): void {
  const nextState = createMockState();
  mockState.games = nextState.games;
  mockState.detailsByGameId = nextState.detailsByGameId;
  mockState.autoGameIds = nextState.autoGameIds;
  mockState.pendingPlansByOperationId = nextState.pendingPlansByOperationId;
  mockState.appliedOperationsById = nextState.appliedOperationsById;
  mockState.manualGameIdByInstallPath = nextState.manualGameIdByInstallPath;
  mockState.rolledBackOperationIds = nextState.rolledBackOperationIds;
  mockState.manualCounter = nextState.manualCounter;
  mockState.operationCounter = nextState.operationCounter;
  mockState.catalogSettings = nextState.catalogSettings;
}

export function mockBuildSwapPlan(
  gameId: string,
  componentId: string,
  artifactId: string,
): Promise<SwapPlan> {
  return resolveMock(() => {
    const details = requireGameDetails(gameId);
    const sourceComponent = requireComponent(details, componentId);
    const candidateGroup = requireCandidateGroup(details, componentId);
    const candidate = candidateGroup.candidates.find((item) => item.artifact_id === artifactId);

    if (!candidate) {
      throw new Error(
        `Mock preview could not find artifact ${artifactId} for component ${componentId}.`,
      );
    }

    const sourceFile = requireFirstComponentFile(sourceComponent);
    const operationId = createPreviewOperationId();

    const plan: SwapPlan = {
      operation_id: operationId,
      confirmation_token: `preview-token:${operationId}`,
      game_id: gameId,
      operation_type: 'replace_component',
      target_path: candidateGroup.file_path,
      replacement_path: candidate.file_path,
      original_version: candidateGroup.current_version ?? sourceFile.version ?? null,
      replacement_version: candidate.version ?? null,
      original_sha256: sourceFile.sha256,
      replacement_sha256: null,
      risk_level: candidate.warning ? 'medium' : 'low',
      requires_backup: true,
      requires_elevation: false,
      artifact_id: artifactId,
      blockers: [],
      warnings: candidate.warning
        ? [candidate.warning]
        : ['confirmation_required_for_swappability'],
    };

    mockState.pendingPlansByOperationId.set(operationId, {
      plan,
      componentId,
    });

    return clone(plan);
  });
}

export function mockApplyOperationPlan(
  operationId: string,
  confirmationToken: string,
): Promise<ApplyOperationResult> {
  return resolveMock(() => {
    const pending = requirePendingSwapPlan(operationId, confirmationToken);
    const { plan, componentId } = pending;

    const details = requireGameDetails(plan.game_id);
    const component = requireComponent(details, componentId);
    const sourceFile = requireFirstComponentFile(component);

    const now = Date.now();
    const backupPath = `${plan.target_path}${BACKUP_SUFFIX}`;

    mockState.appliedOperationsById.set(operationId, {
      gameId: plan.game_id,
      componentId,
      targetPath: plan.target_path,
      originalVersion: plan.original_version ?? null,
      originalSha256: plan.original_sha256 ?? null,
      backupPath,
    });

    sourceFile.version = plan.replacement_version ?? sourceFile.version;
    sourceFile.sha256 = plan.replacement_sha256 ?? sourceFile.sha256;

    updateCandidateGroupCurrentVersion(details, componentId, sourceFile.version ?? null);

    prependOperation(
      details,
      createOperationRecord({
        operationId,
        kind: 'replace_component',
        status: 'completed',
        createdAt: now - 60_000,
        completedAt: now,
        itemCount: 1,
        backupCount: 1,
        backupStatus: 'available',
      }),
    );

    mockState.pendingPlansByOperationId.delete(operationId);

    updateGameCard(plan.game_id, {
      backup_available: true,
      last_operation_status: 'completed',
      operation_count: details.operations.length,
    });

    return clone({
      operation_id: operationId,
      game_id: plan.game_id,
      status: 'completed',
      completed_at: now,
      items: [
        {
          backup_id: `backup:${operationId}`,
          component_id: component.id,
          applied_path: plan.target_path,
          replacement_path: plan.replacement_path,
          backup_path: backupPath,
        },
      ],
    });
  });
}

export function mockRollbackOperation(operationId: string): Promise<RollbackOperationResult> {
  return resolveMock(() => {
    if (mockState.rolledBackOperationIds.has(operationId)) {
      throw new Error(`Mock preview operation ${operationId} has already been rolled back.`);
    }

    const target = findOperationTarget(operationId);

    if (!target) {
      throw new Error(`Mock preview could not find operation ${operationId} to rollback.`);
    }

    if (target.operation.kind === 'rollback_operation') {
      throw new Error(`Mock preview cannot rollback rollback operation ${operationId}.`);
    }

    const { details } = target;
    const snapshot = mockState.appliedOperationsById.get(operationId);

    const component = snapshot
      ? requireComponent(details, snapshot.componentId)
      : findPrimaryRollbackComponent(details);

    if (!component) {
      throw new Error(
        `Mock preview rollback requires at least one component with files for ${details.game.identity.id}.`,
      );
    }

    const sourceFile = requireFirstComponentFile(component);

    if (snapshot) {
      sourceFile.version = snapshot.originalVersion ?? sourceFile.version;
      sourceFile.sha256 = snapshot.originalSha256 ?? sourceFile.sha256;

      updateCandidateGroupCurrentVersion(details, snapshot.componentId, snapshot.originalVersion);
    }

    const now = Date.now();
    const rollbackOperationId = createRollbackOperationId(operationId);
    const restoredPath = snapshot?.targetPath ?? sourceFile.path;
    const backupPath = snapshot?.backupPath ?? `${sourceFile.path}${BACKUP_SUFFIX}`;

    prependOperation(
      details,
      createOperationRecord({
        operationId: rollbackOperationId,
        kind: 'rollback_operation',
        status: 'rolled_back',
        createdAt: now - 20_000,
        completedAt: now,
        itemCount: 1,
        backupCount: 1,
        backupStatus: 'available',
      }),
    );

    mockState.rolledBackOperationIds.add(operationId);

    updateGameCard(details.game.identity.id, {
      backup_available: true,
      last_operation_status: 'rolled_back',
      operation_count: details.operations.length,
    });

    return clone({
      operation_id: rollbackOperationId,
      game_id: details.game.identity.id,
      status: 'rolled_back',
      completed_at: now,
      items: [
        {
          backup_id: `backup:${operationId}`,
          component_id: component.id,
          restored_path: restoredPath,
          backup_path: backupPath,
        },
      ],
    });
  });
}

function createMockState(): MockState {
  const cyberpunk = createCyberpunkDetails();
  const alanWake = createAlanWakeDetails();

  const seedGames = [
    {
      details: cyberpunk,
      card: createGameCardFromDetails(cyberpunk, {
        update_count: cyberpunk.candidate_groups.length,
        risk_level: 'low',
        backup_available: true,
        last_operation_status: getLatestOperationStatus(cyberpunk),
      }),
    },
    {
      details: alanWake,
      card: createGameCardFromDetails(alanWake, {
        update_count: alanWake.candidate_groups.length,
        risk_level: 'medium',
        backup_available: false,
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
    pendingPlansByOperationId: new Map(),
    appliedOperationsById: new Map(),
    manualGameIdByInstallPath: new Map(),
    rolledBackOperationIds: new Set(),
    manualCounter: 0,
    operationCounter: 0,
    catalogSettings: new Map(),
  };
}

function createCyberpunkDetails(): GameDetails {
  const now = Date.now();

  const components: GraphicsComponent[] = [
    {
      id: 'component:cp2077:dlss',
      game_id: 'steam:1091500',
      kind: 'NativeLibrary',
      technology: 'DlssSuperResolution',
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
      technology: 'DlssFrameGeneration',
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
      technology: 'DlssRayReconstruction',
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
      technology: 'DlssSuperResolution',
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
        },
      ],
    },
    {
      component_id: 'component:cp2077:dlssg',
      technology: 'DlssFrameGeneration',
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
    operations: [
      {
        operation_id: 'operation:cp2077:last-swap',
        kind: 'replace_component',
        status: 'completed',
        created_at: now - 86_400_000,
        completed_at: now - 86_340_000,
        item_count: 1,
        backup_count: 1,
        backup_status: 'available',
      },
    ],
  };
}

function createAlanWakeDetails(): GameDetails {
  const now = Date.now();

  const components: GraphicsComponent[] = [
    {
      id: 'component:aw2:streamline',
      game_id: 'epic:alanwake2',
      kind: 'StreamlineComponent',
      technology: 'NvidiaStreamline',
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
      technology: 'DlssFrameGeneration',
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
        technology: 'NvidiaStreamline',
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
            warning: 'streamline_partial_swap',
          },
        ],
      },
    ],
    operations: [
      {
        operation_id: 'operation:aw2:failed-bundle',
        kind: 'replace_component',
        status: 'rollback_required',
        created_at: now - 3_600_000,
        completed_at: null,
        item_count: 2,
        backup_count: 1,
        backup_status: 'partial',
      },
    ],
  };
}

function createManualPreviewDetails(
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
        technology: 'DlssSuperResolution',
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
        technology: 'DlssSuperResolution',
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
            warning: null,
          },
        ],
      },
    ],
    operations: [],
  };
}

function createGameCardFromDetails(
  details: GameDetails,
  overrides: GameCardBuildOverrides,
): GameCard {
  return {
    game_id: details.game.identity.id,
    title: details.game.identity.title,
    launcher: details.game.identity.launcher,
    platform: details.game.platform,
    runtime: details.game.runtime,
    install_path: details.game.install_path,
    external_id: details.game.identity.external_id,
    library_tags: unique(details.components.map((component) => component.technology)),
    component_count: details.components.length,
    updates_available: overrides.update_count > 0,
    update_count: overrides.update_count,
    risk_level: overrides.risk_level,
    backup_available: overrides.backup_available,
    operation_count: details.operations.length,
    last_operation_status: overrides.last_operation_status,
    cover_updated_at_ms: null,
  };
}

function createOperationRecord(input: {
  operationId: string;
  kind: GameOperation['kind'];
  status: GameOperation['status'];
  createdAt: number;
  completedAt: number | null;
  itemCount: number;
  backupCount: number;
  backupStatus: GameOperation['backup_status'];
}): GameOperation {
  return {
    operation_id: input.operationId,
    kind: input.kind,
    status: input.status,
    created_at: input.createdAt,
    completed_at: input.completedAt,
    item_count: input.itemCount,
    backup_count: input.backupCount,
    backup_status: input.backupStatus,
  };
}

function prependOperation(details: GameDetails, operation: GameOperation): void {
  details.operations = [operation, ...details.operations];
}

function updateGameCard(gameId: string, patch: GameCardPatch): void {
  mockState.games = mockState.games.map((game) =>
    game.game_id === gameId ? { ...game, ...patch } : game,
  );
}

function upsertGameCard(card: GameCard): void {
  mockState.games = [card, ...mockState.games.filter((game) => game.game_id !== card.game_id)];
}

function requireGameDetails(gameId: string): GameDetails {
  const details = mockState.detailsByGameId.get(gameId);

  if (!details) {
    throw new Error(`Mock preview could not find game ${gameId}.`);
  }

  return details;
}

function requirePendingSwapPlan(operationId: string, confirmationToken: string): PendingSwapPlan {
  const pending = mockState.pendingPlansByOperationId.get(operationId);

  if (!pending) {
    throw new Error(`Mock preview could not find operation plan ${operationId}.`);
  }

  if (pending.plan.confirmation_token !== confirmationToken) {
    throw new Error('Confirmation token mismatch for operation preview.');
  }

  return pending;
}

function requireComponent(details: GameDetails, componentId: string): GraphicsComponent {
  const component = details.components.find((item) => item.id === componentId);

  if (!component) {
    throw new Error(
      `Mock preview could not find component ${componentId} for ${details.game.identity.id}.`,
    );
  }

  return component;
}

function requireCandidateGroup(details: GameDetails, componentId: string): CandidateGroup {
  const candidateGroup = details.candidate_groups.find(
    (group) => group.component_id === componentId,
  );

  if (!candidateGroup) {
    throw new Error(`Mock preview could not find candidate group for component ${componentId}.`);
  }

  return candidateGroup;
}

function requireFirstComponentFile(
  component: GraphicsComponent,
): GraphicsComponent['files'][number] {
  if (component.files.length === 0) {
    throw new Error(`Mock preview component ${component.id} does not contain any files.`);
  }

  return component.files[0];
}

function updateCandidateGroupCurrentVersion(
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

function findOperationTarget(operationId: string): OperationTarget | null {
  for (const details of mockState.detailsByGameId.values()) {
    const operation = details.operations.find((item) => item.operation_id === operationId);

    if (operation) {
      return { details, operation };
    }
  }

  return null;
}

function findPrimaryRollbackComponent(details: GameDetails): GraphicsComponent | null {
  return details.components.find((item) => item.files.length > 0) ?? null;
}

function getLatestOperationStatus(details: GameDetails): GameCard['last_operation_status'] {
  return details.operations[0]?.status ?? null;
}

function hasAvailableBackup(details: GameDetails): boolean {
  return details.operations.some(
    (operation) => operation.backup_status === 'available' || operation.backup_status === 'partial',
  );
}

function getOrCreateManualGameId(installPath: string): string {
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

function createPreviewOperationId(): string {
  mockState.operationCounter += 1;

  return `operation:preview:${mockState.operationCounter}`;
}

function createRollbackOperationId(targetOperationId: string): string {
  mockState.operationCounter += 1;

  return `operation:rollback:${mockState.operationCounter}:${targetOperationId}`;
}

function lastPathSegment(path: string): string {
  const segments = normalizeWindowsSlashes(path).split('/').filter(Boolean);

  if (segments.length === 0) {
    return '';
  }

  return segments[segments.length - 1];
}

function normalizeInstallPath(path: string): string {
  const normalized = normalizeWindowsSlashes(path.trim()).replace(/\/+$/, '');

  if (!normalized) {
    throw new Error('Mock preview manual scan path is required.');
  }

  return normalized;
}

function normalizeWindowsSlashes(path: string): string {
  return path.replace(/\\/g, '/');
}

function createInstallPathKey(path: string): string {
  return normalizeInstallPath(path).toLowerCase();
}

function unique<T>(items: readonly T[]): T[] {
  return [...new Set(items)];
}

function collectAvailableLibraries(cards: readonly GameCard[]): string[] {
  const values = new Set<string>();
  for (const card of cards) {
    for (const library of card.library_tags) {
      values.add(library);
    }
  }
  return [...values].sort((left, right) => left.localeCompare(right));
}

function compareCards(left: GameCard, right: GameCard, sort: GameCardsQuery['sort']): number {
  const direction = sort.direction === 'asc' ? 1 : -1;
  const byTitle =
    left.title.localeCompare(right.title) || left.game_id.localeCompare(right.game_id);

  if (sort.field === 'title') {
    return byTitle * direction;
  }
  if (sort.field === 'updates') {
    const updatesDiff = left.update_count - right.update_count;
    if (updatesDiff !== 0) {
      return updatesDiff * direction;
    }
    return byTitle;
  }

  const riskDiff = left.risk_level.localeCompare(right.risk_level);
  if (riskDiff !== 0) {
    return riskDiff * direction;
  }
  return byTitle;
}

function hasTauriBridge(): boolean {
  return typeof globalThis === 'object' && '__TAURI_INTERNALS__' in globalThis;
}

function resolveMock<T>(factory: () => T): Promise<T> {
  try {
    return Promise.resolve(factory());
  } catch (error) {
    return Promise.reject(toError(error));
  }
}

function toError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }

  if (typeof error === 'string') {
    return new Error(error);
  }

  return new Error('Mock preview command failed.');
}

function clone<T>(value: T): T {
  const serialized = JSON.stringify(value);

  if (typeof serialized !== 'string') {
    throw new Error('Mock preview could not clone a non-serializable value.');
  }

  return JSON.parse(serialized) as T;
}
