import { describe, expect, it } from 'vitest';

import type { LazyLocale } from '../locale-model';
import { de } from './de';
import { en } from './en';
import { es } from './es';
import { fr } from './fr';
import { ja } from './ja';
import { PLURAL_CATEGORIES, type MessageDictionary, type MessageValue } from './model';
import { ru } from './ru';
import { analyzeMessageTemplate } from './runtime';
import { zhHans } from './zh-Hans';
import { zhHant } from './zh-Hant';

const localizedCatalogs = {
  ru,
  es,
  fr,
  de,
  ja,
  'zh-Hans': zhHans,
  'zh-Hant': zhHant,
} as const satisfies Readonly<Record<LazyLocale, MessageDictionary>>;
const sourceCatalog: MessageDictionary = en;

function placeholders(template: string): string[] {
  const analysis = analyzeMessageTemplate(template);
  if (!analysis.valid) {
    throw new Error(`Invalid message template: ${JSON.stringify(template)}`);
  }
  return [...analysis.placeholders].sort();
}

function without(values: readonly string[], excluded: string): string[] {
  return values.filter((value) => value !== excluded);
}

describe('localized catalog contract', () => {
  for (const [locale, catalog] of Object.entries(localizedCatalogs) as [
    LazyLocale,
    Readonly<Partial<Record<string, MessageValue>>>,
  ][]) {
    it(`${locale} matches the English keys, tags, branches, and placeholders`, () => {
      expect(Object.keys(catalog).sort()).toEqual(Object.keys(sourceCatalog).sort());

      for (const [key, sourceValue] of Object.entries(sourceCatalog)) {
        const candidate = catalog[key];
        if (candidate === undefined) {
          throw new Error(`${locale}.${key} is missing`);
        }

        if (typeof sourceValue === 'string') {
          if (typeof candidate !== 'string') {
            throw new Error(`${locale}.${key} must be a string message`);
          }
          expect(placeholders(candidate)).toEqual(placeholders(sourceValue));
          continue;
        }

        if (typeof candidate === 'string' || candidate.kind !== sourceValue.kind) {
          throw new Error(`${locale}.${key} must use the ${sourceValue.kind} tag`);
        }
        expect(candidate.argument).toBe(sourceValue.argument);

        if (sourceValue.kind === 'plural' && candidate.kind === 'plural') {
          expect(Object.keys(candidate.forms).sort()).toEqual(
            [...PLURAL_CATEGORIES[locale]].sort(),
          );
          for (const [category, template] of Object.entries(candidate.forms)) {
            const sourceTemplate = sourceValue.forms[category] ?? sourceValue.forms.other;
            expect(without(placeholders(template), sourceValue.argument)).toEqual(
              without(placeholders(sourceTemplate), sourceValue.argument),
            );
          }
          continue;
        }

        if (sourceValue.kind === 'select' && candidate.kind === 'select') {
          expect(Object.keys(candidate.cases).sort()).toEqual(
            Object.keys(sourceValue.cases).sort(),
          );
          for (const [caseName, template] of Object.entries(candidate.cases)) {
            expect(placeholders(template)).toEqual(placeholders(sourceValue.cases[caseName]));
          }
        }
      }
    });
  }
});
