export function compactFullSelectionForQuery<T extends string>(
  selected: readonly T[],
  available: readonly T[],
): T[] {
  if (available.length === 0 || selected.length === 0) {
    return [];
  }

  const selectedSet = new Set(selected);
  const selectsEverything = available.every((value) => selectedSet.has(value));

  return selectsEverything ? [] : [...selected];
}
