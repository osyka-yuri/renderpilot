import { toast } from 'svelte-sonner';

import { t, type MessageKeyWithoutParams } from '@shared/i18n';

export type CopyFeedbackKeys = {
  copied: MessageKeyWithoutParams;
  copyFailed: MessageKeyWithoutParams;
};

/**
 * Writes text to the clipboard and shows a success/error toast.
 * Returns whether the write succeeded so the caller can arm UI reset timers.
 */
export async function copyWithFeedback(text: string, keys: CopyFeedbackKeys): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    toast.error(t(keys.copyFailed));
    return false;
  }

  toast.success(t(keys.copied));
  return true;
}

/** Creates a 2s reset timer helper for copy-success UI state. */
export function createCopyResetTimer(onReset: () => void): {
  arm: () => void;
  dispose: () => void;
} {
  let timer: ReturnType<typeof setTimeout> | undefined;

  return {
    arm() {
      if (timer !== undefined) {
        clearTimeout(timer);
      }
      timer = setTimeout(() => {
        timer = undefined;
        onReset();
      }, 2000);
    },
    dispose() {
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
    },
  };
}
