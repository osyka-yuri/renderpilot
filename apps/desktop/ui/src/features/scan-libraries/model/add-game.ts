export type RootRecommendationConfidence = 'authoritative' | 'suggested';
export type InstallBoundaryKind =
  | 'single_install'
  | 'engine_project_subtree'
  | 'binary_subtree'
  | 'single_install_container'
  | 'multiple_install_container'
  | 'ambiguous'
  | 'incomplete';
export type TraversalCompleteness = 'complete' | 'incomplete';
export type InstallBoundaryEvidence =
  | 'launcher_manifest'
  | 'engine_distribution_root'
  | 'root_executable'
  | 'engine_structure'
  | 'component_context'
  | 'executable_branch';
export type RootRecommendationSource =
  | 'launcher_manifest'
  | 'existing_catalog'
  | 'engine_distribution_root'
  | 'root_executable'
  | 'component_context';
export type InstallRelationshipKind =
  | 'new'
  | 'exact_existing'
  | 'inside_existing'
  | 'expands_existing'
  | 'narrows_existing'
  | 'contains_proven_install'
  | 'contains_multiple';

export type ExecutableInspection = {
  path: string;
  relativePath: string;
  sizeBytes: number;
  rankScore: number;
  validWindowsPe: boolean;
  rejectionKind: string | null;
  rejectionToken: string | null;
};

export type AddGameWarning = {
  code: string;
  message: string;
  parameters?: Record<string, string | number>;
};

export type RootCorrectionStatus = 'ready' | 'cleanup_required' | 'blocked';

export type RootCorrectionBlockerKind =
  | 'pending_recovery'
  | 'installed_addon'
  | 'nvapi'
  | 'orphaned_component_baseline';

export type RootCorrectionAssessment = {
  gameId: string;
  status: RootCorrectionStatus;
  cleanupActions: {
    kind: 'rollback_component';
    componentId: string;
  }[];
  blockers: RootCorrectionBlockerKind[];
};

export type AddGameInspection = {
  selectedRoot: string;
  inspectionFingerprint: string;
  catalogGeneration: number;
  boundary: {
    kind: InstallBoundaryKind;
    completeness: TraversalCompleteness;
    candidateRoots: string[];
    evidence: InstallBoundaryEvidence[];
  };
  recommendation: {
    root: string;
    source: RootRecommendationSource;
    confidence: RootRecommendationConfidence;
    completeness: TraversalCompleteness;
    evidence: InstallBoundaryEvidence[];
  } | null;
  relationship: {
    kind: InstallRelationshipKind;
    gameIds: string[];
    provenInstallRoots: string[];
  };
  executables: ExecutableInspection[];
  requiresExplicitExecutable: boolean;
  rootCorrection: RootCorrectionAssessment | null;
  decision: AddGameDecision;
  warnings: AddGameWarning[];
};

export type AddGameRootChoice = 'selected' | 'recommended';
export type AddGameCatalogAction = 'add' | 'rescan' | 'correct_existing_root';
export type AddGameOption = {
  rootChoice: AddGameRootChoice;
  catalogAction: AddGameCatalogAction;
};
export type AddGameUnavailableReason =
  | 'multiple_installs'
  | 'contains_proven_install'
  | 'contains_multiple_catalog_installs'
  | 'inside_existing_install'
  | 'no_readable_executable'
  | 'root_correction_blocked';
export type AddGameDecision =
  | { kind: 'automatic'; option: AddGameOption }
  | { kind: 'review'; defaultOption: AddGameOption; options: [AddGameOption, ...AddGameOption[]] }
  | { kind: 'unavailable'; reasons: AddGameUnavailableReason[] };
export type AutomaticAddGameDecision = Extract<AddGameDecision, { kind: 'automatic' }>;

export type AddGameRequest = {
  selectedRoot: string;
  rootChoice: AddGameRootChoice;
  allowRootCorrection: boolean;
  chosenExecutable: string | null;
  inspectionFingerprint: string;
};

export type AddGameResult = {
  gameId: string;
  effectiveRoot: string;
  disposition: 'added' | 'unchanged' | 'updated' | 'root_corrected';
  rootAuthority: 'launcher_manifest' | 'user_confirmed' | 'legacy';
  detectedLibraryCount: number;
  consolidatedGameIds: string[];
  recoveryBundlePath: string | null;
  warnings: AddGameWarning[];
};

export type AddGameConfirmation = {
  rootChoice: AddGameRootChoice;
  allowRootCorrection: boolean;
  chosenExecutable: string | null;
};

/** Confirmation used when inspection needs no review step. */
export function automaticAddGameConfirmation(
  decision: AutomaticAddGameDecision,
): AddGameConfirmation {
  return {
    rootChoice: decision.option.rootChoice,
    allowRootCorrection: false,
    chosenExecutable: null,
  };
}

/** Root correction offered for one structurally valid manual game card. */
export function hasRootCorrection(inspection: AddGameInspection): boolean {
  return inspection.rootCorrection !== null;
}

export function decisionOptions(decision: AddGameDecision): AddGameOption[] {
  switch (decision.kind) {
    case 'automatic':
      return [decision.option];
    case 'review':
      return decision.options;
    case 'unavailable':
      return [];
  }
}
