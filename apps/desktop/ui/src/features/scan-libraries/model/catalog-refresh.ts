import { scanAutoLibraries } from '../api/desktop';
import { formatPresentedError } from '@shared/error-presentation';
import { t } from '@shared/i18n';

export type ScanLibrariesResult =
  | { kind: 'ok'; partialFailureCount: number }
  | { kind: 'error'; message: string };

/**
 * Triggers an automatic library scan and returns the result.
 * Catches top-level failures and returns a user-facing message instead of throwing.
 */
export async function scanAutoLibrariesWithErrorRecovery(): Promise<ScanLibrariesResult> {
  try {
    const scanResult = await scanAutoLibraries();

    return { kind: 'ok', partialFailureCount: scanResult.partialFailureCount };
  } catch (error) {
    return {
      kind: 'error',
      message: `${t('scan.automaticFailed')} ${formatPresentedError(error)}`,
    };
  }
}
