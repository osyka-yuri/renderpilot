export type NotificationSeverity = 'error' | 'warning' | 'success' | 'info';

export type Notification = {
  id: string;
  severity: NotificationSeverity;
  title: string;
  description?: string;
  important?: boolean;
};

export type NotificationEvent =
  | {
      type: 'published';
      notification: Notification;
    }
  | {
      type: 'dismissed';
      id: string;
    };

export type NotificationInput = {
  id?: string;
  severity: NotificationSeverity;
  title: string;
  description?: string;
  important?: boolean;
};

export type NotificationListener = (event: NotificationEvent) => void;
