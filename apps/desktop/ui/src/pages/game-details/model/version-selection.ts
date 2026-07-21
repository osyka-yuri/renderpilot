/** Returns a stable installed-state value that cannot alias a candidate id. */
export function installedSelectionValue(
  componentId: string,
  candidateArtifactIds: readonly string[],
): string {
  const occupied = new Set(candidateArtifactIds);
  for (let suffix = 0; ; suffix += 1) {
    const value = `installed:${componentId}:${suffix}`;
    if (!occupied.has(value)) {
      return value;
    }
  }
}
