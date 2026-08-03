import type {
  Notification,
  NotificationEvent,
  NotificationInput,
  NotificationListener,
} from './types';

const listeners = new Set<NotificationListener>();
const activeNotifications = new Map<string, Notification>();

let nextNotificationId = 0;

export function publishNotification(input: NotificationInput): string {
  const notification = createNotification(input);

  activeNotifications.set(notification.id, notification);
  emit({ type: 'published', notification });

  return notification.id;
}

export function dismissNotification(id: string): void {
  if (!activeNotifications.delete(id)) {
    return;
  }

  emit({ type: 'dismissed', id });
}

export function subscribeToNotificationEvents(listener: NotificationListener): () => void {
  listeners.add(listener);

  return () => {
    listeners.delete(listener);
  };
}

export function getActiveNotifications(): readonly Notification[] {
  return Array.from(activeNotifications.values());
}

export function clearAllNotifications(): void {
  for (const id of Array.from(activeNotifications.keys())) {
    dismissNotification(id);
  }

  nextNotificationId = 0;
}

function createNotification(input: NotificationInput): Notification {
  const details = normalizeDetails(input.details);
  return {
    id: input.id ?? createNotificationId(),
    severity: input.severity,
    title: normalizeRequiredText(input.title, 'Notification title'),
    description: normalizeOptionalText(input.description),
    ...(details === undefined ? {} : { details }),
    important: input.important,
  };
}

function normalizeDetails(values?: readonly string[]): readonly string[] | undefined {
  if (values === undefined) {
    return undefined;
  }

  const details = values
    .map((value) => normalizeOptionalText(value))
    .filter((value): value is string => value !== undefined);
  return details.length === 0 ? undefined : details;
}

function createNotificationId(): string {
  nextNotificationId += 1;
  return `notification-${nextNotificationId}`;
}

function normalizeRequiredText(value: string, fieldName: string): string {
  const normalizedValue = value.trim();

  if (normalizedValue.length === 0) {
    throw new RangeError(`${fieldName} must not be empty.`);
  }

  return normalizedValue;
}

function normalizeOptionalText(value?: string): string | undefined {
  if (value === undefined) {
    return undefined;
  }

  const normalizedValue = value.trim();

  return normalizedValue.length > 0 ? normalizedValue : undefined;
}

function emit(event: NotificationEvent): void {
  for (const listener of listeners) {
    listener(event);
  }
}
