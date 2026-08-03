export type ErrorDiagnosticSource = 'desktop-command' | 'i18n' | 'client-boundary';
export type ErrorContractStatus = 'known' | 'unknown' | 'malformed';
export type ErrorSeverity = 'warning' | 'error';

export type ErrorDiagnosticEvent = Readonly<{
  source: ErrorDiagnosticSource;
  operation: string;
  code: string;
  contractStatus: ErrorContractStatus;
  severity: ErrorSeverity;
  locale?: string;
  mode?: string;
}>;

export type ErrorDiagnosticSink = {
  report(event: ErrorDiagnosticEvent, developmentCause?: unknown): void;
};

const SAFE_TOKEN = /^[A-Za-z0-9_.:-]{1,96}$/;

const defaultSink: ErrorDiagnosticSink = {
  report(event, developmentCause) {
    const write = event.severity === 'warning' ? console.warn : console.error;
    if (import.meta.env.DEV && developmentCause !== undefined) {
      write('[RenderPilot diagnostic]', event, developmentCause);
      return;
    }
    write('[RenderPilot diagnostic]', event);
  },
};

let sink: ErrorDiagnosticSink = defaultSink;

/** Installs a provider-neutral sink. Passing `null` restores the safe console adapter. */
export function configureErrorDiagnosticSink(next: ErrorDiagnosticSink | null): void {
  sink = next ?? defaultSink;
}

export function reportErrorDiagnostic(
  input: ErrorDiagnosticEvent,
  cause?: unknown,
): ErrorDiagnosticEvent {
  const event: ErrorDiagnosticEvent = Object.freeze({
    source: input.source,
    operation: safeToken(input.operation, 'unknown_operation'),
    code: safeToken(input.code, 'invalid_error_code'),
    contractStatus: input.contractStatus,
    severity: input.severity,
    ...(input.locale === undefined ? {} : { locale: safeToken(input.locale, 'unknown_locale') }),
    ...(input.mode === undefined ? {} : { mode: safeToken(input.mode, 'unknown_mode') }),
  });

  try {
    sink.report(event, projectDevelopmentCause(cause, import.meta.env.DEV));
  } catch {
    // Diagnostics must never replace or mask the application error being reported.
  }
  return event;
}

/** Pure release guard kept separate so production cause suppression is directly testable. */
export function projectDevelopmentCause(cause: unknown, isDevelopment: boolean): unknown {
  return isDevelopment ? cause : undefined;
}

function safeToken(value: string, fallback: string): string {
  return SAFE_TOKEN.test(value) ? value : fallback;
}
