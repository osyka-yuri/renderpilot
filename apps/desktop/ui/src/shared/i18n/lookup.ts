import type { LocalePack } from './packs/types';
import type { MessageValue } from './messages/model';

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

  const fallbackMessages: Readonly<Partial<Record<string, MessageValue>>> = fallbackPack.messages;
  return fallbackMessages[key];
}

/**
 * Looks up an externally supplied message without trusting a translation that
 * was reviewed against a different source string. Static messages deliberately
 * keep precedence because backend error contracts can reference parameterized
 * messages from the regular catalog.
 */
export function lookupExternalMessage(
  key: string,
  source: string,
  activePack: LocalePack,
  fallbackPack: LocalePack,
): MessageValue | undefined {
  const activeMessages: Readonly<Partial<Record<string, MessageValue>>> = activePack.messages;
  const localizedStatic = activeMessages[key];
  if (localizedStatic !== undefined) {
    return localizedStatic;
  }

  const localizedExternal = activePack.externalMessages[key];
  if (localizedExternal?.source === source) {
    return localizedExternal.translation;
  }

  const fallbackMessages: Readonly<Partial<Record<string, MessageValue>>> = fallbackPack.messages;
  return fallbackMessages[key];
}
