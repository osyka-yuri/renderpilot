import type { CommandErrorDto, CommandErrorSeverity } from './types';

const FALLBACK_CODE = 'command_failed';
const FALLBACK_MESSAGE_KEY = 'errors.command_failed';
const FALLBACK_SEVERITY: CommandErrorSeverity = 'error';
const UNKNOWN_ERROR_DETAILS = 'Unknown command error';
const ERROR_NAME = 'DesktopCommandError';

const COMMAND_ERROR_SEVERITIES = new Set<CommandErrorSeverity>(['warning', 'error']);

type ErrorWithCause = Error & { cause?: unknown };

type ErrorConstructorWithStackTrace = ErrorConstructor & {
  captureStackTrace?: (targetObject: object, constructorOpt?: object) => void;
};

export class DesktopCommandError extends Error {
  readonly dto: CommandErrorDto;
  readonly originalError?: unknown;

  constructor(dto: CommandErrorDto, originalError?: unknown) {
    const normalizedDto = normalizeCommandErrorDto(dto);

    super(formatCommandError(normalizedDto));

    this.name = ERROR_NAME;
    this.dto = normalizedDto;

    if (originalError !== undefined) {
      this.originalError = originalError;
      attachCause(this, originalError);
    }

    // Helps when targeting older JS runtimes.
    Object.setPrototypeOf(this, new.target.prototype);

    captureStackTrace(this, new.target);
  }
}

export function normalizeCommandError(error: unknown): DesktopCommandError {
  if (error instanceof DesktopCommandError) {
    return error;
  }

  const dto = parseCommandErrorDto(error);

  if (dto !== null) {
    return new DesktopCommandError(dto, error);
  }

  return new DesktopCommandError(createFallbackDto(getErrorDetails(error)), error);
}

export function describeCommandError(error: unknown): string {
  return normalizeCommandError(error).message;
}

/**
 * Short error details only, without suggested-actions suffix.
 * Prefer for compact banners and inline hints.
 *
 * Use {@link describeCommandError} when the user should see the full message,
 * including suggested actions.
 */
export function describeCommandErrorBrief(error: unknown): string {
  return normalizeCommandError(error).dto.details;
}

function parseCommandErrorDto(value: unknown): CommandErrorDto | null {
  if (!isRecord(value)) {
    return null;
  }

  const { code, severity, messageKey, details, suggestedActions } = value;

  if (
    !isNonEmptyString(code) ||
    !isCommandErrorSeverity(severity) ||
    !isNonEmptyString(messageKey) ||
    typeof details !== 'string' ||
    !Array.isArray(suggestedActions)
  ) {
    return null;
  }

  const normalizedSuggestedActions = normalizeSuggestedActions(suggestedActions);

  return normalizeCommandErrorDto({
    code,
    severity,
    messageKey,
    details,
    suggestedActions: normalizedSuggestedActions,
  });
}

function normalizeCommandErrorDto(dto: CommandErrorDto): CommandErrorDto {
  if (!isRecord(dto)) {
    return createFallbackDto(UNKNOWN_ERROR_DETAILS);
  }

  return {
    code: normalizeNonEmptyString(dto.code, FALLBACK_CODE),
    severity: normalizeSeverity(dto.severity),
    messageKey: normalizeNonEmptyString(dto.messageKey, FALLBACK_MESSAGE_KEY),
    details: normalizeDetails(dto.details),
    suggestedActions: normalizeSuggestedActions(dto.suggestedActions),
  };
}

function createFallbackDto(details: string): CommandErrorDto {
  return {
    code: FALLBACK_CODE,
    severity: FALLBACK_SEVERITY,
    messageKey: FALLBACK_MESSAGE_KEY,
    details: normalizeDetails(details),
    suggestedActions: [],
  };
}

function formatCommandError(error: CommandErrorDto): string {
  const details = normalizeDetails(error.details);
  const suggestedActions = normalizeSuggestedActions(error.suggestedActions);

  if (suggestedActions.length === 0) {
    return details;
  }

  return [details, ...suggestedActions].join(' ');
}

function getErrorDetails(error: unknown): string {
  if (isErrorLike(error)) {
    return firstNonEmptyString(error.message, error.name) ?? UNKNOWN_ERROR_DETAILS;
  }

  if (typeof error === 'string') {
    return normalizeDetails(error);
  }

  return stringifyUnknown(error);
}

function stringifyUnknown(value: unknown): string {
  switch (typeof value) {
    case 'undefined':
      return 'undefined';

    case 'string':
      return normalizeDetails(value);

    case 'number':
    case 'boolean':
    case 'bigint':
    case 'symbol':
      return String(value);

    case 'function':
      return safeString(value);

    case 'object':
      if (value === null) {
        return 'null';
      }

      return stringifyObject(value);
  }
}

function stringifyObject(value: object): string {
  try {
    const json = JSON.stringify(value);

    if (isNonEmptyString(json)) {
      return json;
    }
  } catch {
    // Fall through to String(value).
  }

  return safeString(value);
}

function safeString(value: unknown): string {
  try {
    const result = String(value).trim();

    return result.length > 0 ? result : UNKNOWN_ERROR_DETAILS;
  } catch {
    return UNKNOWN_ERROR_DETAILS;
  }
}

function normalizeSeverity(value: unknown): CommandErrorSeverity {
  return isCommandErrorSeverity(value) ? value : FALLBACK_SEVERITY;
}

function normalizeDetails(value: unknown): string {
  if (typeof value !== 'string') {
    return UNKNOWN_ERROR_DETAILS;
  }

  return value.trim() || UNKNOWN_ERROR_DETAILS;
}

function normalizeSuggestedActions(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value.map(normalizeSuggestedAction).filter(isNonEmptyString);
}

function normalizeSuggestedAction(value: unknown): string {
  return typeof value === 'string' ? value.trim() : '';
}

function normalizeNonEmptyString(value: unknown, fallback: string): string {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : fallback;
}

function firstNonEmptyString(...values: unknown[]): string | undefined {
  for (const value of values) {
    if (typeof value !== 'string') {
      continue;
    }

    const normalized = value.trim();

    if (normalized.length > 0) {
      return normalized;
    }
  }

  return undefined;
}

function isCommandErrorSeverity(value: unknown): value is CommandErrorSeverity {
  return typeof value === 'string' && COMMAND_ERROR_SEVERITIES.has(value as CommandErrorSeverity);
}

function isErrorLike(value: unknown): value is { message?: unknown; name?: unknown } {
  return isRecord(value) && (typeof value.message === 'string' || typeof value.name === 'string');
}

function attachCause(error: Error, cause: unknown): void {
  (error as ErrorWithCause).cause = cause;
}

function captureStackTrace(error: Error, constructor: object): void {
  const capture = (Error as ErrorConstructorWithStackTrace).captureStackTrace;
  if (typeof capture === 'function') {
    capture(error, constructor);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0;
}
