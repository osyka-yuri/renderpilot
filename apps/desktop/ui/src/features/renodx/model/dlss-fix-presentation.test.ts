import { describe, expect, it } from 'vitest';

import { presentDlssFix } from './dlss-fix-presentation';
import type { DlssFixAvailability } from './types';

function binding(
  state: Extract<DlssFixAvailability, { kind: 'binding' }>['state'],
  actions: Extract<DlssFixAvailability, { kind: 'binding' }>['actions'],
): DlssFixAvailability {
  return { kind: 'binding', state, actions };
}

describe('presentDlssFix', () => {
  it('hides an unavailable companion without recorded evidence', () => {
    expect(
      presentDlssFix({
        availability: binding('none', []),
        fallbackEvidencePresent: false,
        updateStatus: null,
      }),
    ).toEqual({ kind: 'hidden' });
  });

  it('offers exactly one install action without claiming an installed component', () => {
    expect(
      presentDlssFix({
        availability: binding('none', ['install']),
        fallbackEvidencePresent: false,
        updateStatus: null,
      }),
    ).toEqual({
      kind: 'component',
      primaryAction: {
        kind: 'install',
        labelKey: 'gameDetails.renodx.actionInstallDlssFix',
      },
      canRemove: false,
      descriptionKey: 'gameDetails.renodx.component.dlssFixOffer',
      status: undefined,
    });
  });

  it('presents a bound companion as updateable and removable', () => {
    expect(
      presentDlssFix({
        availability: binding('bound', ['update', 'remove']),
        fallbackEvidencePresent: false,
        updateStatus: 'available',
      }),
    ).toEqual({
      kind: 'component',
      primaryAction: { kind: 'update', labelKey: 'gameDetails.renodx.actionUpdate' },
      canRemove: true,
      descriptionKey: 'gameDetails.renodx.component.dlssFixDesc',
      status: 'available',
    });
  });

  it('uses the dedicated repair label for partial evidence', () => {
    expect(
      presentDlssFix({
        availability: binding('source_only', ['repair', 'remove']),
        fallbackEvidencePresent: false,
        updateStatus: 'current',
      }),
    ).toEqual({
      kind: 'component',
      primaryAction: {
        kind: 'repair',
        labelKey: 'gameDetails.renodx.actionRepairDlssFix',
      },
      canRemove: true,
      descriptionKey: 'gameDetails.renodx.component.dlssFixDesc',
      status: undefined,
    });
  });

  it('keeps validation as a status instead of manufacturing an action', () => {
    expect(
      presentDlssFix({
        availability: binding('invalid', ['validation_required']),
        fallbackEvidencePresent: false,
        updateStatus: 'current',
      }),
    ).toEqual({
      kind: 'component',
      primaryAction: null,
      canRemove: false,
      descriptionKey: 'gameDetails.renodx.component.dlssFixDesc',
      status: 'unknown_needs_validation',
    });
  });

  it('preserves recorded evidence when the dedicated availability probe fails', () => {
    expect(
      presentDlssFix({
        availability: null,
        fallbackEvidencePresent: true,
        updateStatus: 'current',
      }),
    ).toEqual({
      kind: 'component',
      primaryAction: null,
      canRemove: false,
      descriptionKey: 'gameDetails.renodx.component.dlssFixDesc',
      status: 'current',
    });
  });

  it('keeps interrupted recovery separate from normal component actions', () => {
    expect(
      presentDlssFix({
        availability: { kind: 'recovery_pending', actions: ['retry_recovery'] },
        fallbackEvidencePresent: false,
        updateStatus: null,
      }),
    ).toEqual({ kind: 'recovery_pending' });
  });

  it('fails closed when the backend exposes conflicting primary actions', () => {
    expect(
      presentDlssFix({
        availability: binding('invalid', ['install', 'repair', 'remove']),
        fallbackEvidencePresent: false,
        updateStatus: 'current',
      }),
    ).toEqual({
      kind: 'component',
      primaryAction: null,
      canRemove: true,
      descriptionKey: 'gameDetails.renodx.component.dlssFixDesc',
      status: 'unknown_needs_validation',
    });
  });
});
