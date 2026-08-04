import type { ErrorContractStatus, ErrorSeverity } from '@shared/diagnostics';
import { isNonEmptyString, isPlainObject, isRecord } from '@shared/validation';
import { COMMAND_ERROR_CONTRACT, type CommandErrorCode } from './generated/desktop-command-errors';
import { isLocalErrorCode, LOCAL_ERROR_CONTRACT, type LocalErrorCode } from './local-contract';

export type CommandErrorDto = Readonly<{
  code: string;
  reasonCode?: string;
  recoveryBundlePath?: string;
}>;

export type CodedError = {
  readonly code: string;
  readonly cause?: unknown;
};

export class ClientError extends Error implements CodedError {
  readonly code: LocalErrorCode;

  constructor(code: LocalErrorCode, cause?: unknown) {
    super(code, { cause });
    this.name = 'ClientError';
    this.code = code;
  }
}

export class DesktopCommandError extends Error implements CodedError {
  readonly code: string;
  readonly dto: CommandErrorDto;
  readonly contractStatus: ErrorContractStatus;
  readonly severity: ErrorSeverity;

  private constructor(parsed: ParsedCommandError, cause?: unknown) {
    super(parsed.dto.code, { cause });
    this.name = 'DesktopCommandError';
    this.code = parsed.dto.code;
    this.dto = Object.freeze(parsed.dto);
    this.contractStatus = parsed.contractStatus;
    this.severity = isCommandErrorCode(parsed.dto.code)
      ? COMMAND_ERROR_CONTRACT[parsed.dto.code].severity
      : 'error';
  }

  static fromDto(dto: CommandErrorDto, cause?: unknown): DesktopCommandError {
    return DesktopCommandError.fromTransportValue(dto, cause);
  }

  static fromTransportValue(value: unknown, cause?: unknown): DesktopCommandError {
    const parsed = parseCommandErrorDto(value);
    return new DesktopCommandError(
      parsed ?? {
        dto: { code: 'desktop_transport_failed' },
        contractStatus: 'malformed',
      },
      cause,
    );
  }
}

export function normalizeDesktopCommandError(error: unknown): DesktopCommandError {
  if (error instanceof DesktopCommandError) {
    return error;
  }

  return DesktopCommandError.fromTransportValue(error, error);
}

export function getErrorCode(error: unknown): string {
  if (error instanceof ClientError || error instanceof DesktopCommandError) {
    return error.code;
  }
  if (isRecord(error) && isSafeMachineCode(error.code)) {
    return error.code;
  }
  return 'unexpected_client_error';
}

export function getErrorContractStatus(error: unknown): ErrorContractStatus {
  if (error instanceof DesktopCommandError) {
    return error.contractStatus;
  }
  if (error instanceof ClientError) {
    return 'known';
  }
  if (isRecord(error) && isLocalErrorCodeValue(error.code)) {
    return 'known';
  }
  const parsed = parseCommandErrorDto(error);
  if (parsed !== null) {
    return parsed.contractStatus;
  }
  const code = getErrorCode(error);
  return code === 'unexpected_client_error' ? 'malformed' : 'unknown';
}

export function getErrorSeverity(error: unknown): ErrorSeverity {
  if (error instanceof DesktopCommandError) {
    return error.severity;
  }
  const code = getErrorCode(error);
  if (isCommandErrorCode(code)) {
    return COMMAND_ERROR_CONTRACT[code].severity;
  }
  if (isLocalErrorCode(code)) {
    return LOCAL_ERROR_CONTRACT[code].severity;
  }
  return 'error';
}

export function isCommandErrorCode(code: string): code is CommandErrorCode {
  return Object.hasOwn(COMMAND_ERROR_CONTRACT, code);
}

type ParsedCommandError = Readonly<{
  dto: CommandErrorDto;
  contractStatus: ErrorContractStatus;
}>;

function parseCommandErrorDto(value: unknown): ParsedCommandError | null {
  if (!isPlainObject(value) || !isSafeMachineCode(value.code)) {
    return null;
  }

  const code = value.code;
  if (!isCommandErrorCode(code)) {
    return {
      dto: { code },
      contractStatus: Object.keys(value).every((key) => key === 'code') ? 'unknown' : 'malformed',
    };
  }

  const spec = COMMAND_ERROR_CONTRACT[code];
  const allowedKeys = new Set(['code']);
  let malformed = false;
  let reasonCode: string | undefined;
  let recoveryBundlePath: string | undefined;

  if ('reasonCode' in value) {
    allowedKeys.add('reasonCode');
    if (
      isNonEmptyString(value.reasonCode) &&
      spec.reasonCodes.some((candidate) => candidate === value.reasonCode)
    ) {
      reasonCode = value.reasonCode.trim();
    } else {
      malformed = true;
    }
  }

  if ('recoveryBundlePath' in value) {
    allowedKeys.add('recoveryBundlePath');
    const normalizedPath =
      typeof value.recoveryBundlePath === 'string' ? value.recoveryBundlePath.trim() : '';
    if (spec.recoveryBundlePath && isSafeStructuredText(normalizedPath)) {
      recoveryBundlePath = normalizedPath;
    } else {
      malformed = true;
    }
  }

  if (Object.keys(value).some((key) => !allowedKeys.has(key))) {
    malformed = true;
  }

  return {
    dto: {
      code,
      ...(reasonCode === undefined ? {} : { reasonCode }),
      ...(recoveryBundlePath === undefined ? {} : { recoveryBundlePath }),
    },
    contractStatus: malformed ? 'malformed' : 'known',
  };
}

function isSafeMachineCode(value: unknown): value is string {
  return typeof value === 'string' && /^[a-z][a-z0-9_]{0,63}$/.test(value);
}

function isLocalErrorCodeValue(value: unknown): value is LocalErrorCode {
  return typeof value === 'string' && isLocalErrorCode(value);
}

function isSafeStructuredText(value: string): boolean {
  return value.length > 0 && value.length <= 4096 && !containsControlCharacter(value);
}

function containsControlCharacter(value: string): boolean {
  for (const character of value) {
    const codePoint = character.codePointAt(0) ?? 0;
    if (codePoint <= 0x1f || codePoint === 0x7f) {
      return true;
    }
  }
  return false;
}
