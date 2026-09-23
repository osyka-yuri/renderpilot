import { invoke } from '@tauri-apps/api/core';

import { isDesktopPreviewMode } from '@shared/api-preview';
import { isLocalErrorCode } from '@shared/error-contract';
import {
  configureErrorDiagnosticSink,
  consoleErrorDiagnosticSink,
  type ErrorContractStatus,
  type ErrorDiagnosticEvent,
  type ErrorSeverity,
  type ErrorDiagnosticSink,
} from './core';

type SendDiagnostic = (event: Readonly<Record<string, string>>) => Promise<unknown>;

export function createDesktopDiagnosticSink(send: SendDiagnostic): ErrorDiagnosticSink {
  return {
    report(event, developmentCause) {
      try {
        consoleErrorDiagnosticSink.report(event, developmentCause);
      } catch {
        // The file projection remains independent from console behavior.
      }
      const projection = projectDesktopDiagnostic(event);
      if (projection === null) {
        return;
      }
      try {
        void send(projection).catch(() => {
          // The diagnostic bridge must never report its own transport failure.
        });
      } catch {
        // Synchronous bridge failures are also best-effort.
      }
    },
  };
}

export function installDesktopDiagnosticSink(): void {
  if (isDesktopPreviewMode()) {
    return;
  }
  configureErrorDiagnosticSink(
    createDesktopDiagnosticSink((event) => invoke('record_frontend_diagnostic', { event })),
  );
}

function projectDesktopDiagnostic(
  event: ErrorDiagnosticEvent,
): Readonly<Record<string, string>> | null {
  if (event.source === 'i18n') {
    if (
      event.operation !== 'initialize_locale' &&
      event.operation !== 'switch_locale' &&
      event.operation !== 'system_language_change'
    ) {
      return null;
    }
    if (
      event.code !== 'i18n_locale_load_failed' ||
      event.contractStatus !== 'known' ||
      event.severity !== 'warning' ||
      !isLocale(event.locale) ||
      !isLocaleMode(event.mode)
    ) {
      return null;
    }
    return {
      source: 'i18n',
      operation: event.operation,
      code: 'i18n_locale_load_failed',
      contractStatus: 'known',
      severity: 'warning',
      locale: event.locale,
      mode: event.mode,
    };
  }

  if (event.source === 'client-boundary') {
    if (!isDiagnosticSeverity(event.severity) || !isContractStatus(event.contractStatus)) {
      return null;
    }
    let code: 'client_boundary' | 'known_local_error';
    if (event.contractStatus === 'known') {
      if (!isLocalErrorCode(event.code)) {
        return null;
      }
      code = 'known_local_error';
    } else {
      code = 'client_boundary';
    }
    return {
      source: 'client-boundary',
      operation: 'client_boundary',
      code,
      contractStatus: event.contractStatus,
      severity: event.severity,
    };
  }

  if (
    event.code === 'desktop_transport_failed' &&
    event.contractStatus === 'malformed' &&
    event.severity === 'error'
  ) {
    return {
      source: 'desktop-command',
      operation: 'transport',
      code: 'desktop_transport_failed',
      contractStatus: 'malformed',
      severity: 'error',
    };
  }
  // Ordinary command errors are already recorded at the Rust command boundary.
  return null;
}

function isLocale(value: string | undefined): value is string {
  return (
    value === 'en' ||
    value === 'de' ||
    value === 'es' ||
    value === 'fr' ||
    value === 'ja' ||
    value === 'pt-BR' ||
    value === 'ru' ||
    value === 'zh-Hans' ||
    value === 'zh-Hant'
  );
}

function isLocaleMode(value: string | undefined): value is string {
  return value === 'system' || isLocale(value);
}

function isDiagnosticSeverity(value: unknown): value is ErrorSeverity {
  return value === 'warning' || value === 'error';
}

function isContractStatus(value: unknown): value is ErrorContractStatus {
  return value === 'known' || value === 'unknown' || value === 'malformed';
}
