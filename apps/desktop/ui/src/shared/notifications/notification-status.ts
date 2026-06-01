import { normalizeCommandError } from '@shared/api';
import { t } from '@shared/i18n';
import { dismissNotification, publishNotification } from './notification-center';

export const STATUS_NOTIFICATION_ID = 'desktop-status';

function statusTitle(severity: 'error' | 'warning'): string {
  return severity === 'error' ? t('notify.statusError') : t('notify.statusWarning');
}

export function publishStatusNotification(
  message: string,
  severity: 'error' | 'warning',
): string | null {
  const description = normalizeOptionalText(message);

  if (description === undefined) {
    dismissNotification(STATUS_NOTIFICATION_ID);
    return null;
  }

  return publishNotification({
    id: STATUS_NOTIFICATION_ID,
    severity,
    title: statusTitle(severity),
    description,
    important: severity === 'error',
  });
}

export function clearStatusNotification(): void {
  dismissNotification(STATUS_NOTIFICATION_ID);
}

export function publishCommandErrorNotification(error: unknown): string | null {
  const normalizedError = normalizeCommandError(error);

  return publishStatusNotification(normalizedError.message, normalizedError.dto.severity);
}

function normalizeOptionalText(value?: string): string | undefined {
  if (value === undefined) {
    return undefined;
  }

  const normalizedValue = value.trim();

  return normalizedValue.length > 0 ? normalizedValue : undefined;
}
