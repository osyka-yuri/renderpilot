import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { configureErrorDiagnosticSink, type ErrorDiagnosticEvent } from '@shared/diagnostics';
import { ClientError } from '@shared/errors';

const invokeDesktop = vi.hoisted(() => vi.fn());

vi.mock('@shared/api', () => ({ invokeDesktop }));

import { addGame, inspectGameInstall } from './desktop';

describe('scan-libraries desktop boundary', () => {
  const events: ErrorDiagnosticEvent[] = [];

  beforeEach(() => {
    events.length = 0;
    invokeDesktop.mockReset();
    configureErrorDiagnosticSink({ report: (event) => events.push(event) });
  });

  afterEach(() => {
    configureErrorDiagnosticSink(null);
  });

  it('reports an invalid inspection warning once when the response enters the feature', async () => {
    invokeDesktop.mockResolvedValueOnce({ warnings: [{ code: 'future_warning' }] });

    const result = await inspectGameInstall('C:/Games/Example');

    expect(events).toEqual([
      expect.objectContaining({
        operation: 'inspect_game_install',
        code: 'future_warning',
        contractStatus: 'unknown',
      }),
    ]);
    expect(result.warnings).toEqual([
      { contractStatus: 'unknown', code: 'future_warning', parameters: {} },
    ]);
  });

  it('uses the add-game operation name for malformed result warnings', async () => {
    invokeDesktop.mockResolvedValueOnce({
      warnings: [{ code: 'legacy_cards_consolidated', parameters: { count: 0 } }],
    });

    await addGame({
      selectedRoot: 'C:/Games/Example',
      rootChoice: 'selected',
      allowRootCorrection: false,
      chosenExecutable: null,
      inspectionFingerprint: 'inspection:v1:test',
    });

    expect(events).toEqual([
      expect.objectContaining({
        operation: 'add_game',
        code: 'legacy_cards_consolidated',
        contractStatus: 'malformed',
      }),
    ]);
  });

  it.each([{}, { warnings: null }, { warnings: {} }, { details: 'PRIVATE backend prose' }])(
    'rejects a malformed warnings collection as a transport failure',
    async (response) => {
      invokeDesktop.mockResolvedValueOnce(response);

      const error = await inspectGameInstall('C:/Games/Example').catch((cause: unknown) => cause);
      expect(error).toBeInstanceOf(ClientError);
      expect(error).toMatchObject({
        code: 'desktop_transport_failed',
      });
      expect(JSON.stringify(error)).not.toContain('PRIVATE');
      expect(events).toEqual([
        expect.objectContaining({
          operation: 'inspect_game_install',
          code: 'desktop_transport_failed',
          contractStatus: 'known',
        }),
      ]);
    },
  );
});
