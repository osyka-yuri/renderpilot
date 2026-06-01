/**
 * Catalog value shapes.
 *
 * A message is either a plain string (optionally with `{name}` placeholders)
 * or a set of plural forms selected at render time via `Intl.PluralRules`.
 * English uses `one`/`other`; Russian additionally uses `few`/`many`.
 *
 * Keys are flat, dot-separated strings (e.g. `settings.appearance.title`).
 * The English dictionary is the source of truth for the key set, and every
 * other locale is typed as `Record<MessageKey, MessageValue>`, so a missing
 * translation is a compile-time error.
 */

export type PluralForms = {
  one: string;
  few?: string;
  many?: string;
  other: string;
};

export type MessageValue = string | PluralForms;

export type MessageDictionary = Record<string, MessageValue>;

export type InterpolationParams = Record<string, string | number>;
