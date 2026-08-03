import { LocaleLoadError } from './errors';
import { persistLanguageMode, readStoredLanguageMode } from './language-mode-storage';
import type { Locale } from './locale-model';
import { lookupLocalePackMessage } from './lookup';
import type {
  MessageKey,
  MessageKeyWithoutParams,
  MessageParams,
  MessageRef,
  ParameterizedMessageKey,
} from './messages/en';
import type { InterpolationParams } from './messages/model';
import type { ExactMessageParams } from './messages/params';
import { interpolateMessage, renderMessage } from './messages/runtime';
import { createLocalePackLoader } from './locale-pack-loader';
import { getFallbackPack, getLocaleLoaders } from './packs/registry';
import type { LocalePack } from './packs/types';
import { createI18nCoordinator } from './runtime-coordinator';
import { createInitialState } from './runtime-state';
import type {
  ExternalMessageInput,
  I18nInitializationResult,
  I18nRuntimeDependencies,
  I18nRuntimeState,
  I18nSwitchResult,
} from './runtime-types';
import { observeSystemLanguage, resolveLocale } from './system-language';
import { reportErrorDiagnostic } from '@shared/diagnostics';

export { LocaleLoadError };
export type {
  ExternalMessageInput,
  I18nInitializationResult,
  I18nRuntimeDependencies,
  I18nRuntimeState,
  I18nSwitchResult,
};

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

export function createI18nRuntime(deps: I18nRuntimeDependencies) {
  let activePack = $state<LocalePack>(deps.fallbackPack);
  let state = $state<I18nRuntimeState>(createInitialState(deps.fallbackPack));

  const packLoader = createLocalePackLoader(deps.fallbackPack, deps.loaders);
  const coordinator = createI18nCoordinator(
    {
      fallbackPack: deps.fallbackPack,
      readStoredMode: deps.readStoredMode,
      persistMode: deps.persistMode,
      resolveMode: deps.resolveMode,
      observeSystemLanguage: deps.observeSystemLanguage,
      getLoadedPack: packLoader.getLoadedPack,
      loadPack: packLoader.loadPack,
      reportLoadError: deps.onLocaleLoadError ?? ignoreLocaleLoadError,
    },
    {
      getState: () => state,
      publishState: (nextState) => {
        state = nextState;
      },
      activatePack: (pack) => {
        deps.applyDocumentLocale(pack.locale);
        activePack = pack;
      },
    },
  );

  function getState(): I18nRuntimeState {
    return state;
  }

  function getLocale(): Locale {
    return state.activeLocale;
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

  return {
    getState,
    getLocale,
    initializeI18n: coordinator.initializeI18n,
    setLanguageMode: coordinator.setLanguageMode,
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
  observeSystemLanguage,
  applyDocumentLocale: (locale) => {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = locale;
    }
  },
  onLocaleLoadError: (error, operation) => {
    reportErrorDiagnostic(
      {
        source: 'i18n',
        operation,
        code: error.code,
        contractStatus: 'known',
        severity: 'warning',
        locale: error.locale,
        mode: error.mode,
      },
      error.cause,
    );
  },
});

export const getI18nState = productionRuntime.getState;
export const getLocale = productionRuntime.getLocale;
export const initializeI18n = productionRuntime.initializeI18n;
export const setLanguageMode = productionRuntime.setLanguageMode;
export const t = productionRuntime.t;
export const translateMessageRef = productionRuntime.translateMessageRef;
export const translateExternalMessage = productionRuntime.translateExternalMessage;

function ignoreLocaleLoadError(): void {
  return;
}
