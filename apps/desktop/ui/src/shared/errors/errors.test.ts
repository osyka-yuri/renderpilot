import { describe, expect, it } from 'vitest';
import { LocaleLoadError, t } from '@shared/i18n';
import { formatPresentedError, presentError } from '@shared/error-presentation';
import {
  ClientError,
  DesktopCommandError,
  getErrorCode,
  normalizeDesktopCommandError,
} from './index';

describe('desktop error model and presenter', () => {
  it('presents known command codes entirely from the local contract', () => {
    const raw = {
      code: 'storage_failed',
      details: 'PRIVATE backend prose D:/secret/catalog.db',
    };
    const error = normalizeDesktopCommandError(raw);
    const presented = presentError(error);

    expect(error.contractStatus).toBe('malformed');
    expect(presented.message).toBe(t('user_message.storage_failed'));
    expect(presented.message).not.toContain('PRIVATE');
    expect(presented.suggestedActions).toEqual([
      {
        code: 'inspect_logs',
        label: t('suggested_action.inspect_logs'),
      },
    ]);
    expect(formatPresentedError(error)).toBe(
      `${t('user_message.storage_failed')} ${t('suggested_action.inspect_logs')}`,
    );
  });

  it('classifies a shared updater command code as known transport data', () => {
    const error = normalizeDesktopCommandError({ code: 'app_update_check_failed' });

    expect(error.contractStatus).toBe('known');
    expect(presentError(error)).toMatchObject({
      code: 'app_update_check_failed',
      severity: 'error',
      message: t('user_message.operation_could_not_complete'),
    });
  });

  it('accepts only structured fields allowed for the known code', () => {
    const invalidRoot = presentError({
      code: 'invalid_install_root',
      reasonCode: 'filesystem_root',
    });
    expect(invalidRoot.contractStatus).toBe('known');
    expect(invalidRoot.reasonCode).toBe('filesystem_root');

    const cleanup = presentError({
      code: 'managed_cleanup_ambiguous',
      recoveryBundlePath: ' C:/Recovery/renderpilot-bundle ',
    });
    expect(cleanup.contractStatus).toBe('known');
    expect(cleanup.recoveryBundlePath).toBe('C:/Recovery/renderpilot-bundle');

    const malformed = presentError({
      code: 'invalid_install_root',
      reasonCode: 'backend prose must not pass',
      recoveryBundlePath: 'C:/private',
    });
    expect(malformed.contractStatus).toBe('malformed');
    expect(malformed.reasonCode).toBeUndefined();
    expect(malformed.recoveryBundlePath).toBeUndefined();

    const controlCharacters = presentError({
      code: 'managed_cleanup_ambiguous',
      recoveryBundlePath: 'C:/Recovery/bundle\nPRIVATE',
    });
    expect(controlCharacters.contractStatus).toBe('malformed');
    expect(controlCharacters.recoveryBundlePath).toBeUndefined();
  });

  it('sanitizes direct DesktopCommandError construction as defensively as transport input', () => {
    const error = DesktopCommandError.fromDto({
      code: 'managed_cleanup_ambiguous',
      recoveryBundlePath: ' C:/Recovery/bundle ',
      details: 'PRIVATE backend prose',
    } as never);

    expect(error.contractStatus).toBe('malformed');
    expect(error.dto).toEqual({
      code: 'managed_cleanup_ambiguous',
      recoveryBundlePath: 'C:/Recovery/bundle',
    });
    expect(JSON.stringify(error.dto)).not.toContain('PRIVATE');
    expect(JSON.stringify(error)).not.toContain('PRIVATE');
  });

  it('never displays unknown or malformed backend values', () => {
    const unknown = presentError({ code: 'future_backend_code' });
    expect(unknown.code).toBe('future_backend_code');
    expect(unknown.contractStatus).toBe('unknown');
    expect(unknown.message).toBe(t('error.unexpectedClient'));
    expect(unknown.message).not.toContain('future_backend_code');

    const malformed = presentError(new Error('PRIVATE C:/Users/name/token'));
    expect(malformed.code).toBe('unexpected_client_error');
    expect(malformed.message).toBe(t('error.unexpectedClient'));
    expect(malformed.message).not.toContain('PRIVATE');

    const unsafeTransportValue = normalizeDesktopCommandError({
      code: 'PRIVATE backend prose',
    });
    expect(unsafeTransportValue).toMatchObject({
      code: 'desktop_transport_failed',
      contractStatus: 'malformed',
    });
    expect(presentError(unsafeTransportValue).message).toBe(t('error.desktopTransportFailed'));

    const oversizedCode = normalizeDesktopCommandError({ code: `a${'b'.repeat(64)}` });
    expect(oversizedCode).toMatchObject({
      code: 'desktop_transport_failed',
      contractStatus: 'malformed',
    });

    const arrayValue = Object.assign([], { code: 'storage_failed' });
    const classValue = new (class {
      code = 'storage_failed';
    })();
    expect(normalizeDesktopCommandError(arrayValue).contractStatus).toBe('malformed');
    expect(normalizeDesktopCommandError(classValue).contractStatus).toBe('malformed');
  });

  it('localizes LocaleLoadError while preserving its cause chain', () => {
    const cause = new Error('dynamic import failed at C:/private');
    const error = new LocaleLoadError('ru', 'ru', cause);
    const presented = presentError(error);

    expect(getErrorCode(error)).toBe('i18n_locale_load_failed');
    expect(error.cause).toBe(cause);
    expect(JSON.stringify(error)).not.toContain('dynamic import');
    expect(presented.contractStatus).toBe('known');
    expect(presented.message).toBe(t('error.localeLoadFailed'));
    expect(presented.message).not.toContain('dynamic import');
  });

  it('presents typed client errors without using Error.message', () => {
    const error = new ClientError('d3d12_plan_blocked', new Error('private blocker'));
    expect(JSON.stringify(error)).not.toContain('private blocker');
    expect(presentError(error)).toEqual({
      code: 'd3d12_plan_blocked',
      severity: 'error',
      message: t('gameDetails.d3d12.action.blocked'),
      suggestedActions: [],
      contractStatus: 'known',
    });
  });
});
