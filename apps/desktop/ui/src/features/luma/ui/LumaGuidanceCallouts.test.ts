/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

vi.mock('svelte-sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import { setLanguageMode } from '@shared/i18n';

import LumaGuidanceCalloutsTestHost from './LumaGuidanceCallouts.test-host.svelte';

describe('LumaGuidanceCallouts', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  const writeText = vi.fn<Navigator['clipboard']['writeText']>();

  beforeEach(() => {
    setLanguageMode('en');
    target = document.createElement('div');
    document.body.append(target);
    writeText.mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    component = mount(LumaGuidanceCalloutsTestHost, {
      target,
      props: {
        guidance: [
          {
            id: 'test.ini',
            kind: 'engine_ini',
            fallback_text: 'Set anti-aliasing in Engine.ini.',
            code: '[SystemSettings]\nr.DefaultFeature.AntiAliasing=2',
          },
          {
            id: 'test.warning',
            kind: 'warning',
            fallback_text: 'Do not combine this profile with OptiScaler.',
          },
          {
            id: 'luma.ace-combat-7.engine_ini',
            kind: 'engine_ini',
            fallback_text: 'Apply the following settings manually in Engine.ini.',
            code: '[SystemSettings]\nr.DefaultFeature.AntiAliasing=2',
          },
        ],
      },
    });
    flushSync();
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
    }
    component = undefined;
    target.remove();
    vi.clearAllMocks();
    setLanguageMode('en');
  });

  it('uses reviewed fallback text and a copyable code block', () => {
    expect(target.textContent).toContain('Manual INI change');
    expect(target.textContent).toContain('Set anti-aliasing in Engine.ini.');
    expect(target.textContent).toContain('[SystemSettings]');
    expect(target.textContent).not.toContain('RenderPilot does not change this file');
    expect(target.textContent).not.toContain('Important');
    expect(target.textContent).toContain('Do not combine this profile with OptiScaler.');
  });

  it('renders a localized manifest guidance entry without changing its code', () => {
    setLanguageMode('ru');
    flushSync();

    expect(target.textContent).toContain('Вручную добавьте в Engine.ini следующие настройки.');
    expect(target.textContent).toContain('[SystemSettings]\nr.DefaultFeature.AntiAliasing=2');
  });

  it('copies the exact code', async () => {
    const copyButton = target.querySelector<HTMLButtonElement>('button');
    expect(copyButton?.getAttribute('aria-label')).toBe('Copy');
    expect(copyButton?.textContent).toBe('');
    copyButton?.click();

    await vi.waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('[SystemSettings]\nr.DefaultFeature.AntiAliasing=2');
      expect(copyButton?.getAttribute('aria-label')).toBe('Copied');
    });
  });
});
