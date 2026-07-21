import type { MessageKey } from '@shared/i18n';

import type { AppUpdateDialogState } from './types';

export type UpdateProgressPhase = 'downloading' | 'verifying' | 'installing' | 'restarting';

export type UpdateFailureKind = 'prepare-failed' | 'install-failed' | 'restart-required';

export type UpdateDialogFooter =
  | { kind: 'install' }
  | { kind: 'retry-download' }
  | { kind: 'retry-install' }
  | { kind: 'restart' };

const PHASE_STATUS_KEY: Record<UpdateProgressPhase, MessageKey> = {
  downloading: 'settings.about.updateDialog.downloading',
  verifying: 'settings.about.updateDialog.verifying',
  installing: 'settings.about.updateDialog.installing',
  restarting: 'settings.about.updateDialog.restarting',
};

const FAILURE_TITLE_KEY: Record<UpdateFailureKind, MessageKey> = {
  'prepare-failed': 'settings.about.updateDialog.prepareErrorTitle',
  'install-failed': 'settings.about.updateDialog.installErrorTitle',
  'restart-required': 'settings.about.updateDialog.restartRequiredTitle',
};

const FAILURE_DESCRIPTION_KEY: Record<UpdateFailureKind, MessageKey> = {
  'prepare-failed': 'settings.about.updateDialog.prepareErrorDescription',
  'install-failed': 'settings.about.updateDialog.installErrorDescription',
  'restart-required': 'settings.about.updateDialog.restartRequiredDescription',
};

export function canDismissDialog(state: AppUpdateDialogState | null): boolean {
  return (
    state?.phase === 'available' ||
    state?.phase === 'prepare-failed' ||
    state?.phase === 'install-failed' ||
    state?.phase === 'restart-required'
  );
}

export function progressPhase(state: AppUpdateDialogState | null): UpdateProgressPhase | null {
  switch (state?.phase) {
    case 'downloading':
    case 'verifying':
    case 'installing':
    case 'restarting':
      return state.phase;
    default:
      return null;
  }
}

export function phaseStatusKey(phase: UpdateProgressPhase): MessageKey {
  return PHASE_STATUS_KEY[phase];
}

export function failureKind(state: AppUpdateDialogState | null): UpdateFailureKind | null {
  switch (state?.phase) {
    case 'prepare-failed':
    case 'install-failed':
    case 'restart-required':
      return state.phase;
    default:
      return null;
  }
}

export function failureTitleKey(kind: UpdateFailureKind): MessageKey {
  return FAILURE_TITLE_KEY[kind];
}

export function failureDescriptionKey(kind: UpdateFailureKind): MessageKey {
  return FAILURE_DESCRIPTION_KEY[kind];
}

export function dialogFooter(state: AppUpdateDialogState | null): UpdateDialogFooter | null {
  switch (state?.phase) {
    case 'available':
      return { kind: 'install' };
    case 'downloading':
    case 'verifying':
    case 'installing':
    case 'restarting':
      return null;
    case 'prepare-failed':
      return { kind: 'retry-download' };
    case 'install-failed':
      return { kind: 'retry-install' };
    case 'restart-required':
      return { kind: 'restart' };
    default:
      return null;
  }
}
