import { invokeDesktop } from '@shared/api';

export async function startBackgroundRefresh(): Promise<{
  started: boolean;
  partialFailureCount: number;
}> {
  return invokeDesktop<{ started: boolean; partialFailureCount: number }>(
    'start_background_refresh',
  );
}
