import type { Locale } from '../locale';

export type InterpolationValue = string | number;
export type InterpolationParams = Readonly<Record<string, InterpolationValue>>;

export type PluralMessage<
  Argument extends string = string,
  Forms extends Readonly<Record<string, string>> = Readonly<Record<string, string>>,
> = Readonly<{
  kind: 'plural';
  argument: Argument;
  forms: Forms;
}>;

export type SelectMessage<
  Argument extends string = string,
  Cases extends Readonly<Record<string, string>> = Readonly<Record<string, string>>,
> = Readonly<{
  kind: 'select';
  argument: Argument;
  cases: Cases;
}>;

export type MessageValue = string | PluralMessage | SelectMessage;
export type MessageDictionary = Readonly<Record<string, MessageValue>>;
export type MessageOverrides = Readonly<Partial<Record<string, MessageValue>>>;

export const PLURAL_CATEGORIES = {
  en: ['one', 'other'],
  ru: ['one', 'few', 'many', 'other'],
  es: ['one', 'many', 'other'],
  fr: ['one', 'many', 'other'],
  de: ['one', 'other'],
  ja: ['other'],
  zh: ['other'],
} as const satisfies Readonly<Record<Locale, readonly Intl.LDMLPluralRule[]>>;

export type PluralCategoryFor<CurrentLocale extends Locale> =
  (typeof PLURAL_CATEGORIES)[CurrentLocale][number];

export function plural<
  const Argument extends string,
  const Forms extends Readonly<Record<string, string>>,
>(argument: Argument, forms: Forms): PluralMessage<Argument, Forms> {
  return { kind: 'plural', argument, forms };
}

type SelectCases = Readonly<Record<'other', string>> & Readonly<Record<string, string>>;

export function select<const Argument extends string, const Cases extends SelectCases>(
  argument: Argument,
  cases: Cases,
): SelectMessage<Argument, Cases> {
  return { kind: 'select', argument, cases };
}
