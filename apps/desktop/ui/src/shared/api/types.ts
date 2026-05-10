export type UnixTimestampMs = number;

export type Nullable<T> = T | null;

export type GameId = string;
export type ComponentId = string;
export type ArtifactId = string;
export type OperationId = string;
export type BackupId = string;
export type ConfirmationToken = string;

export type FilePath = string;
export type Sha256Hash = string;
export type Version = string;

export type RiskLevel = string;

export type CommandErrorSeverity = 'warning' | 'error';

export type OperationStatus = string;
export type OperationKind = string;
export type BackupStatus = string;

export type Launcher = string;
export type Platform = string;
export type Runtime = string;
export type Technology = string;
export type ComponentKind = string;
export type Swappability = string;

export type GameIdentity = {
  id: GameId;
  title: string;
  launcher: Launcher;
  external_id?: Nullable<string>;
};

export type GameInstallation = {
  identity: GameIdentity;
  platform: Platform;
  runtime: Runtime;
  install_path: FilePath;
  executable_candidates: FilePath[];
};

export type GameCard = {
  game_id: GameId;
  title: string;
  launcher: Launcher;
  platform: Platform;
  runtime: Runtime;
  install_path: FilePath;
  external_id?: Nullable<string>;

  library_tags: Technology[];
  component_count: number;

  updates_available: boolean;
  update_count: number;

  risk_level: RiskLevel;
  backup_available: boolean;

  operation_count: number;
  last_operation_status?: Nullable<OperationStatus>;

  /** Present when a cover image is stored for this game (Unix ms); drives custom-protocol artwork URLs. */
  cover_updated_at_ms?: Nullable<number>;
};

export type CommandErrorDto = {
  code: string;
  severity: CommandErrorSeverity;
  messageKey: string;
  details: string;
  suggestedActions: string[];
};

export type CoverArtworkResult = {
  file_name: string;
  updated_at_ms: number;
};

export type CatalogSettingPayload = {
  value: string | null;
};

export type GameCardsSortField = 'title' | 'updates' | 'risk';
export type GameCardsSortDirection = 'asc' | 'desc';

export type GameCardsQuery = {
  searchQuery: string;
  selectedLibraries: string[];
  sort: {
    field: GameCardsSortField;
    direction: GameCardsSortDirection;
  };
  page: {
    limit: number;
    offset: number;
  };
};

export type GameCardsResult = {
  items: GameCard[];
  total: number;
  availableLibraries: string[];
  queryFingerprint: string;
};

export type ComponentFile = {
  path: FilePath;
  version?: Nullable<Version>;
  sha256?: Nullable<Sha256Hash>;
};

export type GraphicsComponent = {
  id: ComponentId;
  game_id: GameId;
  kind: ComponentKind;
  technology: Technology;
  swappability: Swappability;
  files: ComponentFile[];
};

export type CandidateComparison = string;

export type Candidate = {
  artifact_id: ArtifactId;
  file_name: string;
  file_path: FilePath;
  version?: Nullable<Version>;
  source_game_id?: Nullable<GameId>;
  comparison: CandidateComparison;
  warning?: Nullable<string>;
};

export type CandidateGroup = {
  component_id: ComponentId;
  technology: Technology;
  file_path: FilePath;
  current_version?: Nullable<Version>;
  candidates: Candidate[];
};

export type OperationSummary = {
  operation_id: OperationId;
  kind: OperationKind;
  status: OperationStatus;
  created_at: UnixTimestampMs;
  completed_at?: Nullable<UnixTimestampMs>;
  item_count: number;
  backup_count: number;
  backup_status: BackupStatus;
};

export type GameListResponse = {
  games: GameInstallation[];
};

export type ScanError = {
  root: string;
  message: string;
};

export type AutoScanResponse = {
  games: GameDetails[];
  /** Omitted by the Rust backend when empty; treat as `[]` when absent. */
  errors?: ScanError[];
};

export type GameDetails = {
  game: GameInstallation;
  components: GraphicsComponent[];
  candidate_groups: CandidateGroup[];
  operations: OperationSummary[];
};

export type SwapPlan = {
  operation_id: OperationId;
  confirmation_token: ConfirmationToken;

  game_id: GameId;
  operation_type: OperationKind;

  target_path: FilePath;
  replacement_path: FilePath;

  original_version?: Nullable<Version>;
  replacement_version?: Nullable<Version>;

  original_sha256?: Nullable<Sha256Hash>;
  replacement_sha256?: Nullable<Sha256Hash>;

  risk_level: RiskLevel;

  requires_backup: boolean;
  requires_elevation: boolean;

  artifact_id: ArtifactId;

  blockers: string[];
  warnings: string[];
};

export type AppliedOperationItem = {
  backup_id: BackupId;
  component_id: ComponentId;
  applied_path: FilePath;
  replacement_path: FilePath;
  backup_path: FilePath;
};

export type RollbackOperationItem = {
  backup_id: BackupId;
  component_id: ComponentId;
  restored_path: FilePath;
  backup_path: FilePath;
};

export type ApplyOperationResult = {
  operation_id: OperationId;
  game_id: GameId;
  status: OperationStatus;
  completed_at?: Nullable<UnixTimestampMs>;
  items: AppliedOperationItem[];
};

export type RollbackOperationResult = {
  operation_id: OperationId;
  game_id: GameId;
  status: OperationStatus;
  completed_at?: Nullable<UnixTimestampMs>;
  items: RollbackOperationItem[];
};
