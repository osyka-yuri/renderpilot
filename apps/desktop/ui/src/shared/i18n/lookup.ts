import type { LocalePack } from './packs/types';
import type { MessageValue } from './messages/types';

/**
 * Looks up a key in the already committed pack and then in the eager English
 * fallback pack. Keeping the active pack as the first argument makes the
 * atomic runtime commit the only state transition translation reads observe.
 */
export function lookupLocalePackMessage(
  key: string,
  activePack: LocalePack,
  fallbackPack: LocalePack,
): MessageValue | undefined {
  const activeMessages: Readonly<Partial<Record<string, MessageValue>>> = activePack.messages;
  const localizedStatic = activeMessages[key];
  if (localizedStatic !== undefined) {
    return localizedStatic;
  }

  for (const catalog of activePack.dynamicCatalogs) {
    const localizedDynamic = catalog[key];
    if (localizedDynamic !== undefined) {
      return localizedDynamic;
    }
  }

  const fallbackMessages: Readonly<Partial<Record<string, MessageValue>>> = fallbackPack.messages;
  return fallbackMessages[key];
}
