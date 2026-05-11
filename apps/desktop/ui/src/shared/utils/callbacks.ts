export type VoidHandler = () => void;

/** Runs a side-effect callback and swallows any thrown error. */
export function ignoreError(task: () => void): void {
  try {
    task();
  } catch {
    // Preserve the original error.
  }
}
