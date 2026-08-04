import { parseLocaleTag } from './locale-tag';

export type IntlFormatterProvider<TFormatter> = (locale: string) => TFormatter;

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

export function createDurationFormatter(
  options: Readonly<Intl.DurationFormatOptions> = {},
): IntlFormatterProvider<Intl.DurationFormat> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.DurationFormat(locale, optionsSnapshot);
  });
}

export function createPluralRules(
  options: Readonly<Intl.PluralRulesOptions> = {},
): IntlFormatterProvider<Intl.PluralRules> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.PluralRules(locale, optionsSnapshot);
  });
}

export function createSegmenter(
  options: Readonly<Intl.SegmenterOptions> = {},
): IntlFormatterProvider<Intl.Segmenter> {
  return createFormatterProvider(options, (locale, optionsSnapshot) => {
    return new Intl.Segmenter(locale, optionsSnapshot);
  });
}

function createFormatterProvider<TOptions extends object, TFormatter>(
  options: Readonly<TOptions>,
  createFormatter: (locale: string, options: Readonly<TOptions>) => TFormatter,
): IntlFormatterProvider<TFormatter> {
  const optionsSnapshot: Readonly<TOptions> = Object.freeze({ ...options });
  const formatters = new Map<string, TFormatter>();

  return (locale) => {
    const canonicalLocale = parseLocaleTag(locale).tag;
    const cached = formatters.get(canonicalLocale);
    if (cached !== undefined) {
      return cached;
    }

    const formatter = createFormatter(canonicalLocale, optionsSnapshot);
    formatters.set(canonicalLocale, formatter);
    return formatter;
  };
}
