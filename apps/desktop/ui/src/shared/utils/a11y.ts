import { trimToOptional } from './normalize';

export type A11yTextPropsInput = Readonly<{
  label?: string | null;
  labelledBy?: string | null;
  describedBy?: string | null;
  title?: string | null;
}>;

export type NormalizedA11yTextProps = Readonly<{
  ariaLabel: string | undefined;
  ariaLabelledBy: string | undefined;
  ariaDescribedBy: string | undefined;
  title: string | undefined;
}>;

/**
 * Prefer `aria-labelledby`.
 *
 * When `aria-labelledby` is set to a non-blank value, `aria-label`
 * should be omitted to avoid competing accessible names.
 */
export function ariaLabelUnlessLabelledBy(
  label: string | null | undefined,
  labelledBy: string | null | undefined,
): string | undefined {
  const normalizedLabelledBy = trimToOptional(labelledBy);

  if (normalizedLabelledBy !== undefined) {
    return undefined;
  }

  return trimToOptional(label);
}

export function normalizeA11yTextProps(input: A11yTextPropsInput): NormalizedA11yTextProps {
  const ariaLabelledBy = trimToOptional(input.labelledBy);

  return {
    ariaLabel: ariaLabelledBy === undefined ? trimToOptional(input.label) : undefined,
    ariaLabelledBy,
    ariaDescribedBy: trimToOptional(input.describedBy),
    title: trimToOptional(input.title),
  };
}
