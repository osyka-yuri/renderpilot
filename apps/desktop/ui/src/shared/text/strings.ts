const GENERIC_UPPERCASE_WORDS = new Set([
  'ACL',
  'API',
  'CPU',
  'DLL',
  'EA',
  'GPU',
  'IO',
  'JSON',
  'PRD',
  'SHA256',
  'UI',
]);

export function shallowStringArrayEqual(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return (
    left === right ||
    (left.length === right.length && left.every((value, index) => value === right[index]))
  );
}

export function humanizeToken(value: string, extraWords?: Set<string>): string {
  const spaced = value
    .replace(/_/g, ' ')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
    .trim();

  return spaced
    .split(/\s+/)
    .map((word) => {
      const upper = word.toUpperCase();
      if (GENERIC_UPPERCASE_WORDS.has(upper) || extraWords?.has(upper)) {
        return upper;
      }

      return upper[0] + upper.slice(1).toLowerCase();
    })
    .join(' ');
}
