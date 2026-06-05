import type { FilePath, Nullable } from '@shared/types';

/**
 * Encapsulates the summary of a historical operation, natively embedded within `GameDetails`.
 *
 * Structurally, this mirrors `OperationSummary` from the `entities/operation` slice
 * (representing an identical underlying Rust type). It is deliberately duplicated here
 * to operate within a distinct Tauri command context and strictly prevent architectural
 * cross-slice dependencies between entities.
 */
export type GameOperationSummary = {
  operation_id: string;
  kind: string;
  status: string;
  created_at: number;
  completed_at?: number | null;
  item_count: number;
  component_id?: string;
};

export type GameId = string;

export type GameRiskLevel = 'safe' | 'low' | 'medium' | 'high' | 'blocked' | 'unknown';

export type Launcher = string;

/** Must match `Launcher` serde names from renderpilot-domain (`stable_enum!`). */
export const LAUNCHER_STEAM = 'Steam' as const;
export const LAUNCHER_GOG = 'Gog' as const;

export type Platform = string;
export type Runtime = string;
export type Technology = string;

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

export type GameSummary = {
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

  is_favorite: boolean;
  is_hidden: boolean;

  risk_level: GameRiskLevel;
  rollback_available: boolean;

  operation_count: number;
  last_operation_status?: Nullable<string>;

  /**
   * Populated with a Unix timestamp (milliseconds) exclusively when local cover artwork
   * is successfully cached for this game. This value actively drives cache-busting for
   * custom-protocol artwork URLs.
   */
  cover_updated_at_ms?: Nullable<number>;
};

export type CoverArtworkResult = {
  file_name: string;
  updated_at_ms: number;
};

export type GameCardsSortField = 'title' | 'updates' | 'risk';
export type GameCardsSortDirection = 'asc' | 'desc';

export type GameCardsQuery = {
  searchQuery: string;
  selectedLibraries: string[];
  selectedLaunchers: string[];
  showHidden: boolean;
  favoritesOnly: boolean;
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
  items: GameSummary[];
  total: number;
  hiddenCount: number;
  availableLibraries: string[];
  availableLaunchers: string[];
  queryFingerprint: string;
};

export type GameListResponse = {
  games: GameInstallation[];
};

export type GameSelectionHandler = (gameId: GameId) => void;

export type GameGraphicsComponent = {
  id: string;
  game_id: string;
  kind: string;
  technology: string;
  swappability: string;
  files: {
    path: string;
    version?: string | null;
    sha256?: string | null;
  }[];
  rollback_available?: boolean;
};

export type GameCandidate = {
  artifact_id: string;
  file_name: string;
  file_path: string | null;
  version?: string | null;
  source_game_id?: string | null;
  comparison: string;
  manifest_entry_id?: Nullable<string>;
  is_downloaded: boolean;
  is_debug: boolean;
  sha256: string;
};

export type GameCandidateGroup = {
  component_id: string;
  technology: string;
  file_path: string;
  current_version?: string | null;
  candidates: GameCandidate[];
};

export type GameDetails = {
  game: GameInstallation;
  components: GameGraphicsComponent[];
  candidate_groups: GameCandidateGroup[];
  operations: GameOperationSummary[];
};

export type ScanError = {
  root: string;
  message: string;
};

export type AutoScanResponse = {
  games: GameDetails[];
  /**
   * Explicitly omitted during serialization by the Rust backend when the collection is empty.
   * Clients must robustly handle absence by substituting an empty array `[]`.
   */
  errors?: ScanError[];
};

export type ScanManualFolderResult = {
  games: GameDetails[];
};
