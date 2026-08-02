import type { Locale } from './locale-model';
import { MESSAGE_CONTRACT_VERSION } from './messages/generated/contract-version';
import type { LocaleLoader, LocalePack } from './packs/types';

export type LocalePackLoader = Readonly<{
  getLoadedPack: (locale: Locale) => LocalePack | undefined;
  loadPack: (locale: Locale) => Promise<LocalePack>;
}>;

export function createLocalePackLoader(
  fallbackPack: LocalePack,
  loaders: Readonly<Record<Locale, LocaleLoader>>,
): LocalePackLoader {
  validateLocalePack(fallbackPack.locale, fallbackPack);

  const loadedPacks = new Map<Locale, LocalePack>([[fallbackPack.locale, fallbackPack]]);
  const inFlightLoads = new Map<Locale, Promise<LocalePack>>();

  function getLoadedPack(locale: Locale): LocalePack | undefined {
    return loadedPacks.get(locale);
  }

  function loadPack(locale: Locale): Promise<LocalePack> {
    const loaded = loadedPacks.get(locale);
    if (loaded !== undefined) {
      return Promise.resolve(loaded);
    }

    const pending = inFlightLoads.get(locale);
    if (pending !== undefined) {
      return pending;
    }

    const load = Promise.resolve()
      .then(loaders[locale])
      .then((candidate: unknown) => {
        validateLocalePack(locale, candidate);
        loadedPacks.set(locale, candidate);
        return candidate;
      })
      .finally(() => {
        if (inFlightLoads.get(locale) === load) {
          inFlightLoads.delete(locale);
        }
      });

    inFlightLoads.set(locale, load);
    return load;
  }

  return { getLoadedPack, loadPack };
}

function validateLocalePack(
  expectedLocale: Locale,
  candidate: unknown,
): asserts candidate is LocalePack {
  if (
    !isRecord(candidate) ||
    candidate.locale !== expectedLocale ||
    candidate.contractVersion !== MESSAGE_CONTRACT_VERSION ||
    !isRecord(candidate.messages) ||
    !Array.isArray(candidate.dynamicCatalogs)
  ) {
    throw new Error(`Invalid locale pack for "${expectedLocale}".`);
  }
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
