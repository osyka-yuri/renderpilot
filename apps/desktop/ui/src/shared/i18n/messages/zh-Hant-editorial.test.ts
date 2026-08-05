import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

import type { MessageValue } from './model';
import { lumaOverrides as lumaOverridesZhHans } from './overrides/luma/zh-Hans';
import { lumaOverrides } from './overrides/luma/zh-Hant';
import { nvapiOverrides as nvapiOverridesZhHans } from './overrides/nvapi/zh-Hans';
import { nvapiOverrides } from './overrides/nvapi/zh-Hant';
import { zhHans } from './zh-Hans';
import { zhHant } from './zh-Hant';

type ChineseLocale = 'zh-Hans' | 'zh-Hant';
type EditorialPolicy = {
  technicalOnlyStaticKeys: string[];
  nvapiVerbatimValues: string[];
  chineseScriptRules: Record<ChineseLocale, { forbiddenTerms: string[] }>;
};

const policy = JSON.parse(
  readFileSync(new URL('../../../../../data/i18n-editorial-policy.json', import.meta.url), 'utf8'),
) as EditorialPolicy;
const technicalOnlyKeys = new Set(policy.technicalOnlyStaticKeys);
const nvapiVerbatimValues = new Set(policy.nvapiVerbatimValues);

const catalogs = [
  ...Object.entries(zhHant),
  ...Object.entries(lumaOverrides).map(([key, value]) => [`lumaOverride.${key}`, value] as const),
  ...Object.entries(nvapiOverrides).map(([key, value]) => [`nvapiOverride.${key}`, value] as const),
] as const;

const zhHansCatalogs = [
  ...Object.entries(zhHans),
  ...Object.entries(lumaOverridesZhHans),
  ...Object.entries(nvapiOverridesZhHans),
] as const;

function templates(value: MessageValue): readonly string[] {
  if (typeof value === 'string') {
    return [value];
  }
  return Object.values(value.kind === 'plural' ? value.forms : value.cases);
}

describe('Traditional Chinese editorial contract', () => {
  it('uses the approved neutral Traditional Chinese terminology', () => {
    for (const [key, value] of catalogs) {
      for (const template of templates(value)) {
        for (const term of policy.chineseScriptRules['zh-Hant'].forbiddenTerms) {
          expect(template, `${key}: ${term}`).not.toContain(term);
        }
      }
    }
  });

  it('does not leave untranslated prose outside deliberate technical labels', () => {
    for (const [key, value] of catalogs) {
      for (const template of templates(value)) {
        const prose = template.replace(/\{[^}]+\}/gu, '');
        const containsLatinWord = /[A-Za-z]{2,}/u.test(prose);
        const containsHanText = /\p{Script=Han}/u.test(prose);
        if (containsLatinWord && !containsHanText) {
          expect(technicalOnlyKeys.has(key) || nvapiVerbatimValues.has(template), key).toBe(true);
        }
      }
    }
  });
});

describe('Simplified Chinese editorial contract', () => {
  it('does not mix in policy-forbidden Traditional Chinese terms', () => {
    for (const [key, value] of zhHansCatalogs) {
      for (const template of templates(value)) {
        for (const term of policy.chineseScriptRules['zh-Hans'].forbiddenTerms) {
          expect(template, `${key}: ${term}`).not.toContain(term);
        }
      }
    }
  });
});
