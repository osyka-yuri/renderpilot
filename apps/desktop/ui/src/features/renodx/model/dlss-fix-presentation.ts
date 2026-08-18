import type { UpdateStatus } from '@entities/addon';
import type { MessageKeyWithoutParams } from '@shared/i18n';

import type { DlssFixAction, DlssFixAvailability } from './types';

type DlssFixPrimaryActionKind = Extract<DlssFixAction, 'install' | 'update' | 'repair'>;

type DlssFixPrimaryAction = {
  kind: DlssFixPrimaryActionKind;
  labelKey: MessageKeyWithoutParams;
};

export type DlssFixPresentation =
  | { kind: 'hidden' }
  | { kind: 'recovery_pending' }
  | {
      kind: 'component';
      primaryAction: DlssFixPrimaryAction | null;
      canRemove: boolean;
      descriptionKey: MessageKeyWithoutParams;
      status: UpdateStatus | undefined;
    };

type DlssFixPresentationInput = {
  availability: DlssFixAvailability | null;
  fallbackEvidencePresent: boolean;
  updateStatus: UpdateStatus | null;
};

const PRIMARY_ACTION_LABELS = {
  install: 'gameDetails.renodx.actionInstallDlssFix',
  update: 'gameDetails.renodx.actionUpdate',
  repair: 'gameDetails.renodx.actionRepairDlssFix',
} satisfies Record<DlssFixPrimaryActionKind, MessageKeyWithoutParams>;

/**
 * Converts backend-authored DLSS-Fix capabilities into one UI projection.
 *
 * The backend remains the sole authority for allowed actions. This presenter
 * only selects the single primary button and display metadata; it never
 * re-derives ownership policy from the binding state.
 */
export function presentDlssFix({
  availability,
  fallbackEvidencePresent,
  updateStatus,
}: DlssFixPresentationInput): DlssFixPresentation {
  if (availability?.kind === 'recovery_pending') {
    return { kind: 'recovery_pending' };
  }

  const actions = availability?.actions ?? [];
  const evidencePresent = availability ? availability.state !== 'none' : fallbackEvidencePresent;
  if (!evidencePresent && actions.length === 0) {
    return { kind: 'hidden' };
  }

  const primaryActionKind = selectPrimaryAction(actions);
  return {
    kind: 'component',
    primaryAction: primaryActionKind
      ? {
          kind: primaryActionKind,
          labelKey: PRIMARY_ACTION_LABELS[primaryActionKind],
        }
      : null,
    canRemove: actions.includes('remove'),
    descriptionKey: evidencePresent
      ? 'gameDetails.renodx.component.dlssFixDesc'
      : 'gameDetails.renodx.component.dlssFixOffer',
    status: resolveStatus(actions, primaryActionKind, evidencePresent, updateStatus),
  };
}

function selectPrimaryAction(actions: readonly DlssFixAction[]): DlssFixPrimaryActionKind | null {
  let selected: DlssFixPrimaryActionKind | null = null;
  for (const action of actions) {
    if (isPrimaryAction(action)) {
      if (selected) {
        return null;
      }
      selected = action;
    }
  }
  return selected;
}

function resolveStatus(
  actions: readonly DlssFixAction[],
  primaryAction: DlssFixPrimaryActionKind | null,
  evidencePresent: boolean,
  updateStatus: UpdateStatus | null,
): UpdateStatus | undefined {
  const conflictingPrimaryActions =
    primaryAction === null && actions.some((action) => isPrimaryAction(action));
  if (actions.includes('validation_required') || conflictingPrimaryActions) {
    return 'unknown_needs_validation';
  }
  if (!evidencePresent || primaryAction === 'install' || primaryAction === 'repair') {
    return undefined;
  }
  return updateStatus ?? undefined;
}

function isPrimaryAction(action: DlssFixAction): action is DlssFixPrimaryActionKind {
  return action === 'install' || action === 'update' || action === 'repair';
}
