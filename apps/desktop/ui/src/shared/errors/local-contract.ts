import type { MessageKeyWithoutParams } from '@shared/i18n';
import type { SuggestedActionCode } from './generated/desktop-command-errors';

type LocalErrorSpec = Readonly<{
  messageKey: MessageKeyWithoutParams;
  severity: 'warning' | 'error';
  actions?: readonly SuggestedActionCode[];
}>;

export const LOCAL_ERROR_CONTRACT = {
  desktop_transport_failed: {
    messageKey: 'error.desktopTransportFailed',
    severity: 'error',
  },
  unexpected_client_error: {
    messageKey: 'error.unexpectedClient',
    severity: 'error',
  },
  i18n_locale_load_failed: {
    messageKey: 'error.localeLoadFailed',
    severity: 'warning',
  },
  external_open_failed: {
    messageKey: 'error.unexpectedClient',
    severity: 'error',
  },
  dialog_open_failed: {
    messageKey: 'error.unexpectedClient',
    severity: 'error',
  },
  add_game_rollback_failed: {
    messageKey: 'addGame.rootCorrection.rollbackFailed',
    severity: 'error',
  },
  add_game_catalog_busy: {
    messageKey: 'addGame.catalogBusy',
    severity: 'warning',
  },
  nvapi_admin_required: {
    messageKey: 'user_message.nvapi_requires_administrator',
    severity: 'warning',
    actions: ['relaunch_as_administrator'],
  },
  graphics_swap_response_invalid: {
    messageKey: 'error.desktopTransportFailed',
    severity: 'error',
  },
  update_all_step_failed: {
    messageKey: 'error.unexpectedClient',
    severity: 'error',
  },
  updater_version_read_failed: {
    messageKey: 'error.unexpectedClient',
    severity: 'warning',
  },
  updater_check_failed: {
    messageKey: 'settings.about.updateCheckError',
    severity: 'error',
  },
  updater_download_failed: {
    messageKey: 'settings.about.updateDialog.prepareErrorDescription',
    severity: 'error',
  },
  updater_retry_wait_failed: {
    messageKey: 'settings.about.updateDialog.prepareErrorDescription',
    severity: 'error',
  },
  updater_install_failed: {
    messageKey: 'settings.about.updateDialog.installErrorDescription',
    severity: 'error',
  },
  updater_relaunch_failed: {
    messageKey: 'settings.about.updateDialog.restartRequiredDescription',
    severity: 'error',
  },
  updater_cleanup_failed: {
    messageKey: 'error.unexpectedClient',
    severity: 'warning',
  },
  d3d12_executable_repair_required: {
    messageKey: 'gameDetails.d3d12.action.repair',
    severity: 'error',
  },
  d3d12_plan_blocked: {
    messageKey: 'gameDetails.d3d12.action.blocked',
    severity: 'error',
  },
  d3d12_confirmation_unavailable: {
    messageKey: 'gameDetails.d3d12.action.blocked',
    severity: 'error',
  },
} as const satisfies Readonly<Record<string, LocalErrorSpec>>;

export type LocalErrorCode = keyof typeof LOCAL_ERROR_CONTRACT;

export function isLocalErrorCode(code: string): code is LocalErrorCode {
  return Object.prototype.hasOwnProperty.call(LOCAL_ERROR_CONTRACT, code);
}
