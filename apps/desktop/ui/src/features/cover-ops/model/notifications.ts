import {
  publishInfoNotification,
  publishStatusNotification,
  publishSuccessNotification,
} from '@shared/notifications';

const COVER_PICKER_PREVIEW_MODE_MESSAGE = 'Choosing a cover file requires the desktop app.';

export function publishCoverUpdatedNotification(): string {
  return publishSuccessNotification('Cover updated', 'The custom artwork has been saved.');
}

export function publishCoverDownloadedNotification(): string {
  return publishInfoNotification('Cover downloaded', 'The game artwork has been refreshed.');
}

export function publishCoverRemovedNotification(): string {
  return publishSuccessNotification('Cover removed', 'The game now uses the default artwork.');
}

export function publishCoverOperationErrorNotification(message: string): string | null {
  return publishStatusNotification(message, 'error');
}

export function publishCoverPickerPreviewModeNotification(): string | null {
  return publishStatusNotification(COVER_PICKER_PREVIEW_MODE_MESSAGE, 'error');
}
