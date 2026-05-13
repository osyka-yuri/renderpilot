export type VoidHandler = () => void;

export function ignoreError(task: () => void): void {
  try {
    task();
  } catch {
    // Preserve the original error.
  }
}
