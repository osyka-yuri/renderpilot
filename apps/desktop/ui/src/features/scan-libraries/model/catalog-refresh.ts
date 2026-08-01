import type { ScanError } from '@entities/game';
import { scanAutoLibraries } from '../api/desktop';
import { describeCommandErrorBrief } from '@shared/api';

export type ScanLibrariesResult =
  | { kind: 'ok'; errors: readonly ScanError[] }
  | { kind: 'error'; message: string };

/**
 * Triggers an automatic library scan and returns the result.
 * Catches top-level failures and returns a user-facing message instead of throwing.
 */
export async function scanAutoLibrariesWithErrorRecovery(): Promise<ScanLibrariesResult> {
  try {
    const scanResult = await scanAutoLibraries();

    return { kind: 'ok', errors: scanResult.errors ?? [] };
  } catch (error) {
    return {
      kind: 'error',
      message: `Automatic library scan failed; your game list was still refreshed. ${describeCommandErrorBrief(error)}`,
    };
  }
}
