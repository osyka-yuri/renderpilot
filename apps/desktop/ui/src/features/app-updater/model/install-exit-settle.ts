/**
 * Wall-clock settle after a busy-phase assignment so Svelte can paint before
 * Windows `update.install()` calls process::exit(0). ~450ms is enough for one
 * committed frame without feeling like a long artificial delay.
 */
const INSTALL_EXIT_PAINT_MS = 450;

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

/** Two animation frames (or a macrotask) so layout can commit. */
function waitForNextPaint(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          resolve();
        });
      });
      return;
    }
    setTimeout(resolve, 0);
  });
}

/**
 * Production settle before Windows `update.install()` may exit the process.
 * Overridable via `CreateAppUpdaterModelOptions.settleUiBeforeInstallExit`.
 */
export async function settleUiBeforeInstallExit(): Promise<void> {
  await waitForNextPaint();
  await delay(INSTALL_EXIT_PAINT_MS);
}
