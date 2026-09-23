import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  configureErrorDiagnosticSink,
  reportErrorDiagnostic,
  type ErrorDiagnosticEvent,
} from './index';
import { ClientError, DesktopCommandError, reportClientError } from '@shared/errors';
import { createDesktopDiagnosticSink } from './desktop-bridge';

describe('desktop diagnostic bridge', () => {
  afterEach(() => {
    configureErrorDiagnosticSink(null);
    vi.restoreAllMocks();
  });

  it('preserves existing console diagnostics while projecting a safe file event', () => {
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const sent: Readonly<Record<string, string>>[] = [];
    configureErrorDiagnosticSink(
      createDesktopDiagnosticSink((event) => Promise.resolve(sent.push(event))),
    );
    const event: ErrorDiagnosticEvent = {
      source: 'i18n',
      operation: 'initialize_locale',
      code: 'i18n_locale_load_failed',
      contractStatus: 'known',
      severity: 'warning',
      locale: 'ru',
      mode: 'system',
    };

    reportErrorDiagnostic(event, new Error('local developer details'));

    expect(warning).toHaveBeenCalledOnce();
    expect(warning.mock.calls[0]?.[1]).toEqual(event);
    expect(sent).toEqual([
      {
        source: 'i18n',
        operation: 'initialize_locale',
        code: 'i18n_locale_load_failed',
        contractStatus: 'known',
        severity: 'warning',
        locale: 'ru',
        mode: 'system',
      },
    ]);
    expect(JSON.stringify(sent)).not.toContain('local developer details');
  });

  it('reduces client-boundary reports to a closed category', () => {
    const sent: Readonly<Record<string, string>>[] = [];
    configureErrorDiagnosticSink(
      createDesktopDiagnosticSink((event) => Promise.resolve(sent.push(event))),
    );

    reportErrorDiagnostic({
      source: 'client-boundary',
      operation: 'C:/Users/private/custom operation',
      code: 'private_path_in_error',
      contractStatus: 'malformed',
      severity: 'warning',
      locale: '../private',
    });

    expect(sent).toEqual([
      {
        source: 'client-boundary',
        operation: 'client_boundary',
        code: 'client_boundary',
        contractStatus: 'malformed',
        severity: 'warning',
      },
    ]);
    expect(JSON.stringify(sent)).not.toContain('private');
  });

  it('records known local ClientError values coarsely and excludes desktop command errors', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const sent: Readonly<Record<string, string>>[] = [];
    configureErrorDiagnosticSink(
      createDesktopDiagnosticSink((event) => Promise.resolve(sent.push(event))),
    );

    reportClientError(
      'C:/private operation',
      new ClientError('external_open_failed', new Error('private local cause')),
    );
    reportClientError('apply_swap', DesktopCommandError.fromDto({ code: 'storage_failed' }));
    reportErrorDiagnostic({
      source: 'client-boundary',
      operation: 'client_boundary',
      code: 'storage_failed',
      contractStatus: 'known',
      severity: 'error',
    });
    reportClientError('updater_check', new ClientError('updater_check_failed'), 'warning');

    expect(sent).toEqual([
      {
        source: 'client-boundary',
        operation: 'client_boundary',
        code: 'known_local_error',
        contractStatus: 'known',
        severity: 'error',
      },
      {
        source: 'client-boundary',
        operation: 'client_boundary',
        code: 'known_local_error',
        contractStatus: 'known',
        severity: 'warning',
      },
    ]);
    expect(JSON.stringify(sent)).not.toContain('external_open_failed');
    expect(JSON.stringify(sent)).not.toContain('private');
    expect(consoleError.mock.calls[0]?.[1]).toEqual(
      expect.objectContaining({
        source: 'client-boundary',
        operation: 'unknown_operation',
        code: 'external_open_failed',
      }),
    );
    expect(consoleError.mock.calls[1]?.[1]).toEqual(
      expect.objectContaining({ source: 'desktop-command', code: 'storage_failed' }),
    );
  });

  it('sends only validated i18n metadata and excludes raw causes', () => {
    const sent: Readonly<Record<string, string>>[] = [];
    configureErrorDiagnosticSink(
      createDesktopDiagnosticSink((event) => Promise.resolve(sent.push(event))),
    );

    reportErrorDiagnostic(
      {
        source: 'i18n',
        operation: 'initialize_locale',
        code: 'i18n_locale_load_failed',
        contractStatus: 'known',
        severity: 'warning',
        locale: 'ru',
        mode: 'system',
      },
      new Error('private C:/Users/person'),
    );

    expect(sent).toEqual([
      {
        source: 'i18n',
        operation: 'initialize_locale',
        code: 'i18n_locale_load_failed',
        contractStatus: 'known',
        severity: 'warning',
        locale: 'ru',
        mode: 'system',
      },
    ]);
  });

  it('drops desktop command duplicates and emits only the transport fallback', () => {
    const sent: Readonly<Record<string, string>>[] = [];
    configureErrorDiagnosticSink(
      createDesktopDiagnosticSink((event) => Promise.resolve(sent.push(event))),
    );

    reportErrorDiagnostic({
      source: 'desktop-command',
      operation: 'apply_swap',
      code: 'storage_failed',
      contractStatus: 'known',
      severity: 'error',
    });
    reportErrorDiagnostic({
      source: 'desktop-command',
      operation: 'apply_swap',
      code: 'desktop_transport_failed',
      contractStatus: 'malformed',
      severity: 'error',
    });

    expect(sent).toEqual([
      {
        source: 'desktop-command',
        operation: 'transport',
        code: 'desktop_transport_failed',
        contractStatus: 'malformed',
        severity: 'error',
      },
    ]);
  });

  it('ignores unsafe inputs and suppresses failed IPC without recursion', async () => {
    const send = vi.fn(() => Promise.reject(new Error('private transport cause')));
    configureErrorDiagnosticSink(createDesktopDiagnosticSink(send));

    reportErrorDiagnostic({
      source: 'i18n',
      operation: 'C:/private',
      code: 'i18n_locale_load_failed',
      contractStatus: 'known',
      severity: 'warning',
      locale: 'ru',
      mode: 'system',
    });
    reportErrorDiagnostic({
      source: 'client-boundary',
      operation: 'private',
      code: 'private',
      contractStatus: 'known',
      severity: 'warning',
    });
    reportErrorDiagnostic({
      source: 'client-boundary',
      operation: 'boundary',
      code: 'malformed_response',
      contractStatus: 'malformed',
      severity: 'warning',
    });
    reportErrorDiagnostic({
      source: 'client-boundary',
      operation: 'boundary',
      code: 'client_boundary',
      contractStatus: 'malformed',
      severity: 'fatal',
    } as unknown as ErrorDiagnosticEvent);

    await Promise.resolve();
    expect(send).toHaveBeenCalledOnce();
  });
});
