import { afterEach, describe, expect, it } from 'vitest';

import { getLocale, setLanguageMode, t, translateKey } from './index';
import { resolveLocale } from './locale';

// The shared test setup pins the locale to 'en'; restore it after each case so
// locale changes here never leak into other suites running in this worker.
afterEach(() => {
  setLanguageMode('en');
});

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
    it('returns the string for the active locale', () => {
      setLanguageMode('en');
      expect(t('nav.games')).toBe('Games');

      setLanguageMode('ru');
      expect(t('nav.games')).toBe('Игры');
    });

    it('interpolates named parameters', () => {
      setLanguageMode('en');
      expect(t('game.card.action.detailsAria', { title: 'Halo' })).toBe('Open details for Halo');
    });

    it('leaves unknown placeholders untouched', () => {
      setLanguageMode('en');
      expect(t('game.card.action.detailsAria', { other: 'x' })).toBe('Open details for {title}');
    });

    it('selects English plural forms by count', () => {
      setLanguageMode('en');
      expect(t('game.dashboard.games', { count: 1 })).toBe('1 game');
      expect(t('game.dashboard.games', { count: 5 })).toBe('5 games');
    });

    it('selects Russian plural forms (one/few/many) by count', () => {
      setLanguageMode('ru');
      expect(t('game.dashboard.games', { count: 1 })).toBe('1 игра');
      expect(t('game.dashboard.games', { count: 2 })).toBe('2 игры');
      expect(t('game.dashboard.games', { count: 5 })).toBe('5 игр');
      // 21 → 'one', 11 → 'many': the rule is not just "last digit".
      expect(t('game.dashboard.games', { count: 21 })).toBe('21 игра');
      expect(t('game.dashboard.games', { count: 11 })).toBe('11 игр');
    });
  });

  describe('translateKey', () => {
    it('translates a known dynamic (backend) key instead of using the fallback', () => {
      setLanguageMode('ru');
      const translated = translateKey('user_message.game_not_in_catalog', 'FALLBACK');

      expect(translated).not.toBe('FALLBACK');
      expect(translated.length).toBeGreaterThan(0);
    });

    it('uses the NVAPI override for ru and the backend fallback for en', () => {
      setLanguageMode('ru');
      const ruLabel = translateKey('nvapi.dlss_sr_render_preset.label', 'Render Preset');
      expect(ruLabel).not.toBe('Render Preset');
      expect(ruLabel.length).toBeGreaterThan(0);

      // English is intentionally omitted from the overrides, so the caller's
      // fallback (the backend dlss_settings.json text) is used.
      setLanguageMode('en');
      expect(translateKey('nvapi.dlss_sr_render_preset.label', 'Render Preset')).toBe(
        'Render Preset',
      );
    });

    it('returns the fallback for an unknown key', () => {
      setLanguageMode('en');
      expect(translateKey('does.not.exist', 'Fallback text')).toBe('Fallback text');
    });

    it('interpolates the fallback when the key is missing', () => {
      setLanguageMode('en');
      expect(translateKey('missing.key', '{action} failed', { action: 'Save' })).toBe(
        'Save failed',
      );
    });
  });

  describe('getLocale', () => {
    it('reflects the active language mode', () => {
      setLanguageMode('ru');
      expect(getLocale()).toBe('ru');

      setLanguageMode('en');
      expect(getLocale()).toBe('en');
    });
  });
});
