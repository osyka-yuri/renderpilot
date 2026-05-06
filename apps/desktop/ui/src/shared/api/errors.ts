import type { CommandErrorDto } from './types';

const FALLBACK_MESSAGE_KEY = 'errors.command_failed';

export class DesktopCommandError extends Error {
  readonly dto: CommandErrorDto;

  constructor(dto: CommandErrorDto) {
    super(formatCommandError(dto));
    this.name = 'DesktopCommandError';
    this.dto = dto;
  }
}

export function normalizeCommandError(error: unknown): DesktopCommandError {
  if (error instanceof DesktopCommandError) {
    return error;
  }

  if (isCommandErrorDto(error)) {
    return new DesktopCommandError(error);
  }

  if (error instanceof Error) {
    return new DesktopCommandError({
      code: 'command_failed',
      severity: 'error',
      message_key: FALLBACK_MESSAGE_KEY,
      details: error.message,
      suggested_actions: [],
    });
  }

  return new DesktopCommandError({
    code: 'command_failed',
    severity: 'error',
    message_key: FALLBACK_MESSAGE_KEY,
    details: String(error),
    suggested_actions: [],
  });
}

export function describeCommandError(error: unknown): string {
  return normalizeCommandError(error).message;
}

function isCommandErrorDto(value: unknown): value is CommandErrorDto {
  if (!value || typeof value !== 'object') {
    return false;
  }

  const candidate = value as Partial<CommandErrorDto>;
  return (
    typeof candidate.code === 'string' &&
    typeof candidate.severity === 'string' &&
    typeof candidate.message_key === 'string' &&
    typeof candidate.details === 'string' &&
    Array.isArray(candidate.suggested_actions) &&
    candidate.suggested_actions.every((action) => typeof action === 'string')
  );
}

function formatCommandError(error: CommandErrorDto): string {
  if (error.suggested_actions.length === 0) {
    return error.details;
  }

  return `${error.details} ${error.suggested_actions.join(' ')}`;
}