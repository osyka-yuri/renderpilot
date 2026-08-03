import { afterEach, describe, expect, it } from 'vitest';
import {
  configureErrorDiagnosticSink,
  reportErrorDiagnostic,
  type ErrorDiagnosticEvent,
} from '@shared/diagnostics';
import { ClientError, DesktopCommandError } from './model';
import { reportClientError, reportDesktopCommandError } from './report';

describe('error reporting boundaries', () => {
  afterEach(() => {
    configureErrorDiagnosticSink(null);
  });

  it('deduplicates the same object across client and desktop reporting boundaries', () => {
    const events: ErrorDiagnosticEvent[] = [];
    configureErrorDiagnosticSink({ report: (event) => events.push(event) });
    const error = DesktopCommandError.fromDto({ code: 'storage_failed' });

    reportDesktopCommandError('desktop_boundary', error);
    reportClientError('outer_boundary', error);

    expect(events).toEqual([
      expect.objectContaining({ operation: 'desktop_boundary', code: 'storage_failed' }),
    ]);
  });

  it('does not identity-deduplicate primitive errors', () => {
    const events: ErrorDiagnosticEvent[] = [];
    configureErrorDiagnosticSink({ report: (event) => events.push(event) });

    reportClientError('first_boundary', 'failure');
    reportClientError('second_boundary', 'failure');

    expect(events).toHaveLength(2);
  });

  it('leaves direct low-level reporting under caller control', () => {
    const events: ErrorDiagnosticEvent[] = [];
    configureErrorDiagnosticSink({ report: (event) => events.push(event) });
    const event = {
      source: 'client-boundary',
      operation: 'manual_boundary',
      code: 'unexpected_client_error',
      contractStatus: 'known',
      severity: 'error',
    } as const;

    reportErrorDiagnostic(event);
    reportErrorDiagnostic(event);

    expect(events).toHaveLength(2);
  });

  it('deduplicates typed client errors by identity, not by code', () => {
    const events: ErrorDiagnosticEvent[] = [];
    configureErrorDiagnosticSink({ report: (event) => events.push(event) });
    const first = new ClientError('update_all_step_failed');
    const second = new ClientError('update_all_step_failed');

    reportClientError('inner_boundary', first);
    reportClientError('outer_boundary', first);
    reportClientError('separate_boundary', second);

    expect(events).toHaveLength(2);
  });
});
