import { describe, expect, it } from 'vitest';

import { getLocale, setLanguageMode, t, translateExternalMessage } from './index';
import { LAZY_LOCALES } from './locale-model';
import { interpolateMessage } from './messages/runtime';
import { resolveLocale } from './locale';

describe('i18n', () => {
  describe('resolveLocale', () => {
    it('returns the explicit locale for non-system modes', () => {
      expect(resolveLocale('en')).toBe('en');
      expect(resolveLocale('ru')).toBe('ru');
    });

    it('resolves system mode to a supported locale', () => {
      expect(['en', 'ru']).toContain(resolveLocale('system'));
    });
  });

  describe('t', () => {
    it('returns the string for the active locale', async () => {
      await setLanguageMode('en');
      expect(t('nav.games')).toBe('Games');

      await setLanguageMode('ru');
      expect(t('nav.games')).toBe('Игры');
    });

    it('interpolates named parameters', async () => {
      await setLanguageMode('en');
      expect(t('game.card.action.detailsAria', { title: 'Halo' })).toBe('Open details for Halo');
    });

    it('leaves unknown placeholders untouched', async () => {
      await setLanguageMode('en');
      expect(interpolateMessage('Open details for {title}', { other: 'x' })).toBe(
        'Open details for {title}',
      );
    });

    it('selects English plural forms by count', async () => {
      await setLanguageMode('en');
      expect(t('game.dashboard.games', { count: 1 })).toBe('1 game');
      expect(t('game.dashboard.games', { count: 5 })).toBe('5 games');
    });

    it('selects Russian plural forms (one/few/many) by count', async () => {
      await setLanguageMode('ru');
      expect(t('game.dashboard.games', { count: 1 })).toBe('1 игра');
      expect(t('game.dashboard.games', { count: 2 })).toBe('2 игры');
      expect(t('game.dashboard.games', { count: 5 })).toBe('5 игр');
      // 21 → 'one', 11 → 'many': the rule is not just "last digit".
      expect(t('game.dashboard.games', { count: 21 })).toBe('21 игра');
      expect(t('game.dashboard.games', { count: 11 })).toBe('11 игр');
    });

    it('pluralizes the bulk download summary toast', async () => {
      await setLanguageMode('en');
      expect(t('libraries.actions.downloadAllDoneToast', { count: 1 })).toBe(
        'Downloaded 1 library',
      );
      expect(t('libraries.actions.downloadAllDoneToast', { count: 3 })).toBe(
        'Downloaded 3 libraries',
      );

      await setLanguageMode('ru');
      expect(t('libraries.actions.downloadAllDoneToast', { count: 5 })).toBe('Скачано 5 библиотек');
    });
  });

  describe('translateExternalMessage', () => {
    it('translates a known dynamic (backend) key instead of using the fallback', async () => {
      await setLanguageMode('ru');
      const translated = translateExternalMessage({
        key: 'user_message.game_not_in_catalog',
        fallback: 'FALLBACK',
      });

      expect(translated).not.toBe('FALLBACK');
      expect(translated.length).toBeGreaterThan(0);
    });

    it('uses the NVAPI translation in every translated locale and the backend fallback in English', async () => {
      for (const locale of LAZY_LOCALES) {
        await setLanguageMode(locale);
        const label = translateExternalMessage({
          key: 'nvapi.dlss_sr_render_preset.label',
          fallback: 'Render Preset',
        });
        expect(label).not.toBe('Render Preset');
        expect(label.length).toBeGreaterThan(0);
      }

      await setLanguageMode('en');
      expect(
        translateExternalMessage({
          key: 'nvapi.dlss_sr_render_preset.label',
          fallback: 'Render Preset',
        }),
      ).toBe('Render Preset');
    });

    it('uses the Luma message translation in every translated locale and the manifest fallback in English', async () => {
      const key = 'luma.cod-black-ops-3.warning';
      const fallback =
        'Avoid official public matchmaking while Luma is installed. This may result in a ban.';

      for (const locale of LAZY_LOCALES) {
        await setLanguageMode(locale);
        expect(translateExternalMessage({ key, fallback })).not.toBe(fallback);
      }

      await setLanguageMode('en');
      expect(translateExternalMessage({ key, fallback })).toBe(fallback);
    });

    it('fails closed when a known producer changes its source text', async () => {
      await setLanguageMode('ru');
      expect(
        translateExternalMessage({
          key: 'nvapi.dlss_sr_render_preset.label',
          fallback: 'Render Preset (changed upstream)',
        }),
      ).toBe('Render Preset (changed upstream)');
    });

    it('uses the backend fallback for a runtime-only NVAPI key', async () => {
      await setLanguageMode('ja');
      expect(
        translateExternalMessage({
          key: 'nvapi.future_setting.label',
          fallback: 'Future Setting',
        }),
      ).toBe('Future Setting');
    });

    it('keeps parameterized static messages available through the external API', async () => {
      await setLanguageMode('ru');
      const path = 'C:\\recovery\\bundle';
      const translated = translateExternalMessage({
        key: 'addGame.warning.recoveryBundleCreated',
        fallback: 'Recovery bundle: {path}',
        params: { path },
      });
      expect(translated).toContain(path);
      expect(translated).not.toBe(`Recovery bundle: ${path}`);
    });

    it('returns the fallback for an unknown key', async () => {
      await setLanguageMode('en');
      expect(translateExternalMessage({ key: 'does.not.exist', fallback: 'Fallback text' })).toBe(
        'Fallback text',
      );
    });

    it('interpolates the fallback when the key is missing', async () => {
      await setLanguageMode('en');
      expect(
        translateExternalMessage({
          key: 'missing.key',
          fallback: '{action} failed',
          params: { action: 'Save' },
        }),
      ).toBe('Save failed');
    });

    it('returns an invalid external fallback without partial interpolation', async () => {
      await setLanguageMode('en');
      expect(
        translateExternalMessage({
          key: 'missing.key',
          fallback: '{valid} then {bad-name}',
          params: { valid: 'replaced' },
        }),
      ).toBe('{valid} then {bad-name}');
    });
  });

  describe('getLocale', () => {
    it('reflects the active language mode', async () => {
      await setLanguageMode('ru');
      expect(getLocale()).toBe('ru');

      await setLanguageMode('en');
      expect(getLocale()).toBe('en');
    });
  });
});
