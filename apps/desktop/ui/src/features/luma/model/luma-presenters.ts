import { ADDON_DISPLAY_NAME } from '@shared/model';
import {
  createReshadePresenters,
  formatHostDescription,
  type HostDescription,
  type HostDetection,
  type HostFacts,
  type RiskAssessment,
} from '@entities/addon';

import type { LumaActions } from './types';

const presenters = createReshadePresenters('gameDetails.luma', ADDON_DISPLAY_NAME.luma);

export function getReshadeDescription(input: {
  detection: HostDetection;
  facts: HostFacts;
  actions?: LumaActions;
}): HostDescription {
  const description = presenters.getReshadeDescription(input);
  if (
    input.facts.update_status !== 'unknown_needs_validation' ||
    input.actions === undefined ||
    input.actions.update !== undefined ||
    input.actions.repair !== undefined ||
    description.kind !== 'parts'
  ) {
    return description;
  }

  // A compatible runtime recovered from disk (or a reused user runtime) has
  // no truthful source/channel and deliberately no host update action. The
  // generic facts value is still useful to the backend, but presenting it as
  // an instruction to validate would contradict that lifecycle contract.
  return {
    ...description,
    parts: description.parts.filter(
      (part) => part.kind !== 'message' || part.key !== 'gameDetails.luma.fresh.validationRequired',
    ),
  };
}

/**
 * Renders an install-risk message: the backend's own `message_key` when it's
 * in the catalog, else the severity-based fallback — either way interpolated
 * with Luma's display name (the risk copy is addon-agnostic).
 */
export function riskMessage(risk: RiskAssessment): string {
  return presenters.riskMessage(risk);
}

/** Structured host description → single i18n string (tool keys via factory). */
export function describeReshadeHost(input: {
  detection: HostDetection;
  facts: HostFacts;
  actions?: LumaActions;
}): string {
  return formatHostDescription(getReshadeDescription(input));
}
