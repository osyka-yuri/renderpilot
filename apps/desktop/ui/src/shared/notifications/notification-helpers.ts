import { publishNotification } from './notification-center';
import type { NotificationSeverity } from './types';

/**
 * Formats an unknown error value into a human-readable string suitable for
 * use as a notification description.
 */
export function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  return String(error);
}

export function publishSuccessNotification(title: string, description?: string): string {
  return publishTransientNotification('success', title, description);
}

export function publishInfoNotification(title: string, description?: string): string {
  return publishTransientNotification('info', title, description);
}

export function publishWarningNotification(title: string, description?: string): string {
  return publishNotification({ severity: 'warning', title, description });
}

/**
 * Error toasts default to `important: true` so they are not auto-dismissed
 * — the user needs to see what action failed and why.
 */
export function publishErrorNotification(title: string, description?: string): string {
  return publishNotification({ severity: 'error', title, description, important: true });
}

function publishTransientNotification(
  severity: Extract<NotificationSeverity, 'success' | 'info'>,
  title: string,
  description?: string,
): string {
  return publishNotification({
    severity,
    title,
    description,
  });
}
