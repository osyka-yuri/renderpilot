import { afterEach, describe, expect, it } from 'vitest';
import {
  configureErrorDiagnosticSink,
  projectDevelopmentCause,
  reportErrorDiagnostic,
  type ErrorDiagnosticEvent,
} from './index';

describe('error diagnostics', () => {
  afterEach(() => {
    configureErrorDiagnosticSink(null);
  });

  it('emits only the safe structured projection', () => {
    const received: { event: ErrorDiagnosticEvent; cause?: unknown }[] = [];
    const cause = new Error('PRIVATE C:/Users/name/token');
    configureErrorDiagnosticSink({
      report: (event, developmentCause) => {
        received.push({ event, cause: developmentCause });
      },
    });

    const event = reportErrorDiagnostic(
      {
        source: 'desktop-command',
        operation: 'scan_auto_libraries',
        code: 'storage_failed',
        contractStatus: 'known',
        severity: 'error',
      },
      cause,
    );

    expect(event).toEqual({
      source: 'desktop-command',
      operation: 'scan_auto_libraries',
      code: 'storage_failed',
      contractStatus: 'known',
      severity: 'error',
    });
    expect(JSON.stringify(event)).not.toContain('PRIVATE');
    expect(received[0]?.event).toBe(event);
    if (import.meta.env.DEV) {
      expect(received[0]?.cause).toBe(cause);
    } else {
      expect(received[0]?.cause).toBeUndefined();
    }
  });

  it('sanitizes arbitrary values before they reach a sink', () => {
    const received: ErrorDiagnosticEvent[] = [];
    configureErrorDiagnosticSink({
      report: (event) => {
        received.push(event);
      },
    });

    reportErrorDiagnostic({
      source: 'client-boundary',
      operation: 'path C:/Users/private',
      code: 'Error: secret token',
      contractStatus: 'malformed',
      severity: 'error',
      locale: '../secret',
    });

    expect(received).toEqual([
      {
        source: 'client-boundary',
        operation: 'unknown_operation',
        code: 'invalid_error_code',
        contractStatus: 'malformed',
        severity: 'error',
        locale: 'unknown_locale',
      },
    ]);
  });

  it('drops raw causes unconditionally on the production path', () => {
    const cause = new Error('PRIVATE C:/Users/name/token');

    expect(projectDevelopmentCause(cause, false)).toBeUndefined();
    expect(projectDevelopmentCause(cause, true)).toBe(cause);
  });

  it('never lets a failing diagnostic provider replace the application flow', () => {
    configureErrorDiagnosticSink({
      report: () => {
        throw new Error('telemetry unavailable');
      },
    });

    expect(() =>
      reportErrorDiagnostic({
        source: 'client-boundary',
        operation: 'safe_boundary',
        code: 'unexpected_client_error',
        contractStatus: 'known',
        severity: 'error',
      }),
    ).not.toThrow();
  });
});
