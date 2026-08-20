import { t, type MessageKeyWithoutParams } from '@shared/i18n';
import { publishErrorNotification, publishSuccessNotification } from '@shared/notifications';

export type CopyFeedbackKeys = {
  copied: MessageKeyWithoutParams;
  copyFailed: MessageKeyWithoutParams;
};

/**
 * Writes text to the clipboard and shows a success/error toast.
 */
export async function copyWithFeedback(text: string, keys: CopyFeedbackKeys): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    publishErrorNotification(t(keys.copyFailed));
    return;
  }

  publishSuccessNotification(t(keys.copied));
}
