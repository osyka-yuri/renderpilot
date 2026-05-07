export interface GameIdentity {
  id: string;
  title: string;
  launcher: string;
  external_id?: string | null;
}

export interface GameInstallation {
  identity: GameIdentity;
  platform: string;
  runtime: string;
  install_path: string;
  executable_candidates: string[];
}

export interface GameCard {
  game_id: string;
  title: string;
  launcher: string;
  platform: string;
  runtime: string;
  install_path: string;
  external_id?: string | null;
  technology_tags: string[];
  component_count: number;
  updates_available: boolean;
  update_count: number;
  risk_level: string;
  backup_available: boolean;
  operation_count: number;
  last_operation_status?: string | null;
}

export type CommandErrorSeverity = 'warning' | 'error';

export interface CommandErrorDto {
  code: string;
  severity: CommandErrorSeverity;
  message_key: string;
  details: string;
  suggested_actions: string[];
}

export interface ComponentFile {
  path: string;
  version?: string | null;
  sha256?: string | null;
}

export interface GraphicsComponent {
  id: string;
  game_id: string;
  kind: string;
  technology: string;
  swappability: string;
  files: ComponentFile[];
}

export interface Candidate {
  artifact_id: string;
  file_name: string;
  file_path: string;
  version?: string | null;
  source_game_id?: string | null;
  comparison: string;
  warning?: string | null;
}

export interface CandidateGroup {
  component_id: string;
  technology: string;
  file_path: string;
  current_version?: string | null;
  candidates: Candidate[];
}

export interface OperationSummary {
  operation_id: string;
  kind: string;
  status: string;
  created_at: number;
  completed_at?: number | null;
  item_count: number;
  backup_count: number;
  backup_status: string;
}

export interface GameListResponse {
  games: GameInstallation[];
}

export interface GameDetails {
  game: GameInstallation;
  components: GraphicsComponent[];
  candidate_groups: CandidateGroup[];
  operations: OperationSummary[];
}

export interface SwapPlan {
  operation_id: string;
  confirmation_token: string;
  game_id: string;
  operation_type: string;
  target_path: string;
  replacement_path: string;
  original_version?: string | null;
  replacement_version?: string | null;
  original_sha256?: string | null;
  replacement_sha256?: string | null;
  risk_level: string;
  requires_backup: boolean;
  requires_elevation: boolean;
  artifact_id: string;
  blockers: string[];
  warnings: string[];
}

export interface ApplyOperationResult {
  operation_id: string;
  game_id: string;
  status: string;
  completed_at?: number | null;
  items: Array<{
    backup_id: string;
    component_id: string;
    applied_path: string;
    replacement_path: string;
    backup_path: string;
  }>;
}

export interface RollbackOperationResult {
  operation_id: string;
  game_id: string;
  status: string;
  completed_at?: number | null;
  items: Array<{
    backup_id: string;
    component_id: string;
    restored_path: string;
    backup_path: string;
  }>;
}
