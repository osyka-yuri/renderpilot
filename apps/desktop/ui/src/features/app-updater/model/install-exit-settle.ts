import { tick } from 'svelte';

/** Waits for Svelte state flush and one browser paint opportunity. */
function waitForNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => {
        resolve();
      });
      return;
    }
    setTimeout(resolve, 0);
  });
}

/**
 * Lifecycle-based settle before Windows may exit the process. There is no
 * wall-clock timeout: readiness follows Svelte's flush and the browser frame.
 * Overridable via `CreateAppUpdaterModelOptions.settleUiBeforeInstallExit`.
 */
export async function settleUiBeforeInstallExit(): Promise<void> {
  await tick();
  await waitForNextPaint();
}
