export function formatTimestamp(value?: number | null): string {
  if (!value) {
    return 'No timestamp yet';
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(value);
}
