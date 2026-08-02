import type { LazyLocale, Locale } from '../locale-model';
import { enPack } from './en';
import type { LocaleLoader, LocalePack } from './types';

const lazyLocaleLoaders = {
  ru: () => import('./ru').then((module) => module.default),
  es: () => import('./es').then((module) => module.default),
  fr: () => import('./fr').then((module) => module.default),
  de: () => import('./de').then((module) => module.default),
  ja: () => import('./ja').then((module) => module.default),
  'zh-Hans': () => import('./zh-Hans').then((module) => module.default),
  'zh-Hant': () => import('./zh-Hant').then((module) => module.default),
} satisfies Record<LazyLocale, LocaleLoader>;

const localeLoaders = {
  en: () => Promise.resolve(enPack),
  ...lazyLocaleLoaders,
} satisfies Record<Locale, LocaleLoader>;

export function getLocaleLoaders(): Readonly<Record<Locale, LocaleLoader>> {
  return localeLoaders;
}

export function getFallbackPack(): LocalePack {
  return enPack;
}
