const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const;
const BYTES_PER_UNIT = 1024;

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B';
  }

  const unitIndex = Math.min(
    Math.floor(Math.log(bytes) / Math.log(BYTES_PER_UNIT)),
    BYTE_UNITS.length - 1,
  );

  const value = bytes / BYTES_PER_UNIT ** unitIndex;
  const rounded = Number(value.toFixed(1));

  return `${rounded} ${BYTE_UNITS[unitIndex]}`;
}
