import { reportErrorDiagnostic } from '@shared/diagnostics';
import {
  DesktopCommandError,
  getErrorCode,
  getErrorContractStatus,
  getErrorSeverity,
} from './model';

const reportedErrorObjects = new WeakSet();

export function reportDesktopCommandError(
  operation: string,
  error: DesktopCommandError,
  cause: unknown = error.cause,
): void {
  if (!claimDiagnostic(error)) {
    return;
  }
  reportErrorDiagnostic(
    {
      source: 'desktop-command',
      operation,
      code: error.code,
      contractStatus: error.contractStatus,
      severity: error.severity,
    },
    cause,
  );
}

export function reportClientError(
  operation: string,
  error: unknown,
  severity = getErrorSeverity(error),
): void {
  if (!claimDiagnostic(error)) {
    return;
  }
  const source = error instanceof DesktopCommandError ? 'desktop-command' : 'client-boundary';
  reportErrorDiagnostic(
    {
      source,
      operation,
      code: getErrorCode(error),
      contractStatus: getErrorContractStatus(error),
      severity,
    },
    error,
  );
}

function claimDiagnostic(error: unknown): boolean {
  if ((typeof error !== 'object' && typeof error !== 'function') || error === null) {
    return true;
  }
  if (reportedErrorObjects.has(error)) {
    return false;
  }
  reportedErrorObjects.add(error);
  return true;
}
