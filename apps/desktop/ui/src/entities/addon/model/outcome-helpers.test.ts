import { describe, expect, it } from 'vitest';

import { deriveCommonOutcomeFields } from './outcome-helpers';

describe('deriveCommonOutcomeFields', () => {
  it('returns empty fields when outcome is null', () => {
    expect(deriveCommonOutcomeFields(null)).toEqual({
      isInstallable: false,
      isIncompatible: false,
      isBlacklisted: false,
      isUnsupported: false,
      isBlockedByOtherAddon: false,
      isUnmanagedPresent: false,
      otherAddonKind: null,
      otherAddonUnmanaged: false,
      confidence: null,
      notesKeys: [],
      blacklistReason: null,
      risk: null,
      requiresConfirmation: false,
    });
  });

  it('maps installable outcomes', () => {
    const fields = deriveCommonOutcomeFields({
      kind: 'installable',
      confidence: 'verified',
      risk: { severity: 'warn', message_key: 'addon.risk.anticheat_detected' },
      notes_keys: ['note.a'],
    });

    expect(fields.isInstallable).toBe(true);
    expect(fields.confidence).toBe('verified');
    expect(fields.notesKeys).toEqual(['note.a']);
    expect(fields.requiresConfirmation).toBe(true);
  });

  it('maps blocked_by_other_addon outcomes', () => {
    const fields = deriveCommonOutcomeFields({
      kind: 'blocked_by_other_addon',
      other_kind: 'renodx',
      unmanaged: true,
    });

    expect(fields.isBlockedByOtherAddon).toBe(true);
    expect(fields.otherAddonKind).toBe('renodx');
    expect(fields.otherAddonUnmanaged).toBe(true);
  });

  it('maps unmanaged_present outcomes', () => {
    expect(deriveCommonOutcomeFields({ kind: 'unmanaged_present' }).isUnmanagedPresent).toBe(true);
  });

  it('ignores tool-specific outcome kinds', () => {
    expect(deriveCommonOutcomeFields({ kind: 'external', url: 'https://x' }).isInstallable).toBe(
      false,
    );
  });
});
