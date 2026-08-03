import { presentError, type PresentedError } from '@shared/error-presentation';
import { t } from '@shared/i18n';

import { publishNotification } from './notification-center';

export type PresentedErrorNotificationContent = Readonly<{
  severity: PresentedError['severity'];
  description: string;
  details: readonly string[];
}>;

export function getPresentedErrorNotificationContent(
  error: unknown,
): PresentedErrorNotificationContent {
  const presented = presentError(error);
  return {
    severity: presented.severity,
    description: presented.message,
    details: [
      ...presented.suggestedActions.map(({ label }) => label),
      ...(presented.recoveryBundlePath === undefined
        ? []
        : [t('error.recoveryBundlePath', { path: presented.recoveryBundlePath })]),
    ],
  };
}

export function publishPresentedErrorNotification(title: string, error: unknown): string {
  const content = getPresentedErrorNotificationContent(error);
  return publishNotification({
    severity: content.severity,
    title,
    description: content.description,
    details: content.details,
    important: content.severity === 'error',
  });
}
