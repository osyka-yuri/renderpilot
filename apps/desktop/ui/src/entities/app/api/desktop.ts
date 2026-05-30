import { invokeDesktop } from '@shared/api';

import type { AppInitializationState } from '../model/types';

/**
 * Retrieves the application's initialization snapshot computed during process
 * startup (e.g., current elevation state). The backend command executes
 * synchronously, ensuring a highly performant IPC round-trip.
 */
export async function getAppInitializationState(): Promise<AppInitializationState> {
  return invokeDesktop<AppInitializationState>('get_app_initialization_state');
}

/**
 * Instructs the Windows OS to relaunch RenderPilot with administrator privileges
 * utilizing `ShellExecuteW(verb="runas")`. Upon success, the current instance
 * terminates and the elevated process initiates; consequently, this promise
 * will never resolve in a successful elevation scenario.
 *
 * Resolves with `{ relaunched: false, noop: true }` if the application already
 * possesses elevated privileges. Rejects with a `DesktopCommandError` if the
 * user cancels the UAC consent dialog or if OS policies restrict elevation.
 */
export async function requestAdminRelaunch(): Promise<{
  relaunched: boolean;
  noop?: boolean;
}> {
  return invokeDesktop<{ relaunched: boolean; noop?: boolean }>('request_admin_relaunch');
}
