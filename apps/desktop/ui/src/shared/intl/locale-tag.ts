export type CanonicalLocaleTag = Readonly<{
  tag: string;
  language: string;
  script?: string;
  region?: string;
}>;

const canonicalLocaleTags = new Map<string, CanonicalLocaleTag>();

/**
 * Canonicalizes and decomposes a BCP 47 language tag through the platform Intl
 * implementation. Invalid input deliberately preserves the native RangeError
 * contract so callers can decide whether invalid tags are fatal or skippable.
 */
export function parseLocaleTag(locale: string): CanonicalLocaleTag {
  const cached = canonicalLocaleTags.get(locale);
  if (cached !== undefined) {
    return cached;
  }

  const [tag] = Intl.getCanonicalLocales(locale);
  const parsed = new Intl.Locale(tag);
  const result: CanonicalLocaleTag = Object.freeze({
    tag,
    language: parsed.language,
    ...(parsed.script === undefined ? {} : { script: parsed.script }),
    ...(parsed.region === undefined ? {} : { region: parsed.region }),
  });

  canonicalLocaleTags.set(locale, result);
  canonicalLocaleTags.set(tag, result);
  return result;
}
