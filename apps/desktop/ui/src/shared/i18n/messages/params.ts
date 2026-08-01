import type { PluralMessage, SelectMessage } from './model';

type AlphaLower =
  | 'a'
  | 'b'
  | 'c'
  | 'd'
  | 'e'
  | 'f'
  | 'g'
  | 'h'
  | 'i'
  | 'j'
  | 'k'
  | 'l'
  | 'm'
  | 'n'
  | 'o'
  | 'p'
  | 'q'
  | 'r'
  | 's'
  | 't'
  | 'u'
  | 'v'
  | 'w'
  | 'x'
  | 'y'
  | 'z';
type Digit = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9';
type PlaceholderCharacter = AlphaLower | Uppercase<AlphaLower> | Digit | '_';

type ValidTemplate<Placeholders extends string> = Readonly<{
  valid: true;
  placeholders: Placeholders;
}>;
type InvalidTemplate = Readonly<{ valid: false; placeholders: never }>;

type IsPlaceholderName<Name extends string> = Name extends ''
  ? false
  : Name extends `${infer Character}${infer Rest}`
    ? Character extends PlaceholderCharacter
      ? Rest extends ''
        ? true
        : IsPlaceholderName<Rest>
      : false
    : false;

type HasClosingBrace<Text extends string> = Text extends `${string}}${string}` ? true : false;

type ScanTemplate<
  Template extends string,
  Placeholders extends string = never,
> = Template extends `${infer PlainText}{${infer AfterOpeningBrace}`
  ? HasClosingBrace<PlainText> extends true
    ? InvalidTemplate
    : AfterOpeningBrace extends `${infer Name}}${infer Rest}`
      ? IsPlaceholderName<Name> extends true
        ? ScanTemplate<Rest, Placeholders | Name>
        : InvalidTemplate
      : InvalidTemplate
  : HasClosingBrace<Template> extends true
    ? InvalidTemplate
    : ValidTemplate<Placeholders>;

export type MessageTemplateAnalysis<Template extends string> = string extends Template
  ? Readonly<{ valid: boolean; placeholders: string }>
  : ScanTemplate<Template>;

export type IsValidMessageTemplate<Template extends string> =
  MessageTemplateAnalysis<Template>['valid'];

export type MessagePlaceholders<Template extends string> =
  MessageTemplateAnalysis<Template> extends ValidTemplate<infer Placeholders>
    ? Placeholders
    : never;

export type IsMessageParameterName<Value extends string> =
  MessageTemplateAnalysis<`{${Value}}`> extends ValidTemplate<Value> ? true : false;

type Simplify<Value> = { [Key in keyof Value]: Value[Key] } & {};

type ParamsFromPlaceholders<Names extends string> = [Names] extends [never]
  ? Readonly<Record<never, never>>
  : Readonly<Record<Names, string | number>>;

type TemplatesIn<Value extends Readonly<Record<string, string>>> = Value[keyof Value];

export type ParamsForMessage<Value> = Value extends string
  ? ParamsFromPlaceholders<MessagePlaceholders<Value>>
  : Value extends PluralMessage<
        infer Argument,
        infer Forms extends Readonly<Record<string, string>>
      >
    ? Simplify<
        ParamsFromPlaceholders<MessagePlaceholders<TemplatesIn<Forms>>> &
          Readonly<Record<Argument, number>>
      >
    : Value extends SelectMessage<
          infer Argument,
          infer Cases extends Readonly<Record<string, string>>
        >
      ? Simplify<
          ParamsFromPlaceholders<MessagePlaceholders<TemplatesIn<Cases>>> &
            Readonly<Record<Argument, Extract<Exclude<keyof Cases, 'other'>, string>>>
        >
      : never;

export type ExactMessageParams<Expected, Candidate extends Expected> = Candidate &
  Readonly<Record<Exclude<keyof Candidate, keyof Expected>, never>>;
