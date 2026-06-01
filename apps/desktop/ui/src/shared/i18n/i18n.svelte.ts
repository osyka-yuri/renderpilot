/**
 * Reactive localization core (Svelte 5 universal reactivity).
 *
 * A module-level `$state` holds the current language mode; every `t()` call
 * reads it, so switching the language re-runs all `t()` usages inside templates
 * and `$derived` — the whole UI re-renders without a reload.
 */

import {
  persistLanguageMode,
  readStoredLanguageMode,
  resolveLocale,
  type LanguageMode,
  type Locale,
} from './locale';
import { messages, nvapiOverrides, type MessageKey } from './messages';
import type {
  InterpolationParams,
  MessageDictionary,
  MessageValue,
  PluralForms,
} from './messages/types';

let currentMode = $state<LanguageMode>('system');

const currentLocale = $derived(resolveLocale(currentMode));

const pluralRulesByLocale: Partial<Record<Locale, Intl.PluralRules>> = {};

/** Reads the persisted preference and applies it. Call once during bootstrap. */
export function initI18n(): void {
  applyLanguageMode(readStoredLanguageMode());
}

/** Persists and applies a new preference. Re-renders the UI reactively. */
export function setLanguageMode(mode: LanguageMode): void {
  persistLanguageMode(mode);
  applyLanguageMode(mode);
}

/** Concrete resolved language the catalog is keyed by. Reactive. */
export function getLocale(): Locale {
  return currentLocale;
}

/** Translate a known catalog key. Supports `{name}` interpolation and plurals (via `params.count`). */
export function t(key: MessageKey, params?: InterpolationParams): string {
  return translate(key, key, params);
}

/**
 * Translate a dynamic (non-literal) key — e.g. a backend `messageKey`. Falls
 * back to `fallback` when the key is absent from the catalog.
 */
export function translateKey(key: string, fallback: string, params?: InterpolationParams): string {
  return translate(key, fallback, params);
}

function translate(key: string, fallback: string, params: InterpolationParams | undefined): string {
  const value = lookup(key, currentLocale);

  if (value === undefined) {
    return interpolate(fallback, params);
  }

  return render(value, params, currentLocale);
}

function applyLanguageMode(mode: LanguageMode): void {
  currentMode = mode;
  applyDocumentLanguage(resolveLocale(mode));
}

function lookup(key: string, locale: Locale): MessageValue | undefined {
  // Typed catalog first (static UI strings + backend error keys), then the
  // locale's NVAPI overrides. NVAPI English is omitted from the overrides, so
  // `en` falls through to the caller's fallback (the backend `dlss_settings.json`
  // text) — a single source of English truth. `entry()` models the index access
  // as possibly-undefined: callers pass arbitrary runtime keys that may be absent.
  return entry(messages[locale], key) ?? entry(messages.en, key) ?? nvapiOverrides[locale]?.[key];
}

function entry(dict: MessageDictionary, key: string): MessageValue | undefined {
  return dict[key];
}

function render(
  value: MessageValue,
  params: InterpolationParams | undefined,
  locale: Locale,
): string {
  if (typeof value === 'string') {
    return interpolate(value, params);
  }

  const count = typeof params?.count === 'number' ? params.count : 0;

  return interpolate(selectPluralForm(value, count, locale), params);
}

function selectPluralForm(forms: PluralForms, count: number, locale: Locale): string {
  const category = pluralRulesFor(locale).select(count);

  return forms[category as keyof PluralForms] ?? forms.other;
}

function pluralRulesFor(locale: Locale): Intl.PluralRules {
  let rules = pluralRulesByLocale[locale];

  if (!rules) {
    rules = new Intl.PluralRules(locale);
    pluralRulesByLocale[locale] = rules;
  }

  return rules;
}

function interpolate(template: string, params?: InterpolationParams): string {
  if (!params) {
    return template;
  }

  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : match,
  );
}

function applyDocumentLanguage(locale: Locale): void {
  if (typeof document !== 'undefined') {
    document.documentElement.lang = locale;
  }
}
