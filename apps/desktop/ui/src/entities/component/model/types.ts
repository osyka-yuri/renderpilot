import type { FilePath, Nullable, Sha256Hash, Version } from '@shared/types';
export type ComponentId = string;
export type ArtifactId = string;
export type GameId = string;

export type ComponentKind = string;
export type Swappability = string;
export type Technology = string;

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

export type BuildPlanHandler = (componentId: ComponentId, artifactId: ArtifactId) => void;
