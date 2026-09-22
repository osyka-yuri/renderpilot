import type { AddonBadgeTone, CatalogMessage } from '@entities/addon';
import { OPTISCALER_MESSAGE_CONTEXTS, OPTISCALER_SOURCE_CATALOG } from '@shared/i18n';

import type {
  OptiScalerAvailability,
  OptiScalerCompatibilityGuidance,
  OptiScalerCompatibilityStatus,
  OptiScalerModuleAvailability,
} from './types';

export type OptiScalerCardState = 'installed' | 'installable' | 'blocked' | 'unmanaged';

export function selectedModuleIds(report: OptiScalerAvailability): string[] {
  return report.modules.filter((module) => module.selected).map((module) => module.id);
}

/**
 * Resolves a compatibility callout through the desktop's reviewed message
 * contract. The backend selects an id; it cannot supply display prose.
 */
export function trustedOptiScalerGuidance(
  guidance: OptiScalerCompatibilityGuidance,
): CatalogMessage | null {
  const id = guidance.message.id;
  if (
    !Object.hasOwn(OPTISCALER_SOURCE_CATALOG, id) ||
    OPTISCALER_MESSAGE_CONTEXTS[id as keyof typeof OPTISCALER_MESSAGE_CONTEXTS] !== guidance.kind
  ) {
    return null;
  }
  return {
    id,
    fallback_text: OPTISCALER_SOURCE_CATALOG[id as keyof typeof OPTISCALER_SOURCE_CATALOG],
  };
}

export function toggleSelectedModule(
  modules: OptiScalerModuleAvailability[],
  selected: string[],
  id: string,
  checked: boolean,
): string[] {
  const byId = new Map(modules.map((module) => [module.id, module]));
  const next = new Set(selected);

  function removeWithDependents(moduleId: string): void {
    const module = byId.get(moduleId);
    if (module && !module.optional) {
      return;
    }
    next.delete(moduleId);
    for (const candidate of modules) {
      if (next.has(candidate.id) && candidate.requires.includes(moduleId)) {
        removeWithDependents(candidate.id);
      }
    }
  }

  function addWithRequirements(moduleId: string): void {
    const module = byId.get(moduleId);
    if (!module?.available) {
      return;
    }
    next.add(moduleId);
    for (const requiredId of module.requires) {
      addWithRequirements(requiredId);
    }
  }

  const module = byId.get(id);
  if (checked) {
    if (module?.available) {
      for (const conflictId of module.conflicts) {
        removeWithDependents(conflictId);
      }
      for (const candidate of modules) {
        if (candidate.conflicts.includes(id)) {
          removeWithDependents(candidate.id);
        }
      }
      addWithRequirements(id);
    }
  } else if (module?.optional !== false) {
    removeWithDependents(id);
  }

  for (const required of modules.filter((candidate) => !candidate.optional)) {
    addWithRequirements(required.id);
  }

  const known = modules
    .filter((candidate) => next.has(candidate.id))
    .map((candidate) => candidate.id);
  const unknown = selected.filter((moduleId) => !byId.has(moduleId) && next.has(moduleId));
  return [...known, ...unknown];
}

export function optiscalerCardState(report: OptiScalerAvailability): OptiScalerCardState {
  if (report.lifecycle.unmanaged) {
    return 'unmanaged';
  }
  if (report.install.installed) {
    return 'installed';
  }
  if (optiscalerPrerequisiteUnresolved(report)) {
    return 'blocked';
  }
  if (report.eligibility.available && !report.proxy_conflict) {
    return 'installable';
  }
  return 'blocked';
}

export function optiscalerManagedTargetAvailable(report: OptiScalerAvailability | null): boolean {
  return Boolean(report?.lifecycle.maintenance_available && !report.lifecycle.unmanaged);
}

export function optiscalerActionAvailability(
  report: OptiScalerAvailability,
  installed: boolean,
  busy: boolean,
) {
  const targetAvailable = optiscalerManagedTargetAvailable(report);
  const install = optiscalerInstallEligible(report, installed);
  return {
    install: !busy && install,
    update: !busy && installed && targetAvailable,
    modules: !busy && installed && targetAvailable,
    repair: !busy && installed && targetAvailable,
    relocate: !busy && installed && (report.relocation?.available ?? false),
    uninstall: !busy && installed,
  };
}

/**
 * Whether the install control belongs to the current card state.
 *
 * Visibility intentionally does not depend on an in-flight mutation: a busy
 * card keeps its install control in place and disables it, rather than making
 * the action disappear mid-operation.
 */
export function optiscalerInstallVisible(
  report: OptiScalerAvailability,
  installed: boolean,
): boolean {
  return !installed && !report.lifecycle.unmanaged;
}

function optiscalerPrerequisiteUnresolved(report: OptiScalerAvailability): boolean {
  return report.prerequisite.state !== 'none' && report.prerequisite.state !== 'satisfied';
}

function optiscalerInstallEligible(report: OptiScalerAvailability, installed: boolean): boolean {
  const targetAvailable =
    report.eligibility.available &&
    !report.proxy_conflict &&
    !report.lifecycle.unmanaged &&
    (report.prerequisite.state === 'none' || report.prerequisite.state === 'satisfied');
  return !installed && targetAvailable;
}

const declaredInputLabels = {
  dlss2_plus: 'DLSS 2+',
  fsr2_plus: 'FSR 2+',
  xess: 'XeSS',
};

export function formatOptiScalerDeclaredInput(input: keyof typeof declaredInputLabels): string {
  return declaredInputLabels[input];
}

export function optiscalerControlId(gameId: string, scope: string): string {
  return `optiscaler-${gameId}-${scope}`.replace(/[^A-Za-z0-9_-]/g, '-');
}

export function optiscalerCompatibilityBadgeTone(
  status: OptiScalerCompatibilityStatus,
): AddonBadgeTone {
  switch (status) {
    case 'working':
      return 'verified';
    case 'conditional':
      return 'experimental';
    case 'untested':
      return 'untested';
    case 'unsupported':
      return 'unsupported';
  }
}

export type OptiScalerAlertTone = 'default' | 'warning';
export type OptiScalerAlertIcon = 'info' | 'warning';

export type OptiScalerAlertPresentation = {
  tone: OptiScalerAlertTone;
  icon: OptiScalerAlertIcon;
};

export function optiscalerBlockedAlertPresentation(
  report: OptiScalerAvailability,
): OptiScalerAlertPresentation {
  if (report.prerequisite.state === 'install_luma') {
    return { tone: 'default', icon: 'info' };
  }
  return { tone: 'warning', icon: 'warning' };
}
