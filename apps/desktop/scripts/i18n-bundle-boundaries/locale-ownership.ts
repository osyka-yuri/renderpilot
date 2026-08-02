import { LAZY_LOCALES, type LazyLocale } from '../../ui/src/shared/i18n/locale-model.ts';

export const PACK_ROOT = '/ui/src/shared/i18n/packs/';
export const MESSAGE_ROOT = '/ui/src/shared/i18n/messages/';

const OVERRIDE_ROOTS = [
  `${MESSAGE_ROOT}overrides/luma/`,
  `${MESSAGE_ROOT}overrides/nvapi/`,
] as const;

const LOCALE_MODULE_SUFFIXES = new Map(
  LAZY_LOCALES.map((locale) => [locale, localeModuleSuffixes(locale)] as const),
);

export function localeModuleOwner(moduleId: string): LazyLocale | null {
  const normalizedModuleId = normalize(moduleId);
  for (const [locale, suffixes] of LOCALE_MODULE_SUFFIXES) {
    if (suffixes.some((suffix) => normalizedModuleId.endsWith(suffix))) {
      return locale;
    }
  }
  return null;
}

export function isI18nOverrideModule(moduleId: string): boolean {
  const normalizedModuleId = normalize(moduleId);
  return OVERRIDE_ROOTS.some((root) => normalizedModuleId.includes(root));
}

function localeModuleSuffixes(locale: LazyLocale): readonly string[] {
  return [
    `${PACK_ROOT}${locale}.ts`,
    `${MESSAGE_ROOT}${locale}.ts`,
    ...OVERRIDE_ROOTS.map((root) => `${root}${locale}.ts`),
  ];
}

function normalize(value: string): string {
  return value.replace(/\\/g, '/');
}
