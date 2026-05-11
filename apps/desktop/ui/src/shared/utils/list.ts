const DEFAULT_COMPACT_LIST_LIMIT = 3;

export function compactList(
  values: string[],
  emptyCopy: string,
  maxVisible = DEFAULT_COMPACT_LIST_LIMIT,
): string {
  if (values.length === 0) {
    return emptyCopy;
  }

  const visibleLimit = normalizeVisibleLimit(maxVisible);
  const visibleValues = values.slice(0, visibleLimit);
  const remainingCount = values.length - visibleValues.length;
  const suffix = remainingCount > 0 ? ` +${remainingCount} more` : '';

  return `${visibleValues.join(' · ')}${suffix}`;
}

function normalizeVisibleLimit(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_COMPACT_LIST_LIMIT;
  }

  return Math.max(1, Math.floor(value));
}
