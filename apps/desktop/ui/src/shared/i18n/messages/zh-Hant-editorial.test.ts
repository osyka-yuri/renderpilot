import { describe, expect, it } from 'vitest';

import type { MessageValue } from './model';
import { lumaGuidanceOverrides } from './overrides/luma/zh-Hant';
import { zhHant } from './zh-Hant';

const FORBIDDEN_TERMS = [
  '插件',
  '匹配',
  '封禁',
  '封號',
  '全屏',
  '訪問',
  '單個',
  '驅動器',
  '快捷方式',
  '批處理',
  '列表',
  '跟蹤',
  '宿主',
  '槽位',
  '可執行檔案',
  '重置',
  '不可用',
  '共享',
] as const;

const FORBIDDEN_PUNCTUATION = ['...', '“', '”', '——'] as const;

const TECHNICAL_ONLY_KEYS = new Set([
  'gameDetails.luma.channel.nightly',
  'gameDetails.luma.channel.stable',
  'gameDetails.luma.features.dlssFsr',
  'gameDetails.luma.features.hdr',
  'gameDetails.luma.generic.engineUnity',
  'gameDetails.luma.generic.engineUnreal',
  'gameDetails.luma.title',
  'gameDetails.renodx.channel.nightly',
  'gameDetails.renodx.channel.stable',
  'gameDetails.renodx.component.dlssFix',
  'gameDetails.renodx.title',
  'libraries.documents.formatPdf',
  'settings.catalog.source.gog.title',
  'settings.catalog.source.steam.title',
  'settings.catalog.source.steamgriddb.title',
  'settings.language.de',
  'settings.language.en',
  'settings.language.es',
  'settings.language.fr',
  'settings.tabs.nvidia',
  'settings.tabs.renodx',
]);

const catalogs = [
  ...Object.entries(zhHant),
  ...Object.entries(lumaGuidanceOverrides).map(
    ([key, value]) => [`lumaOverride.${key}`, value] as const,
  ),
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
        for (const term of FORBIDDEN_TERMS) {
          expect(template, `${key}: ${term}`).not.toContain(term);
        }
      }
    }
  });

  it('uses the approved punctuation style', () => {
    for (const [key, value] of catalogs) {
      for (const template of templates(value)) {
        for (const punctuation of FORBIDDEN_PUNCTUATION) {
          expect(template, `${key}: ${punctuation}`).not.toContain(punctuation);
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
          expect(TECHNICAL_ONLY_KEYS.has(key), key).toBe(true);
        }
      }
    }
  });
});
