export type ClassValue = string | false | null | undefined;

/**
 * Converts a class value into a normalized class name.
 * Returns null for values that should be skipped.
 */
function toClassName(value: ClassValue): string | null {
  if (typeof value !== 'string') {
    return null;
  }

  const className = value.trim();

  return className === '' ? null : className;
}

/**
 * Joins non-empty CSS class names.
 *
 * Useful for conditional class composition:
 *
 * cx('button', isActive && 'button--active')
 */
export function cx(...values: readonly ClassValue[]): string {
  return values
    .map(toClassName)
    .filter((value): value is string => value !== null)
    .join(' ');
}
