import type { LocaleLoadError } from './errors';
import type { LanguageMode, Locale } from './locale-model';
import type { LocalePack } from './packs/types';
import type { I18nRuntimeState } from './runtime-types';

export function createInitialState(fallbackPack: LocalePack): I18nRuntimeState {
  return {
    status: 'idle',
    activeMode: 'en',
    activeLocale: fallbackPack.locale,
    pending: null,
    error: null,
  };
}

export function createActiveState(
  initialized: boolean,
  mode: LanguageMode,
  pack: LocalePack,
): I18nRuntimeState {
  return {
    status: initialized ? 'ready' : 'idle',
    activeMode: mode,
    activeLocale: pack.locale,
    pending: null,
    error: null,
  };
}

export function createLoadingState(
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

export function createReadyState(mode: LanguageMode, pack: LocalePack): I18nRuntimeState {
  return {
    status: 'ready',
    activeMode: mode,
    activeLocale: pack.locale,
    pending: null,
    error: null,
  };
}

export function createErrorState(
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

export function shouldObserveSystemLanguage(state: I18nRuntimeState): boolean {
  return state.activeMode === 'system' || state.pending?.mode === 'system';
}
