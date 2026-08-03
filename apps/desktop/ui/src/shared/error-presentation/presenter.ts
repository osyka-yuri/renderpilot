import type { ErrorContractStatus, ErrorSeverity } from '@shared/diagnostics';
import {
  COMMAND_ERROR_CONTRACT,
  DesktopCommandError,
  getErrorCode,
  getErrorContractStatus,
  getErrorSeverity,
  isCommandErrorCode,
  isLocalErrorCode,
  LOCAL_ERROR_CONTRACT,
  normalizeDesktopCommandError,
  SUGGESTED_ACTION_CONTRACT,
  type SuggestedActionCode,
} from '@shared/errors';
import { t, translateExternalMessage } from '@shared/i18n';

export type PresentedError = Readonly<{
  code: string;
  severity: ErrorSeverity;
  message: string;
  suggestedActions: readonly Readonly<{ code: SuggestedActionCode; label: string }>[];
  reasonCode?: string;
  recoveryBundlePath?: string;
  contractStatus: ErrorContractStatus;
}>;

export function presentError(error: unknown): PresentedError {
  const normalized = normalizeForPresentation(error);
  const code = getErrorCode(normalized);

  if (isCommandErrorCode(code)) {
    const spec = COMMAND_ERROR_CONTRACT[code];
    const dto = normalized instanceof DesktopCommandError ? normalized.dto : { code };
    return {
      code,
      severity: spec.severity,
      message: translateExternalMessage({
        key: spec.messageKey,
        fallback: t('error.unexpectedClient'),
      }),
      suggestedActions: presentActions(spec.actions),
      ...(dto.reasonCode === undefined ? {} : { reasonCode: dto.reasonCode }),
      ...(dto.recoveryBundlePath === undefined
        ? {}
        : { recoveryBundlePath: dto.recoveryBundlePath }),
      contractStatus: getErrorContractStatus(normalized),
    };
  }

  if (isLocalErrorCode(code)) {
    const spec = LOCAL_ERROR_CONTRACT[code];
    return {
      code,
      severity: spec.severity,
      message: t(spec.messageKey),
      suggestedActions: presentActions('actions' in spec ? spec.actions : []),
      contractStatus: getErrorContractStatus(normalized),
    };
  }

  return {
    code,
    severity: getErrorSeverity(normalized),
    message: t('error.unexpectedClient'),
    suggestedActions: [],
    contractStatus: getErrorContractStatus(normalized),
  };
}

export function formatPresentedError(
  error: unknown,
  options: Readonly<{ includeActions?: boolean }> = {},
): string {
  const presented = presentError(error);
  if (options.includeActions === false || presented.suggestedActions.length === 0) {
    return presented.message;
  }
  return [presented.message, ...presented.suggestedActions.map(({ label }) => label)]
    .filter((value) => value.length > 0)
    .join(' ');
}

function presentActions(actionCodes: readonly SuggestedActionCode[]) {
  return actionCodes.map((actionCode) => ({
    code: actionCode,
    label: translateExternalMessage({
      key: SUGGESTED_ACTION_CONTRACT[actionCode].messageKey,
      fallback: '',
    }),
  }));
}

function normalizeForPresentation(error: unknown): unknown {
  if (error instanceof DesktopCommandError) {
    return error;
  }
  const code = getErrorCode(error);
  if (isCommandErrorCode(code)) {
    return normalizeDesktopCommandError(error);
  }
  return error;
}
