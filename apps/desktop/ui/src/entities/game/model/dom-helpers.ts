const UNKNOWN_ID_SEGMENT = 'unknown';
const TITLE_ID_PREFIX = 'game-card-title';

export function normalizeDomIdSegment(value: string): string {
  const normalizedValue = value
    .trim()
    .replace(/\s+/g, '-')
    .replace(/[^a-zA-Z0-9_-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase();

  return normalizedValue || UNKNOWN_ID_SEGMENT;
}

export function createTitleId(gameId: string): string {
  return `${TITLE_ID_PREFIX}-${normalizeDomIdSegment(gameId)}`;
}
