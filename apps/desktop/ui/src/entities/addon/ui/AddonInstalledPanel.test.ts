/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import type { AddonStoreView } from '../model/store-view';
import { createInstalledLabels } from '../model/presenters';
import AddonInstalledPanel from './AddonInstalledPanel.svelte';

describe('AddonInstalledPanel', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  type PanelStore = Pick<
    AddonStoreView,
    | 'busy'
    | 'freshness'
    | 'confidence'
    | 'addonDated'
    | 'installedAt'
    | 'lastCheckedAt'
    | 'hostActions'
    | 'hostUpdate'
    | 'addonUpdate'
    | 'updateAvailable'
    | 'checkForUpdates'
    | 'update'
    | 'uninstall'
  >;

  function createFakeStore(overrides: Partial<PanelStore> = {}): PanelStore {
    return {
      busy: false,
      freshness: 'current' as const,
      confidence: null,
      addonDated: null,
      installedAt: null,
      lastCheckedAt: null,
      hostActions: {},
      hostUpdate: null,
      addonUpdate: null,
      updateAvailable: false,
      checkForUpdates: vi.fn(),
      update: vi.fn(),
      uninstall: vi.fn(() => Promise.resolve('ok' as const)),
      ...overrides,
    };
  }

  function render(
    props: {
      busy?: boolean;
      storeOverrides?: Partial<ReturnType<typeof createFakeStore>>;
    } = {},
  ) {
    component = mount(AddonInstalledPanel, {
      target,
      props: {
        gameId: 'game-1',
        store: createFakeStore(props.storeOverrides),
        busy: props.busy ?? false,
        labels: createInstalledLabels('gameDetails.renodx'),
        statusI18nPrefix: 'gameDetails.renodx',
        reshadeDescription: 'ReShade 6.4.0',
        addonDescription: 'RenoDX HDR',
        onRepair: vi.fn(),
      },
    });
    flushSync();
  }

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
  });

  function findCheckUpdatesButton(): HTMLButtonElement {
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find((b) =>
      b.textContent.includes('Check for updates'),
    );
    if (!button) {
      throw new Error('Check for updates button was not found in panel');
    }
    return button;
  }

  it('renders check for updates button with RefreshCwIcon in idle state', () => {
    render();

    const button = findCheckUpdatesButton();
    expect(button.disabled).toBe(false);
    expect(button.getAttribute('aria-busy')).toBe('false');
    expect(button.querySelector('svg.animate-spin')).toBeNull();
    expect(button.querySelector('.lucide-refresh-cw')).not.toBeNull();
    expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('');
  });

  it('spins RefreshCwIcon, sets aria-busy, and preserves stable text during update check', () => {
    render({ storeOverrides: { freshness: 'checking' } });

    const button = findCheckUpdatesButton();
    expect(button.disabled).toBe(true);
    expect(button.getAttribute('aria-busy')).toBe('true');
    expect(button.querySelector('svg.animate-spin')).not.toBeNull();
    expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('Checking…');
  });

  it('disables check button without spinning RefreshCwIcon when external busy is true but not checking', () => {
    render({ busy: true, storeOverrides: { freshness: 'current' } });

    const button = findCheckUpdatesButton();
    expect(button.disabled).toBe(true);
    expect(button.getAttribute('aria-busy')).toBe('false');
    expect(button.querySelector('svg.animate-spin')).toBeNull();
    expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('');
  });
});
