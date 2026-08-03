import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { configureErrorDiagnosticSink, type ErrorDiagnosticEvent } from '@shared/diagnostics';
import { t } from '@shared/i18n';

import {
  formatAddGameWarning,
  normalizeAddGameWarnings,
  presentAddGameWarning,
} from './add-game-warning';

describe('add-game warning contract', () => {
  const events: ErrorDiagnosticEvent[] = [];

  beforeEach(() => {
    events.length = 0;
    configureErrorDiagnosticSink({ report: (event) => events.push(event) });
  });

  afterEach(() => {
    configureErrorDiagnosticSink(null);
  });

  it('normalizes and translates known warnings once at the response boundary', () => {
    const warnings = normalizeAddGameWarnings(
      [
        { code: 'filesystem_probe_error' },
        { code: 'legacy_cards_consolidated', parameters: { count: 2 } },
        {
          code: 'recovery_bundle_created',
          parameters: { path: ' C:/catalog/recovery.bundle ' },
        },
      ],
      'inspect_game_install',
    );

    expect(warnings.map(formatAddGameWarning)).toEqual([
      t('addGame.warning.filesystemProbeError'),
      t('addGame.warning.legacyCardsConsolidated', { count: 2 }),
      t('addGame.warning.recoveryBundleCreated', { path: 'C:/catalog/recovery.bundle' }),
    ]);
    expect(events).toEqual([]);
  });

  it('strips arbitrary data from unknown and malformed warnings', () => {
    const warnings = normalizeAddGameWarnings(
      [
        { code: 'future_warning', parameters: { secret: 'PRIVATE' } },
        { code: 'legacy_cards_retained', parameters: { count: '2', secret: 'PRIVATE' } },
        { details: 'PRIVATE' },
      ],
      'inspect_game_install',
    );

    expect(warnings).toEqual([
      { contractStatus: 'unknown', code: 'future_warning', parameters: {} },
      { contractStatus: 'malformed', code: 'legacy_cards_retained', parameters: {} },
      { contractStatus: 'malformed', code: 'invalid_add_game_warning', parameters: {} },
    ]);
    expect(JSON.stringify(warnings)).not.toContain('PRIVATE');
    expect(warnings.map(presentAddGameWarning).map(({ message }) => message)).toEqual([
      t('addGame.warning.unknown'),
      t('addGame.warning.unknown'),
      t('addGame.warning.unknown'),
    ]);
    expect(events).toEqual([
      expect.objectContaining({ code: 'future_warning', contractStatus: 'unknown' }),
      expect.objectContaining({ code: 'legacy_cards_retained', contractStatus: 'malformed' }),
      expect.objectContaining({ code: 'invalid_add_game_warning', contractStatus: 'malformed' }),
    ]);
  });

  it('rejects unsafe paths and extra fields before presentation', () => {
    const warnings = normalizeAddGameWarnings(
      [
        {
          code: 'recovery_bundle_created',
          parameters: { path: 'C:/Recovery/bundle\nPRIVATE' },
        },
        { code: 'filesystem_probe_error', unexpected: true },
      ],
      'add_game',
    );

    expect(warnings.every(({ contractStatus }) => contractStatus === 'malformed')).toBe(true);
    expect(warnings.map(formatAddGameWarning)).toEqual([
      t('addGame.warning.unknown'),
      t('addGame.warning.unknown'),
    ]);
    expect(events).toHaveLength(2);
  });
});
