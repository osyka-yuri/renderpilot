import {
  persistLanguageMode,
  readStoredLanguageMode,
  resolveLocale,
  type LanguageMode,
  type Locale,
} from './locale';
import { getFallbackPack, getLocaleLoaders } from './packs/registry';
import type { LocaleLoader, LocalePack } from './packs/types';
import { lookupLocalePackMessage } from './lookup';
import { LocaleLoadError } from './errors';
import { interpolateMessage, renderMessage } from './messages/runtime';
import type {
  MessageKey,
  MessageKeyWithoutParams,
  MessageParams,
  MessageRef,
  ParameterizedMessageKey,
} from './messages/en';
import { MESSAGE_CONTRACT_VERSION } from './messages/generated/contract-version';
import type { InterpolationParams } from './messages/model';
import type { ExactMessageParams } from './messages/params';

export { LocaleLoadError };

export type I18nInitializationResult = Readonly<{
  activeMode: LanguageMode;
  activeLocale: Locale;
  fallbackUsed: boolean;
  error: LocaleLoadError | null;
}>;

export type I18nSwitchResult =
  | Readonly<{ outcome: 'applied'; mode: LanguageMode; locale: Locale }>
  | Readonly<{ outcome: 'superseded'; mode: LanguageMode; locale: Locale }>;

export type I18nRuntimeState = Readonly<{
  status: 'idle' | 'loading' | 'ready' | 'error';
  activeMode: LanguageMode;
  activeLocale: Locale;
  pending: Readonly<{ mode: LanguageMode; locale: Locale }> | null;
  error: LocaleLoadError | null;
}>;

export type I18nRuntimeDependencies = Readonly<{
  fallbackPack: LocalePack;
  loaders: Readonly<Record<Locale, LocaleLoader>>;
  readStoredMode: () => LanguageMode;
  persistMode: (mode: LanguageMode) => void;
  resolveMode: (mode: LanguageMode) => Locale;
  applyDocumentLocale: (locale: Locale) => void;
}>;

export type ExternalMessageInput = Readonly<{
  key: string;
  fallback: string;
  params?: InterpolationParams;
}>;

export function createMessageRef<const Key extends MessageKeyWithoutParams>(
  key: Key,
): MessageRef<Key>;
export function createMessageRef<
  const Key extends ParameterizedMessageKey,
  const Params extends MessageParams<Key>,
>(key: Key, params: ExactMessageParams<MessageParams<Key>, Params>): MessageRef<Key>;
export function createMessageRef(key: MessageKey, params?: InterpolationParams) {
  return params === undefined ? { key } : { key, params };
}

type ActiveTransition = Readonly<{
  mode: LanguageMode;
  promise: Promise<I18nSwitchResult>;
}>;

function createInitialState(fallbackPack: LocalePack): I18nRuntimeState {
  return {
    status: 'idle',
    activeMode: 'en',
    activeLocale: fallbackPack.locale,
    pending: null,
    error: null,
  };
}

export function createI18nRuntime(deps: I18nRuntimeDependencies) {
  validatePack(deps.fallbackPack.locale, deps.fallbackPack);

  // These maps are opaque runtime caches, not render state. Locale activation
  // remains atomic through the single activePack state value.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const loadedPacks = new Map<Locale, LocalePack>([[deps.fallbackPack.locale, deps.fallbackPack]]);
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const inFlightLoads = new Map<Locale, Promise<LocalePack>>();

  let activePack = $state<LocalePack>(deps.fallbackPack);
  let state = $state<I18nRuntimeState>(createInitialState(deps.fallbackPack));
  let sequence = 0;
  let initialized = false;
  let initializationPromise: Promise<I18nInitializationResult> | null = null;
  let activeTransition: ActiveTransition | null = null;

  function getState(): I18nRuntimeState {
    return state;
  }

  function getLocale(): Locale {
    return state.activeLocale;
  }

  function initializeI18n(): Promise<I18nInitializationResult> {
    initializationPromise ??= initializeInternal();
    return initializationPromise;
  }

  async function initializeInternal(): Promise<I18nInitializationResult> {
    const storedMode = deps.readStoredMode();
    const targetLocale = deps.resolveMode(storedMode);

    state = loadingState(state, storedMode, targetLocale);

    try {
      const pack = await loadPack(targetLocale);

      applyPack(pack);
      initialized = true;
      state = readyState(storedMode, pack);

      return {
        activeMode: storedMode,
        activeLocale: pack.locale,
        fallbackUsed: false,
        error: null,
      };
    } catch (cause) {
      const error = toLocaleLoadError(storedMode, targetLocale, cause);

      sequence += 1;
      applyPack(deps.fallbackPack);
      initialized = true;
      state = errorState(
        storedMode === 'system' ? 'system' : 'en',
        deps.fallbackPack.locale,
        error,
      );

      return {
        activeMode: state.activeMode,
        activeLocale: state.activeLocale,
        fallbackUsed: true,
        error,
      };
    }
  }

  function setLanguageMode(mode: LanguageMode): Promise<I18nSwitchResult> {
    if (initializationPromise !== null && !initialized) {
      return initializationPromise.then(() => setLanguageMode(mode));
    }

    const targetLocale = deps.resolveMode(mode);

    if (activeTransition?.mode === mode) {
      return activeTransition.promise;
    }

    const cachedPack = loadedPacks.get(targetLocale);
    if (cachedPack !== undefined) {
      sequence += 1;
      activeTransition = null;
      commit(mode, cachedPack);
      state = activeState(mode, cachedPack);
      return Promise.resolve({ outcome: 'applied', mode, locale: targetLocale });
    }

    const requestSequence = ++sequence;
    state = loadingState(state, mode, targetLocale);

    const transition = loadAndCommit(mode, targetLocale, requestSequence);
    const wrappedTransition = transition.finally(() => {
      if (activeTransition?.promise === wrappedTransition) {
        activeTransition = null;
      }
    });

    activeTransition = { mode, promise: wrappedTransition };
    return wrappedTransition;
  }

  async function loadAndCommit(
    mode: LanguageMode,
    targetLocale: Locale,
    requestSequence: number,
  ): Promise<I18nSwitchResult> {
    try {
      const pack = await loadPack(targetLocale);

      if (requestSequence !== sequence) {
        return { outcome: 'superseded', mode, locale: targetLocale };
      }

      commit(mode, pack);
      state = activeState(mode, pack);
      return { outcome: 'applied', mode, locale: targetLocale };
    } catch (cause) {
      if (requestSequence !== sequence) {
        return { outcome: 'superseded', mode, locale: targetLocale };
      }

      const error = toLocaleLoadError(mode, targetLocale, cause);
      state = errorState(state.activeMode, state.activeLocale, error);
      throw error;
    }
  }

  function applyPack(pack: LocalePack): void {
    activePack = pack;
    deps.applyDocumentLocale(pack.locale);
  }

  function commit(mode: LanguageMode, pack: LocalePack): void {
    applyPack(pack);

    try {
      deps.persistMode(mode);
    } catch {
      // Persistence failures must not roll back an already committed pack.
    }
  }

  function activeState(mode: LanguageMode, pack: LocalePack): I18nRuntimeState {
    return {
      status: initialized ? 'ready' : 'idle',
      activeMode: mode,
      activeLocale: pack.locale,
      pending: null,
      error: null,
    };
  }

  function loadingState(
    current: I18nRuntimeState,
    mode: LanguageMode,
    locale: Locale,
  ): I18nRuntimeState {
    return {
      status: 'loading',
      activeMode: current.activeMode,
      activeLocale: current.activeLocale,
      pending: { mode, locale },
      error: null,
    };
  }

  function readyState(mode: LanguageMode, pack: LocalePack): I18nRuntimeState {
    return {
      status: 'ready',
      activeMode: mode,
      activeLocale: pack.locale,
      pending: null,
      error: null,
    };
  }

  function errorState(
    activeMode: LanguageMode,
    activeLocale: Locale,
    error: LocaleLoadError,
  ): I18nRuntimeState {
    return {
      status: 'error',
      activeMode,
      activeLocale,
      pending: null,
      error,
    };
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
      .then(deps.loaders[locale])
      .then((candidate: unknown) => {
        validatePack(locale, candidate);
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

  function translate(
    key: string,
    fallback: string,
    params: InterpolationParams | undefined,
  ): string {
    const value = lookupLocalePackMessage(key, activePack, deps.fallbackPack);
    return value === undefined
      ? interpolateMessage(fallback, params)
      : renderMessage(value, params, activePack.locale);
  }

  function t(key: MessageKeyWithoutParams): string;
  function t<const Key extends ParameterizedMessageKey, const Params extends MessageParams<Key>>(
    key: Key,
    params: ExactMessageParams<MessageParams<Key>, Params>,
  ): string;
  function t(key: MessageKey, params?: InterpolationParams): string {
    return translate(key, key, params);
  }

  function translateMessageRef(reference: MessageRef): string {
    return translate(
      reference.key,
      reference.key,
      'params' in reference ? reference.params : undefined,
    );
  }

  function translateExternalMessage(message: ExternalMessageInput): string {
    return translate(message.key, message.fallback, message.params);
  }

  function toLocaleLoadError(mode: LanguageMode, locale: Locale, cause: unknown): LocaleLoadError {
    return cause instanceof LocaleLoadError ? cause : new LocaleLoadError(mode, locale, cause);
  }

  function validatePack(
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

  return {
    getState,
    getLocale,
    initializeI18n,
    setLanguageMode,
    t,
    translateMessageRef,
    translateExternalMessage,
  };
}

const productionRuntime = createI18nRuntime({
  fallbackPack: getFallbackPack(),
  loaders: getLocaleLoaders(),
  readStoredMode: readStoredLanguageMode,
  persistMode: persistLanguageMode,
  resolveMode: resolveLocale,
  applyDocumentLocale: (locale) => {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = locale;
    }
  },
});

export const getI18nState = productionRuntime.getState;
export const getLocale = productionRuntime.getLocale;
export const initializeI18n = productionRuntime.initializeI18n;
export const setLanguageMode = productionRuntime.setLanguageMode;
export const t = productionRuntime.t;
export const translateMessageRef = productionRuntime.translateMessageRef;
export const translateExternalMessage = productionRuntime.translateExternalMessage;
