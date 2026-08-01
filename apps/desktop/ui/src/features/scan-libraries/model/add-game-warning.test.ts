import { describe, expect, it } from 'vitest';
import { t } from '@shared/i18n';

import { formatAddGameWarning } from './add-game-warning';

describe('formatAddGameWarning', () => {
  it('translates a known warning without parameters', () => {
    expect(
      formatAddGameWarning({
        code: 'probe_incomplete',
        message: 'Backend fallback.',
      }),
    ).toBe(t('addGame.warning.probeIncomplete'));
  });

  it('translates structured numeric and interpolation parameters', () => {
    expect(
      formatAddGameWarning({
        code: 'legacy_cards_consolidated',
        message: 'Backend fallback.',
        parameters: { count: 2 },
      }),
    ).toBe(t('addGame.warning.legacyCardsConsolidated', { count: 2 }));
    expect(
      formatAddGameWarning({
        code: 'recovery_bundle_created',
        message: 'Backend fallback.',
        parameters: { path: 'C:/catalog/recovery.bundle' },
      }),
    ).toBe(t('addGame.warning.recoveryBundleCreated', { path: 'C:/catalog/recovery.bundle' }));
  });

  it('preserves the original backend fallback for malformed structured warnings', () => {
    const fallback = 'Fallback with {backend-braces}.';

    expect(
      formatAddGameWarning({
        code: 'legacy_cards_retained',
        message: fallback,
        parameters: { count: '2' },
      }),
    ).toBe(fallback);
    expect(
      formatAddGameWarning({
        code: 'root_correction_history_archived',
        message: fallback,
        parameters: { unrelated: 'value' },
      }),
    ).toBe(fallback);
  });

  it('preserves the fallback for forward-compatible warning codes', () => {
    expect(
      formatAddGameWarning({
        code: 'future_warning',
        message: 'A warning from a newer backend.',
      }),
    ).toBe('A warning from a newer backend.');
  });
});
