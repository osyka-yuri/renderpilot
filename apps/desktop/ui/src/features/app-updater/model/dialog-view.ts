import type { MessageKey } from '@shared/i18n';

import type { AppUpdateDialogState } from './types';

export type UpdateProgressPhase =
  'downloading' | 'retrying-download' | 'verifying' | 'installing' | 'restarting';

export type UpdateFailureKind = 'prepare-failed' | 'install-failed' | 'restart-required';

export type UpdateDialogFooter =
  | { kind: 'install' }
  | { kind: 'retry-download' }
  | { kind: 'retry-install' }
  | { kind: 'restart' };

type DialogProjection = {
  dismissible: boolean;
  progress: UpdateProgressPhase | null;
  failure: UpdateFailureKind | null;
  footer: UpdateDialogFooter | null;
};

const DIALOG_PROJECTION = {
  available: {
    dismissible: true,
    progress: null,
    failure: null,
    footer: { kind: 'install' },
  },
  downloading: {
    dismissible: false,
    progress: 'downloading',
    failure: null,
    footer: null,
  },
  'retrying-download': {
    dismissible: false,
    progress: 'retrying-download',
    failure: null,
    footer: null,
  },
  verifying: {
    dismissible: false,
    progress: 'verifying',
    failure: null,
    footer: null,
  },
  installing: {
    dismissible: false,
    progress: 'installing',
    failure: null,
    footer: null,
  },
  restarting: {
    dismissible: false,
    progress: 'restarting',
    failure: null,
    footer: null,
  },
  'prepare-failed': {
    dismissible: true,
    progress: null,
    failure: 'prepare-failed',
    footer: { kind: 'retry-download' },
  },
  'install-failed': {
    dismissible: true,
    progress: null,
    failure: 'install-failed',
    footer: { kind: 'retry-install' },
  },
  'restart-required': {
    dismissible: true,
    progress: null,
    failure: 'restart-required',
    footer: { kind: 'restart' },
  },
} as const satisfies Record<AppUpdateDialogState['phase'], DialogProjection>;

const PHASE_STATUS_KEY: Record<UpdateProgressPhase, MessageKey> = {
  downloading: 'settings.about.updateDialog.downloading',
  'retrying-download': 'settings.about.updateDialog.downloading',
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
  return state === null ? false : DIALOG_PROJECTION[state.phase].dismissible;
}

export function progressPhase(state: AppUpdateDialogState | null): UpdateProgressPhase | null {
  return state === null ? null : DIALOG_PROJECTION[state.phase].progress;
}

export function phaseStatusKey(phase: UpdateProgressPhase): MessageKey {
  return PHASE_STATUS_KEY[phase];
}

export function failureKind(state: AppUpdateDialogState | null): UpdateFailureKind | null {
  return state === null ? null : DIALOG_PROJECTION[state.phase].failure;
}

export function failureTitleKey(kind: UpdateFailureKind): MessageKey {
  return FAILURE_TITLE_KEY[kind];
}

export function failureDescriptionKey(kind: UpdateFailureKind): MessageKey {
  return FAILURE_DESCRIPTION_KEY[kind];
}

export function dialogFooter(state: AppUpdateDialogState | null): UpdateDialogFooter | null {
  return state === null ? null : DIALOG_PROJECTION[state.phase].footer;
}
