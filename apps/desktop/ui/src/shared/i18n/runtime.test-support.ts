import type { LanguageMode, Locale } from './locale-model';
import { MESSAGE_CONTRACT_VERSION } from './messages/generated/contract-version';
import type { LocalePack } from './packs/types';
import { createI18nRuntime } from './runtime.svelte';

type TestMessage = {
  nav: string;
  dynamic?: string;
};

export function pack(
  locale: Locale,
  messages: TestMessage,
  dynamicCatalogs: LocalePack['dynamicCatalogs'] = [],
): LocalePack {
  return {
    locale,
    contractVersion: MESSAGE_CONTRACT_VERSION,
    messages: {
      'nav.games': messages.nav,
      ...(messages.dynamic ? { 'dynamic.key': messages.dynamic } : {}),
    },
    dynamicCatalogs,
  };
}

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });

  return { promise, resolve, reject };
}

export function createTestRuntime(options: {
  storedMode?: LanguageMode;
  loaders?: Partial<Record<Locale, () => Promise<LocalePack>>>;
  systemLocale?: Locale;
  applyDocumentLocale?: (locale: Locale) => void;
}) {
  const baseEnglishPack = pack('en', { nav: 'Games' });
  const enPack: LocalePack = {
    ...baseEnglishPack,
    messages: {
      ...baseEnglishPack.messages,
      'english.only': 'English fallback',
    },
  };
  const storedMode = options.storedMode ?? 'en';
  let persistedMode = storedMode;
  let currentSystemLocale = options.systemLocale ?? 'en';
  let documentLocale: Locale = 'en';
  const persistedModes: LanguageMode[] = [];
  const documentLocales: Locale[] = [];
  const systemLanguageListeners = new Set<() => void>();
  const loaders = {
    en: () => Promise.resolve(enPack),
    ru: () => Promise.resolve(pack('ru', { nav: 'Games' })),
    es: () => Promise.resolve(pack('es', { nav: 'Games' })),
    fr: () => Promise.resolve(pack('fr', { nav: 'Games' })),
    de: () => Promise.resolve(pack('de', { nav: 'Games' })),
    ja: () => Promise.resolve(pack('ja', { nav: 'Games' })),
    'zh-Hans': () => Promise.resolve(pack('zh-Hans', { nav: 'Games' })),
    'zh-Hant': () => Promise.resolve(pack('zh-Hant', { nav: 'Games' })),
    ...options.loaders,
  } satisfies Record<Locale, () => Promise<LocalePack>>;

  const runtime = createI18nRuntime({
    fallbackPack: enPack,
    loaders,
    readStoredMode: () => storedMode,
    persistMode: (mode) => {
      persistedMode = mode;
      persistedModes.push(mode);
    },
    resolveMode: (mode) => (mode === 'system' ? currentSystemLocale : mode),
    observeSystemLanguage: (listener) => {
      systemLanguageListeners.add(listener);
      return () => systemLanguageListeners.delete(listener);
    },
    applyDocumentLocale: (locale) => {
      options.applyDocumentLocale?.(locale);
      documentLocale = locale;
      documentLocales.push(locale);
    },
  });

  return {
    runtime,
    get persistedMode() {
      return persistedMode;
    },
    get documentLocale() {
      return documentLocale;
    },
    persistedModes,
    documentLocales,
    setSystemLocale(locale: Locale) {
      currentSystemLocale = locale;
      for (const listener of [...systemLanguageListeners]) {
        listener();
      }
    },
    get systemObserverCount() {
      return systemLanguageListeners.size;
    },
  };
}
