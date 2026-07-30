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

export type LibraryComponent = {
  id: ComponentId;
  game_id: GameId;
  kind: ComponentKind;
  technology: Technology;
  swappability: Swappability;
  files: ComponentFile[];
};

export type BuildPlanHandler = (componentId: ComponentId, artifactId: ArtifactId) => void;
