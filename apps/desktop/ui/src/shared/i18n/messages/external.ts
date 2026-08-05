import type { ExternalMessage, ExternalMessageCatalog } from './model';

type SourceCatalog = Readonly<Record<string, string>>;
type ExactTranslations<Source extends SourceCatalog> = Readonly<{
  [Key in keyof Source]: string;
}>;

/** Binds reviewed translations to the exact producer text they were based on. */
export function bindExternalMessages<const Source extends SourceCatalog>(
  source: Source,
  translations: ExactTranslations<Source>,
): ExternalMessageCatalog {
  return Object.fromEntries(
    Object.keys(source).map((key) => [
      key,
      { source: source[key], translation: translations[key] },
    ]),
  );
}

export function mergeExternalMessages(
  ...catalogs: readonly ExternalMessageCatalog[]
): ExternalMessageCatalog {
  const merged: Record<string, ExternalMessage> = {};
  for (const catalog of catalogs) {
    for (const [key, message] of Object.entries(catalog)) {
      if (message === undefined) {
        throw new Error(`Invalid external i18n message: ${key}`);
      }
      if (Object.hasOwn(merged, key)) {
        throw new Error(`Duplicate external i18n message: ${key}`);
      }
      merged[key] = message;
    }
  }
  return merged;
}
