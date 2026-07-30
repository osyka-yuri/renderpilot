import { describe, expect, it } from 'vitest';

import {
  canDismissDialog,
  dialogFooter,
  failureDescriptionKey,
  failureKind,
  failureTitleKey,
  phaseStatusKey,
  progressPhase,
} from './dialog-view';
import type { AppUpdateDialogState, AppUpdateOffer } from './types';

const offer: AppUpdateOffer = {
  currentVersion: '1.0.0',
  version: '1.1.0',
  releaseTimestamp: null,
  releaseNotes: { blocks: [], truncated: false },
};

const dialogByPhase = {
  available: { phase: 'available', offer },
  downloading: {
    phase: 'downloading',
    offer,
    progress: { ratio: null, receivedBytes: 0, totalBytes: null, networkFinished: false },
  },
  'retrying-download': { phase: 'retrying-download', offer },
  verifying: {
    phase: 'verifying',
    offer,
    progress: { ratio: null, receivedBytes: 0, totalBytes: null, networkFinished: false },
  },
  installing: { phase: 'installing', offer },
  restarting: { phase: 'restarting', offer },
  'prepare-failed': { phase: 'prepare-failed', offer },
  'install-failed': { phase: 'install-failed', offer },
  'restart-required': { phase: 'restart-required', offer },
} as const satisfies Record<AppUpdateDialogState['phase'], AppUpdateDialogState>;

function dialog(phase: AppUpdateDialogState['phase']): AppUpdateDialogState {
  return dialogByPhase[phase];
}

describe('dialog-view', () => {
  it('maps progress phases only for active dialog phases', () => {
    expect(progressPhase(dialog('downloading'))).toBe('downloading');
    expect(progressPhase(dialog('retrying-download'))).toBe('retrying-download');
    expect(progressPhase(dialog('verifying'))).toBe('verifying');
    expect(progressPhase(dialog('installing'))).toBe('installing');
    expect(progressPhase(dialog('restarting'))).toBe('restarting');
    expect(progressPhase(dialog('available'))).toBeNull();
    expect(progressPhase(null)).toBeNull();
  });

  it('maps failure kinds and translated copy keys', () => {
    expect(failureKind(dialog('prepare-failed'))).toBe('prepare-failed');
    expect(failureKind(dialog('install-failed'))).toBe('install-failed');
    expect(failureKind(dialog('restart-required'))).toBe('restart-required');
    expect(failureKind(dialog('available'))).toBeNull();

    expect(failureTitleKey('prepare-failed')).toBe('settings.about.updateDialog.prepareErrorTitle');
    expect(failureDescriptionKey('install-failed')).toBe(
      'settings.about.updateDialog.installErrorDescription',
    );
    expect(phaseStatusKey('downloading')).toBe('settings.about.updateDialog.downloading');
  });

  it('keeps dismissal and footer policy in one exhaustive projection', () => {
    const phases: AppUpdateDialogState['phase'][] = [
      'available',
      'downloading',
      'retrying-download',
      'verifying',
      'installing',
      'restarting',
      'prepare-failed',
      'install-failed',
      'restart-required',
    ];

    expect(
      Object.fromEntries(phases.map((phase) => [phase, dialogFooter(dialog(phase))?.kind ?? null])),
    ).toEqual({
      available: 'install',
      downloading: null,
      'retrying-download': null,
      verifying: null,
      installing: null,
      restarting: null,
      'prepare-failed': 'retry-download',
      'install-failed': 'retry-install',
      'restart-required': 'restart',
    });
    expect(phases.filter((phase) => canDismissDialog(dialog(phase)))).toEqual([
      'available',
      'prepare-failed',
      'install-failed',
      'restart-required',
    ]);
  });
});
