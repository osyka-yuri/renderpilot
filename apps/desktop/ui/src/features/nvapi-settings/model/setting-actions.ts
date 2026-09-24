/** Whether the confirmed live profile contains an explicit override to delete. */
export function canResetToDriverDefault(currentIsExplicit: boolean | null): boolean {
  return currentIsExplicit === true;
}
