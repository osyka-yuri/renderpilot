import type { CommandErrorDto } from './types';

const FALLBACK_CODE = 'command_failed';
const FALLBACK_MESSAGE_KEY = 'errors.command_failed';
const FALLBACK_SEVERITY: CommandErrorDto['severity'] = 'error';
const UNKNOWN_ERROR_DETAILS = 'Unknown command error';

export class DesktopCommandError extends Error {
  readonly dto: CommandErrorDto;
  readonly originalError?: unknown;

  constructor(dto: CommandErrorDto, originalError?: unknown) {
    const normalizedDto = cloneCommandErrorDto(dto);

    super(formatCommandError(normalizedDto));

    this.name = 'DesktopCommandError';
    this.dto = normalizedDto;
    this.originalError = originalError;

    // Helps when targeting older JS runtimes.
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export function normalizeCommandError(error: unknown): DesktopCommandError {
  if (error instanceof DesktopCommandError) {
    return error;
  }

  if (isCommandErrorDto(error)) {
    return new DesktopCommandError(error, error);
  }

  return new DesktopCommandError(createFallbackDto(getErrorDetails(error)), error);
}

export function describeCommandError(error: unknown): string {
  return normalizeCommandError(error).message;
}

function createFallbackDto(details: string): CommandErrorDto {
  return {
    code: FALLBACK_CODE,
    severity: FALLBACK_SEVERITY,
    messageKey: FALLBACK_MESSAGE_KEY,
    details,
    suggestedActions: [],
  };
}

function isCommandErrorDto(value: unknown): value is CommandErrorDto {
  if (!isRecord(value)) {
    return false;
  }

  return (
    isNonEmptyString(value.code) &&
    isNonEmptyString(value.severity) &&
    isNonEmptyString(value.messageKey) &&
    typeof value.details === 'string' &&
    Array.isArray(value.suggestedActions) &&
    value.suggestedActions.every(isString)
  );
}

function formatCommandError(error: CommandErrorDto): string {
  const details = error.details.trim() || UNKNOWN_ERROR_DETAILS;

  const suggestedActions = error.suggestedActions.map((action) => action.trim()).filter(Boolean);

  if (suggestedActions.length === 0) {
    return details;
  }

  return [details, ...suggestedActions].join(' ');
}

function getErrorDetails(error: unknown): string {
  if (error instanceof Error) {
    return error.message.trim() || error.name || UNKNOWN_ERROR_DETAILS;
  }

  if (typeof error === 'string') {
    return error.trim() || UNKNOWN_ERROR_DETAILS;
  }

  return stringifyUnknown(error);
}

function stringifyUnknown(value: unknown): string {
  try {
    if (value === null) {
      return 'null';
    }

    if (value === undefined) {
      return 'undefined';
    }

    const json = JSON.stringify(value);

    if (json) {
      return json;
    }
  } catch {
    // Fall through to String(value).
  }

  try {
    return String(value);
  } catch {
    return UNKNOWN_ERROR_DETAILS;
  }
}

function cloneCommandErrorDto(dto: CommandErrorDto): CommandErrorDto {
  return {
    ...dto,
    suggestedActions: [...dto.suggestedActions],
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isString(value: unknown): value is string {
  return typeof value === 'string';
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0;
}
