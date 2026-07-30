/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { setLanguageMode } from '@shared/i18n';

import UpdateProgress from './UpdateProgress.svelte';

describe('UpdateProgress', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(async () => {
    await setLanguageMode('en');
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component !== undefined) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
  });

  it('renders a ratio as localized text and accessible progress metadata', async () => {
    component = mount(UpdateProgress, {
      target,
      props: {
        phase: 'downloading',
        progress: {
          ratio: 0.6,
          receivedBytes: 512,
          totalBytes: 1024,
          networkFinished: false,
        },
      },
    });
    flushSync();

    const progress = target.querySelector('[data-slot="progress"]');
    expect(target.textContent).toContain('60%');
    expect(progress?.getAttribute('aria-label')).toBe('Download progress');
    expect(progress?.getAttribute('aria-valuemin')).toBe('0');
    expect(progress?.getAttribute('aria-valuemax')).toBe('1');
    expect(progress?.getAttribute('aria-valuenow')).toBe('0.6');
    expect(progress?.getAttribute('aria-valuetext')).toBe('60%');

    await setLanguageMode('ru');
    flushSync();

    const localizedProgress = target.querySelector('[data-slot="progress"]');
    expect(localizedProgress?.getAttribute('aria-valuetext')).toMatch(/^60\s%$/u);
    expect(localizedProgress?.getAttribute('aria-label')).toBe('Прогресс загрузки');
  });

  it('exposes an indeterminate progress bar without a fabricated value', () => {
    component = mount(UpdateProgress, {
      target,
      props: {
        phase: 'downloading',
        progress: {
          ratio: null,
          receivedBytes: 512,
          totalBytes: null,
          networkFinished: false,
        },
      },
    });
    flushSync();

    const progress = target.querySelector('[data-slot="progress"]');
    expect(progress?.getAttribute('aria-label')).toBe('Download progress');
    expect(progress?.hasAttribute('aria-valuenow')).toBe(false);
    expect(progress?.hasAttribute('aria-valuetext')).toBe(false);
  });
});
