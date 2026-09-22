import type { CatalogMessage } from '@entities/addon';

export type OptiScalerCompatibilityBlockCode =
  | 'requires_x64'
  | 'unsupported_graphics_api'
  | 'catalog_unsupported'
  | 'input_not_detected'
  | 'release_unavailable'
  | 'selected_modules_unavailable'
  | 'catalog_identity_conflict'
  | 'luma_prerequisite'
  | 'proxy_conflict';

export type OptiScalerRelocationBlockCode = 'active_peer_cross_target_unsupported';

export type OptiScalerModuleAvailability = {
  id: string;
  selected: boolean;
  optional: boolean;
  available: boolean;
  requires: string[];
  conflicts: string[];
  description: string;
};

export type OptiScalerCompatibilityStatus = 'working' | 'conditional' | 'untested' | 'unsupported';

export type OptiScalerCompatibilityGuidanceKind = 'warning' | 'compatibility' | 'game_setting';

export type OptiScalerCompatibilityGuidance = {
  kind: OptiScalerCompatibilityGuidanceKind;
  message: CatalogMessage;
};

export type OptiScalerPrerequisiteState =
  | 'none'
  | 'satisfied'
  | 'remove_reno_dx'
  | 'luma_torn'
  | 'luma_broken'
  | 'install_luma'
  | 'luma_unavailable';

export type OptiScalerAvailability = {
  game_id: string;
  launcher: string;
  install: {
    installed: boolean;
    release: string | null;
  };
  eligibility: {
    available: boolean;
    block_code: OptiScalerCompatibilityBlockCode | null;
  };
  selected_release: string | null;
  relocation: {
    target_exe: string;
    display_name: string;
    available: boolean;
    block_code: OptiScalerRelocationBlockCode | null;
  } | null;
  proxy_conflict: string | null;
  compatibility: {
    status: OptiScalerCompatibilityStatus;
    /** Exact reviewed catalogue inputs, never inferred from physical evidence. */
    declared_inputs: ('dlss2_plus' | 'fsr2_plus' | 'xess')[];
    launch: {
      arguments: string[];
      requirement: 'required' | 'recommended';
    } | null;
    guidance: OptiScalerCompatibilityGuidance[];
  };
  prerequisite: {
    state: OptiScalerPrerequisiteState;
  };
  modules: OptiScalerModuleAvailability[];
  lifecycle: {
    update_available: boolean;
    repair_required: boolean;
    drifted: boolean;
    unmanaged: boolean;
    maintenance_available: boolean;
    maintenance_block_code: OptiScalerCompatibilityBlockCode | null;
  };
};

export type OptiScalerOperationResult = {
  installed: boolean;
  release: string | null;
  changed_count: number;
  preserved_count: number;
  config_conflicts: string[];
};

export type OptiScalerUpdateCheck = {
  overall: 'current' | 'available' | 'unknown' | 'channel_mismatch' | 'unknown_needs_validation';
  installed_release: string | null;
  available_release: string | null;
  update_available: boolean;
  repair_required: boolean;
  drifted: boolean;
};
