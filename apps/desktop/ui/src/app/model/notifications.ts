import { publishStatusNotification } from '@shared/notifications';
import { t } from '@shared/i18n';

export function publishStalePlanNotification(): string {
  return publishStatusNotification(t('notify.stalePlan'), 'error') ?? 'desktop-status';
}

export function publishMissingStableGameDetailsNotification(): string {
  return publishStatusNotification(t('notify.missingStableGameId'), 'error') ?? 'desktop-status';
}
