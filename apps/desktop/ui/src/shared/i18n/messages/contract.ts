import type { Locale } from '../locale';
import type {
  MessageDictionary,
  MessageOverrides,
  PluralCategoryFor,
  PluralMessage,
  SelectMessage,
} from './model';
import type { IsMessageParameterName, IsValidMessageTemplate, MessagePlaceholders } from './params';

type ExactKeys<Actual, Expected> =
  Exclude<keyof Actual, keyof Expected> extends never
    ? Exclude<keyof Expected, keyof Actual> extends never
      ? true
      : false
    : false;
type SameUnion<Left, Right> = [Left] extends [Right]
  ? [Right] extends [Left]
    ? true
    : false
  : false;

type SourcePluralForms = Readonly<Record<'one' | 'other', string>>;

type ValidateTemplates<Templates extends Readonly<Record<string, string>>> = {
  readonly [Key in keyof Templates]: IsValidMessageTemplate<Templates[Key]> extends true
    ? Templates[Key]
    : never;
};

type ValidateSourceValue<Value> = Value extends string
  ? IsValidMessageTemplate<Value> extends true
    ? Value
    : never
  : Value extends PluralMessage<infer Argument, infer Forms>
    ? IsMessageParameterName<Argument> extends true
      ? ExactKeys<Forms, SourcePluralForms> extends true
        ? Value & { readonly forms: ValidateTemplates<Forms> }
        : never
      : never
    : Value extends SelectMessage<infer Argument, infer Cases>
      ? IsMessageParameterName<Argument> extends true
        ? 'other' extends keyof Cases
          ? Exclude<keyof Cases, 'other'> extends never
            ? never
            : Value & { readonly cases: ValidateTemplates<Cases> }
          : never
        : never
      : never;

type ValidateSourceCatalog<Catalog extends MessageDictionary> = {
  readonly [Key in keyof Catalog]: ValidateSourceValue<Catalog[Key]>;
};

export function defineSourceCatalog<const Catalog extends MessageDictionary>(
  catalog: Catalog & ValidateSourceCatalog<Catalog>,
): Catalog {
  return catalog;
}

type LocalizedPluralForms<CurrentLocale extends Locale> = Readonly<
  Record<PluralCategoryFor<CurrentLocale>, string>
>;

type LocalizedMessage<CurrentLocale extends Locale, SourceValue> = SourceValue extends string
  ? string
  : SourceValue extends PluralMessage<infer Argument>
    ? PluralMessage<Argument, LocalizedPluralForms<CurrentLocale>>
    : SourceValue extends SelectMessage<
          infer Argument,
          infer Cases extends Readonly<Record<string, string>>
        >
      ? SelectMessage<Argument, Readonly<Record<Extract<keyof Cases, string>, string>>>
      : never;

export type LocalizedCatalog<
  CurrentLocale extends Locale,
  Source extends MessageDictionary,
> = Readonly<{
  [Key in keyof Source]: LocalizedMessage<CurrentLocale, Source[Key]>;
}>;

type SourcePluralTemplate<
  Forms extends Readonly<Record<string, string>>,
  Category extends string,
> = Category extends keyof Forms ? Forms[Category] : Forms['other'];

type ValidatePluralForms<
  Argument extends string,
  SourceForms extends Readonly<Record<string, string>>,
  CandidateForms extends Readonly<Record<string, string>>,
> = {
  readonly [Category in keyof CandidateForms]: Category extends string
    ? IsValidMessageTemplate<CandidateForms[Category]> extends true
      ? SameUnion<
          Exclude<MessagePlaceholders<SourcePluralTemplate<SourceForms, Category>>, Argument>,
          Exclude<MessagePlaceholders<CandidateForms[Category]>, Argument>
        > extends true
        ? CandidateForms[Category]
        : never
      : never
    : never;
};

type ValidateSelectCases<
  SourceCases extends Readonly<Record<string, string>>,
  CandidateCases extends Readonly<Record<string, string>>,
> = {
  readonly [Case in keyof CandidateCases]: Case extends keyof SourceCases
    ? IsValidMessageTemplate<CandidateCases[Case]> extends true
      ? SameUnion<
          MessagePlaceholders<SourceCases[Case]>,
          MessagePlaceholders<CandidateCases[Case]>
        > extends true
        ? CandidateCases[Case]
        : never
      : never
    : never;
};

type ValidateLocalizedValue<
  SourceValue,
  CandidateValue,
  CurrentLocale extends Locale,
> = SourceValue extends string
  ? CandidateValue extends string
    ? IsValidMessageTemplate<CandidateValue> extends true
      ? SameUnion<
          MessagePlaceholders<SourceValue>,
          MessagePlaceholders<CandidateValue>
        > extends true
        ? CandidateValue
        : never
      : never
    : never
  : SourceValue extends PluralMessage<
        infer SourceArgument,
        infer SourceForms extends Readonly<Record<string, string>>
      >
    ? CandidateValue extends PluralMessage<
        infer CandidateArgument,
        infer CandidateForms extends Readonly<Record<string, string>>
      >
      ? CandidateArgument extends SourceArgument
        ? SourceArgument extends CandidateArgument
          ? ExactKeys<CandidateForms, LocalizedPluralForms<CurrentLocale>> extends true
            ? CandidateValue & {
                readonly forms: ValidatePluralForms<SourceArgument, SourceForms, CandidateForms>;
              }
            : never
          : never
        : never
      : never
    : SourceValue extends SelectMessage<
          infer SourceArgument,
          infer SourceCases extends Readonly<Record<string, string>>
        >
      ? CandidateValue extends SelectMessage<
          infer CandidateArgument,
          infer CandidateCases extends Readonly<Record<string, string>>
        >
        ? CandidateArgument extends SourceArgument
          ? SourceArgument extends CandidateArgument
            ? ExactKeys<CandidateCases, SourceCases> extends true
              ? CandidateValue & {
                  readonly cases: ValidateSelectCases<SourceCases, CandidateCases>;
                }
              : never
            : never
          : never
        : never
      : never;

type ValidateLocalizedCatalog<
  CurrentLocale extends Locale,
  Source extends MessageDictionary,
  Candidate extends MessageDictionary,
> = Readonly<{
  [Key in keyof Source]: Key extends keyof Candidate
    ? ValidateLocalizedValue<Source[Key], Candidate[Key], CurrentLocale>
    : never;
}> &
  Readonly<Record<Exclude<keyof Candidate, keyof Source>, never>>;

export function defineLocalizedCatalog<
  CurrentLocale extends Locale,
  Source extends MessageDictionary,
>(): <const Candidate extends MessageDictionary>(
  catalog: Candidate & ValidateLocalizedCatalog<CurrentLocale, Source, Candidate>,
) => Candidate {
  return (catalog) => catalog;
}

export type LocalizedOverrides<
  CurrentLocale extends Locale,
  Source extends MessageDictionary,
> = Readonly<Partial<LocalizedCatalog<CurrentLocale, Source>>>;

type ValidateLocalizedOverrides<
  CurrentLocale extends Locale,
  Source extends MessageDictionary,
  Candidate extends MessageOverrides,
> = {
  readonly [Key in keyof Candidate]: Key extends keyof Source
    ? ValidateLocalizedValue<Source[Key], Candidate[Key], CurrentLocale>
    : never;
};

export function defineLocalizedOverrides<
  CurrentLocale extends Locale,
  Source extends MessageDictionary,
>(): <const Candidate extends MessageOverrides>(
  catalog: Candidate & ValidateLocalizedOverrides<CurrentLocale, Source, Candidate>,
) => Candidate {
  return (catalog) => catalog;
}
