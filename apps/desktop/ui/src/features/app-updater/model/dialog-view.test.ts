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
  date: null,
  releaseNotes: { blocks: [], truncated: false },
};

function dialog(phase: AppUpdateDialogState['phase']): AppUpdateDialogState {
  if (phase === 'downloading' || phase === 'verifying') {
    return {
      phase,
      offer,
      progress: { percent: null, receivedBytes: 0, totalBytes: null, networkFinished: false },
    };
  }
  return { phase, offer };
}

describe('dialog-view', () => {
  it('maps progress phases only for active dialog phases', () => {
    expect(progressPhase(dialog('downloading'))).toBe('downloading');
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
