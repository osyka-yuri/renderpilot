export function hasPartialNormalizedSelection(
  selectedValues: readonly string[],
  availableValues: readonly string[],
): boolean {
  const selectedValueSet = new Set(selectedValues);

  return availableValues.some((value) => !selectedValueSet.has(value));
}
