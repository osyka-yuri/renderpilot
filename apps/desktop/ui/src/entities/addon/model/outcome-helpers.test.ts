import { describe, expect, it } from 'vitest';

import { deriveCommonOutcomeFields, isCommonAvailabilityOutcome } from './outcome-helpers';

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
      blacklistMessage: null,
      risk: null,
      requiresConfirmation: false,
    });
  });

  it('maps installable outcomes', () => {
    const fields = deriveCommonOutcomeFields({
      kind: 'installable',
      confidence: 'verified',
      risk: { severity: 'warn', message_key: 'addon.risk.anticheat_detected' },
    });

    expect(fields.isInstallable).toBe(true);
    expect(fields.confidence).toBe('verified');
    expect(fields.requiresConfirmation).toBe(true);
  });

  it('accepts tool-specific installable extras without requiring shared notes', () => {
    const lumaOutcome = {
      kind: 'installable',
      confidence: 'verified',
      risk: { severity: 'info', message_key: 'addon.risk.none' },
      features: { dlss_fsr: 'supported', hdr: 'unknown' },
      guidance: [],
      launch_args: ['-dx11'],
    } as const;
    const fields = deriveCommonOutcomeFields(lumaOutcome);

    expect(fields.isInstallable).toBe(true);
    expect(fields.confidence).toBe('verified');
    expect(fields.requiresConfirmation).toBe(false);
    expect(fields.risk).toEqual({ severity: 'info', message_key: 'addon.risk.none' });
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

  it('preserves the full catalogue message for blacklisted outcomes', () => {
    const message = { id: 'addon.blocked.test', fallback_text: 'Known not to work.' };
    const fields = deriveCommonOutcomeFields({ kind: 'blacklisted', message });

    expect(fields.isBlacklisted).toBe(true);
    expect(fields.blacklistMessage).toEqual(message);
  });

  it('maps unmanaged_present outcomes', () => {
    expect(deriveCommonOutcomeFields({ kind: 'unmanaged_present' }).isUnmanagedPresent).toBe(true);
  });

  it('ignores tool-specific outcome kinds', () => {
    expect(isCommonAvailabilityOutcome({ kind: 'external' })).toBe(false);
  });
});
