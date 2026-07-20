import type { Locale } from './locale';
import type { MessageDictionary, MessageValue } from './messages/types';

export type DynamicMessageCatalog = Partial<Record<Locale, MessageDictionary>>;

/** Resolves every source for the active locale before considering English.
 * This keeps a localized runtime/catalog override from being shadowed by a
 * coincidentally matching English static key. */
export function lookupLocalizedMessage(
  key: string,
  locale: Locale,
  staticMessages: Partial<Record<Locale, MessageDictionary>> & { en: MessageDictionary },
  dynamicCatalogs: readonly DynamicMessageCatalog[],
): MessageValue | undefined {
  const localizedStatic = staticMessages[locale]?.[key];
  if (localizedStatic !== undefined) {
    return localizedStatic;
  }

  for (const catalog of dynamicCatalogs) {
    const localizedDynamic = catalog[locale]?.[key];
    if (localizedDynamic !== undefined) {
      return localizedDynamic;
    }
  }

  return staticMessages.en[key];
}
