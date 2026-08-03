import { reportErrorDiagnostic, type ErrorContractStatus } from '@shared/diagnostics';
import { t, translateExternalMessage } from '@shared/i18n';
import { isPlainObject } from '@shared/validation';
import { ADD_GAME_WARNING_CONTRACT, type AddGameWarningCode } from './generated/add-game-warnings';

type WarningParameters = Readonly<Record<string, string | number>>;
type EmptyWarningParameters = Readonly<Record<string, never>>;

export type NormalizedAddGameWarning =
  | Readonly<{
      contractStatus: 'known';
      code: AddGameWarningCode;
      parameters: WarningParameters;
    }>
  | Readonly<{
      contractStatus: Exclude<ErrorContractStatus, 'known'>;
      code: string;
      parameters: EmptyWarningParameters;
    }>;

const EMPTY_PARAMETERS: EmptyWarningParameters = Object.freeze({});

/** Formats a structured backend warning without ever presenting backend prose. */
export function formatAddGameWarning(warning: NormalizedAddGameWarning): string {
  return presentAddGameWarning(warning).message;
}

export type PresentedAddGameWarning = Readonly<{
  code: string;
  message: string;
  contractStatus: ErrorContractStatus;
}>;

/** Pure warning projection. Diagnostics belong to the command-response boundary. */
export function presentAddGameWarning(warning: NormalizedAddGameWarning): PresentedAddGameWarning {
  if (warning.contractStatus !== 'known') {
    return {
      code: warning.code,
      message: t('addGame.warning.unknown'),
      contractStatus: warning.contractStatus,
    };
  }

  const spec = ADD_GAME_WARNING_CONTRACT[warning.code];
  return {
    code: warning.code,
    message: translateExternalMessage({
      key: spec.messageKey,
      fallback: t('addGame.warning.unknown'),
      ...(Object.keys(warning.parameters).length === 0 ? {} : { params: warning.parameters }),
    }),
    contractStatus: 'known',
  };
}

/** Reports each malformed/unknown warning once when a desktop response enters the feature. */
export function normalizeAddGameWarnings(
  warnings: readonly unknown[],
  operation: 'inspect_game_install' | 'add_game',
): NormalizedAddGameWarning[] {
  return warnings.map((warning) => {
    const normalized = normalizeAddGameWarning(warning);
    if (normalized.contractStatus !== 'known') {
      reportErrorDiagnostic({
        source: 'client-boundary',
        operation,
        code: normalized.code,
        contractStatus: normalized.contractStatus,
        severity: 'warning',
      });
    }
    return normalized;
  });
}

function normalizeAddGameWarning(value: unknown): NormalizedAddGameWarning {
  if (!isPlainObject(value) || !isSafeMachineCode(value.code)) {
    return malformedWarning('invalid_add_game_warning');
  }

  const code = value.code;
  const allowedKeys = new Set(['code', 'parameters']);
  if (Object.keys(value).some((key) => !allowedKeys.has(key))) {
    return malformedWarning(code);
  }

  if (!isAddGameWarningCode(code)) {
    return 'parameters' in value && !isPlainObject(value.parameters)
      ? malformedWarning(code)
      : unknownWarning(code);
  }

  const parameters = validateParameters(
    value.parameters,
    ADD_GAME_WARNING_CONTRACT[code].parameters,
  );
  return parameters === null
    ? malformedWarning(code)
    : { contractStatus: 'known', code, parameters: Object.freeze(parameters) };
}

function unknownWarning(code: string): NormalizedAddGameWarning {
  return { contractStatus: 'unknown', code, parameters: EMPTY_PARAMETERS };
}

function malformedWarning(code: string): NormalizedAddGameWarning {
  return { contractStatus: 'malformed', code, parameters: EMPTY_PARAMETERS };
}

function isAddGameWarningCode(code: string): code is AddGameWarningCode {
  return Object.prototype.hasOwnProperty.call(ADD_GAME_WARNING_CONTRACT, code);
}

function validateParameters(
  input: unknown,
  contract: Readonly<Record<string, 'positive_integer' | 'non_blank_string'>>,
): Record<string, string | number> | null {
  const source = input ?? {};
  if (!isPlainObject(source)) {
    return null;
  }
  const expectedNames = Object.keys(contract);
  if (
    Object.keys(source).length !== expectedNames.length ||
    Object.keys(source).some((name) => !Object.prototype.hasOwnProperty.call(contract, name))
  ) {
    return null;
  }

  const output: Record<string, string | number> = {};
  for (const name of expectedNames) {
    const value = source[name];
    switch (contract[name]) {
      case 'positive_integer':
        if (!Number.isSafeInteger(value) || typeof value !== 'number' || value <= 0) {
          return null;
        }
        output[name] = value;
        break;
      case 'non_blank_string': {
        if (typeof value !== 'string') {
          return null;
        }
        const normalized = value.trim();
        if (
          normalized.length === 0 ||
          normalized.length > 4096 ||
          containsControlCharacter(normalized)
        ) {
          return null;
        }
        output[name] = normalized;
        break;
      }
    }
  }
  return output;
}

function isSafeMachineCode(value: unknown): value is string {
  return typeof value === 'string' && /^[a-z][a-z0-9_]{0,63}$/.test(value);
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
