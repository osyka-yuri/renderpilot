export type IntlFormatterProvider<TFormatter> = (locale: string) => TFormatter;

const canonicalLocales = new Map<string, string>();

export function createNumberFormatter(
  options: Readonly<Intl.NumberFormatOptions> = {},
): IntlFormatterProvider<Intl.NumberFormat> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.NumberFormat(locale, optionsSnapshot);
  });
}

export function createDateTimeFormatter(
  options: Readonly<Intl.DateTimeFormatOptions> = {},
): IntlFormatterProvider<Intl.DateTimeFormat> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.DateTimeFormat(locale, optionsSnapshot);
  });
}

export function createRelativeTimeFormatter(
  options: Readonly<Intl.RelativeTimeFormatOptions> = {},
): IntlFormatterProvider<Intl.RelativeTimeFormat> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.RelativeTimeFormat(locale, optionsSnapshot);
  });
}

export function createListFormatter(
  options: Readonly<Intl.ListFormatOptions> = {},
): IntlFormatterProvider<Intl.ListFormat> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.ListFormat(locale, optionsSnapshot);
  });
}

export function createPluralRules(
  options: Readonly<Intl.PluralRulesOptions> = {},
): IntlFormatterProvider<Intl.PluralRules> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.PluralRules(locale, optionsSnapshot);
  });
}

function createFormatterProvider<TOptions extends object, TFormatter>(
  options: Readonly<TOptions>,
  createFormatter: (locale: string, options: Readonly<TOptions>) => TFormatter,
): IntlFormatterProvider<TFormatter> {
  const optionsSnapshot: Readonly<TOptions> = Object.freeze({ ...options });
  const formatters = new Map<string, TFormatter>();

  return (locale) => {
    const canonicalLocale = canonicalizeLocale(locale);
    const cached = formatters.get(canonicalLocale);
    if (cached !== undefined) {
      return cached;
    }

    const formatter = createFormatter(canonicalLocale, optionsSnapshot);
    formatters.set(canonicalLocale, formatter);
    return formatter;
  };
}

function canonicalizeLocale(locale: string): string {
  const cached = canonicalLocales.get(locale);
  if (cached !== undefined) {
    return cached;
  }

  const [canonicalLocale] = Intl.getCanonicalLocales(locale);
  canonicalLocales.set(locale, canonicalLocale);
  return canonicalLocale;
}
