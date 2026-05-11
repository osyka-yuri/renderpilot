export function formatPartialScanWarning(scanErrorCount: number): string {
  assertPositiveInteger(scanErrorCount, 'scanErrorCount');

  const failedRootLabel = getFailedRootLabel(scanErrorCount);

  return `Some game libraries could not be scanned (${scanErrorCount} ${failedRootLabel} failed). Check logs for details.`;
}

function getFailedRootLabel(count: number): 'root' | 'roots' {
  return count === 1 ? 'root' : 'roots';
}

function assertPositiveInteger(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new RangeError(`${name} must be a positive integer.`);
  }
}
