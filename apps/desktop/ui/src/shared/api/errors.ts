import type { CommandErrorDto, CommandErrorSeverity, SuggestedActionDto } from './types';
import { isString, isNonEmptyString, isRecord, isErrorLike, isFunction } from '@shared/validation';
import { translateKey } from '@shared/i18n';

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
 * Extracts a concise error description, explicitly omitting any appended suggested-action suffixes.
 * This format is optimal for space-constrained UI elements such as compact banners or inline validation hints.
 *
 * For scenarios requiring comprehensive user context—including actionable remediation steps—utilize
 * {@link describeCommandError} instead.
 */
export function describeCommandErrorBrief(error: unknown): string {
  const dto = normalizeCommandError(error).dto;

  return translateKey(dto.messageKey, dto.details);
}

/**
 * Synthesizes the most actionable, single-line technical description of a given command error.
 *
 * The resolution strategy prioritizes `debugDetails` (which surfaces the exact backend failure
 * reason, exclusively available in development builds) before gracefully falling back to `details`
 * (the generic, localized text guaranteed to be present). This is highly recommended for error
 * toasts or diagnostic logs where actionable clarity is required over generic "operation failed" messaging.
 */
export function describeCommandErrorTechnical(error: unknown): string {
  const dto = normalizeCommandError(error).dto;
  const debug = dto.debugDetails;
  if (typeof debug === 'string' && debug.trim().length > 0) {
    return debug.trim();
  }
  return dto.details;
}

function parseCommandErrorDto(value: unknown): CommandErrorDto | null {
  if (!isRecord(value)) {
    return null;
  }

  const { code, severity, messageKey, details, suggestedActions, debugDetails } = value;

  if (
    !isNonEmptyString(code) ||
    !isCommandErrorSeverity(severity) ||
    !isNonEmptyString(messageKey) ||
    !isString(details) ||
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
    debugDetails: isString(debugDetails) ? debugDetails : undefined,
  });
}

function normalizeCommandErrorDto(dto: CommandErrorDto): CommandErrorDto {
  if (!isRecord(dto)) {
    return createFallbackDto(UNKNOWN_ERROR_DETAILS);
  }

  const debugDetails =
    isString(dto.debugDetails) && dto.debugDetails.trim().length > 0
      ? dto.debugDetails.trim()
      : undefined;

  return {
    code: normalizeNonEmptyString(dto.code, FALLBACK_CODE),
    severity: normalizeSeverity(dto.severity),
    messageKey: normalizeNonEmptyString(dto.messageKey, FALLBACK_MESSAGE_KEY),
    details: normalizeDetails(dto.details),
    suggestedActions: normalizeSuggestedActions(dto.suggestedActions),
    debugDetails,
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
  const message = translateKey(error.messageKey, normalizeDetails(error.details));
  const actions = error.suggestedActions
    .map((action) => translateKey(action.key, action.text))
    .filter(isNonEmptyString);

  if (actions.length === 0) {
    return message;
  }

  return [message, ...actions].join(' ');
}

function getErrorDetails(error: unknown): string {
  if (isErrorLike(error)) {
    return firstNonEmptyString(error.message, error.name) ?? UNKNOWN_ERROR_DETAILS;
  }

  if (isString(error)) {
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
  if (!isString(value)) {
    return UNKNOWN_ERROR_DETAILS;
  }

  return value.trim() || UNKNOWN_ERROR_DETAILS;
}

function normalizeSuggestedActions(value: unknown): SuggestedActionDto[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map(normalizeSuggestedAction)
    .filter((action): action is SuggestedActionDto => action !== null);
}

function normalizeSuggestedAction(value: unknown): SuggestedActionDto | null {
  if (!isRecord(value)) {
    return null;
  }

  const key = isString(value.key) ? value.key.trim() : '';
  const text = isString(value.text) ? value.text.trim() : '';

  if (key.length === 0 && text.length === 0) {
    return null;
  }

  return { key, text };
}

function normalizeNonEmptyString(value: unknown, fallback: string): string {
  return isNonEmptyString(value) ? value.trim() : fallback;
}

function firstNonEmptyString(...values: unknown[]): string | undefined {
  for (const value of values) {
    if (!isString(value)) {
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

function attachCause(error: Error, cause: unknown): void {
  (error as ErrorWithCause).cause = cause;
}

function captureStackTrace(error: Error, constructor: object): void {
  const capture = (Error as ErrorConstructorWithStackTrace).captureStackTrace;
  if (isFunction(capture)) {
    capture(error, constructor);
  }
}
