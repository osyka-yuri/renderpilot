import { afterEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@shared/api-preview', () => ({
  isDesktopPreviewMode: () => false,
  invokePreviewCommand: vi.fn(),
}));

import { configureErrorDiagnosticSink, type ErrorDiagnosticEvent } from '@shared/diagnostics';
import { DesktopCommandError } from '@shared/errors';
import { invokeDesktop } from './desktop-transport';

describe('invokeDesktop error boundary', () => {
  afterEach(() => {
    configureErrorDiagnosticSink(null);
    invoke.mockReset();
  });

  it('normalizes and reports a command rejection exactly once without unsafe event fields', async () => {
    const raw = {
      code: 'storage_failed',
      details: 'PRIVATE C:/Users/name/catalog.db',
    };
    const reports: { event: ErrorDiagnosticEvent; cause?: unknown }[] = [];
    configureErrorDiagnosticSink({
      report: (event, cause) => reports.push({ event, cause }),
    });
    invoke.mockRejectedValueOnce(raw);

    const error = await invokeDesktop('query_game_cards').catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(DesktopCommandError);
    expect(error).toMatchObject({ code: 'storage_failed', contractStatus: 'malformed' });
    expect(reports).toHaveLength(1);
    expect(reports[0]?.event).toEqual({
      source: 'desktop-command',
      operation: 'query_game_cards',
      code: 'storage_failed',
      contractStatus: 'malformed',
      severity: 'error',
    });
    expect(JSON.stringify(reports[0]?.event)).not.toContain('PRIVATE');
    expect(JSON.stringify((error as DesktopCommandError).dto)).not.toContain('PRIVATE');
  });
});
