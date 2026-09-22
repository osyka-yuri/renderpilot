/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import type { OptiScalerApi } from '../api/desktop';
import { createOptiScalerStore } from '../model/create-optiscaler-store.svelte';
import type { OptiScalerAvailability, OptiScalerOperationResult } from '../model/types';
import {
  buildOptiScalerAvailability,
  buildOptiScalerOperationResult,
} from '../model/optiscaler-test-fixtures';
import OptiScalerCardTestHost from './OptiScalerCard.test-host.svelte';

const gameId = 'steam:optiscaler-card';

function module(id: string, selected = false) {
  return {
    id,
    selected,
    optional: id !== 'core',
    available: true,
    requires: id === 'core' ? [] : ['core'],
    conflicts: [],
    description: `Technical description for ${id}`,
  };
}

function report(overrides: Partial<OptiScalerAvailability> = {}): OptiScalerAvailability {
  const base = buildOptiScalerAvailability({
    game_id: gameId,
    selected_release: 'v0.9.3',
    compatibility: {
      status: 'untested',
      declared_inputs: ['dlss2_plus'],
      launch: null,
      guidance: [],
    },
    modules: [module('core', true), module('ffx_dx12', true)],
    install: {
      installed: true,
      release: 'v0.9.3',
    },
  });
  return { ...base, ...overrides };
}

function operation(): OptiScalerOperationResult {
  return buildOptiScalerOperationResult();
}

function api(current: OptiScalerAvailability): OptiScalerApi {
  return {
    availability: vi.fn(() => Promise.resolve(current)),
    checkUpdate: vi.fn(() =>
      Promise.resolve({
        overall: 'current' as const,
        installed_release: current.install.release,
        available_release: current.selected_release,
        update_available: false,
        repair_required: current.lifecycle.repair_required,
        drifted: current.lifecycle.drifted,
      }),
    ),
    install: vi.fn(() => Promise.resolve(operation())),
    update: vi.fn(() => Promise.resolve(operation())),
    repair: vi.fn(() => Promise.resolve(operation())),
    setModules: vi.fn(() => Promise.resolve(operation())),
    relocate: vi.fn(() => Promise.resolve(operation())),
    uninstall: vi.fn(() => Promise.resolve(operation())),
  };
}

function findButton(label: string, root: ParentNode = document): HTMLButtonElement | undefined {
  return [...root.querySelectorAll<HTMLButtonElement>('button')].find(
    (button) => button.textContent.trim() === label,
  );
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

describe('OptiScalerCard', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

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

  async function render(current: OptiScalerAvailability) {
    const currentApi = api(current);
    const store = createOptiScalerStore({ api: currentApi });
    await store.load(gameId);
    component = mount(OptiScalerCardTestHost, {
      target,
      props: { gameId, store },
    });
    flushSync();
    return { currentApi, store };
  }

  it('shows version, update check, and repair only when repair is required', async () => {
    await render(report());

    expect(target.textContent).toContain('v0.9.3');
    expect(target.textContent).toContain('Integrates through');
    expect(target.textContent).toContain('DLSS 2+');
    expect(target.textContent).not.toContain('Detected in the game');
    expect(findButton('Check for updates', target)).toBeDefined();
    expect(findButton('Repair', target)).toBeUndefined();

    if (!component) {
      throw new Error('OptiScalerCard was not mounted');
    }
    await unmount(component);
    component = undefined;
    target.replaceChildren();

    await render(
      report({
        lifecycle: {
          update_available: false,
          repair_required: true,
          drifted: true,
          unmanaged: false,
          maintenance_available: true,
          maintenance_block_code: null,
        },
      }),
    );
    expect(findButton('Repair', target)).toBeDefined();
    expect(target.textContent).not.toContain('OptiScaler.ini');
  });

  it('displays compatibility badge alongside installed status for an installed card', async () => {
    await render(
      report({
        compatibility: {
          status: 'working',
          declared_inputs: ['dlss2_plus'],
          launch: null,
          guidance: [],
        },
      }),
    );

    expect(target.textContent).toContain('Installed');
    expect(target.textContent).toContain('Confirmed');
  });

  it('keeps raw component identifiers out of the card and explains them in settings', async () => {
    await render(report({ install: { installed: false, release: null } }));

    expect(target.textContent).not.toContain('ffx_dx12');
    expect(target.textContent).not.toContain('Technical description');

    findButton('Configure components', target)?.click();
    flushSync();

    expect(document.body.textContent).toContain('AMD FSR for DirectX 12');
    expect(document.body.textContent).toContain('Adds AMD upscaling and frame-generation support');
    expect(document.body.textContent).not.toContain('Technical details');
    const selectedRow = document
      .querySelector('#optiscaler-steam-optiscaler-card-module-ffx_dx12')
      ?.closest('label');
    expect(selectedRow?.className).not.toContain('bg-primary');
  });

  it('keeps an unknown configuration without detected input hard blocked', async () => {
    const blocked = report({
      install: { installed: false, release: null },
      eligibility: {
        available: false,
        block_code: 'input_not_detected',
      },
    });
    await render(blocked);

    expect(target.textContent).toContain('Unverified');
    expect(target.textContent).toContain('v0.9.3');
    expect(findButton('Install', target)?.disabled).toBe(true);
    expect(document.querySelector('[role="dialog"][data-state="open"]')).toBeNull();
  });

  it('installs a directly eligible configuration without confirmation', async () => {
    const currentApi = api(
      report({
        install: { installed: false, release: null },
        eligibility: { available: true, block_code: null },
        compatibility: {
          status: 'untested',
          declared_inputs: ['fsr2_plus'],
          launch: null,
          guidance: [],
        },
      }),
    );
    const store = createOptiScalerStore({ api: currentApi });
    await store.load(gameId);
    component = mount(OptiScalerCardTestHost, {
      target,
      props: { gameId, store },
    });
    flushSync();

    findButton('Install', target)?.click();
    await vi.waitFor(() => {
      expect(currentApi.install).toHaveBeenCalledWith(gameId, ['core', 'ffx_dx12']);
    });
    expect(document.querySelector('[role="dialog"][data-state="open"]')).toBeNull();
  });

  it('renders reviewed compatibility guidance through its stable localized message id', async () => {
    await render(
      report({
        compatibility: {
          status: 'working',
          declared_inputs: [],
          launch: null,
          guidance: [
            {
              kind: 'compatibility',
              message: {
                id: 'optiscaler-jedi-survivor-restart',
                fallback_text: 'Untrusted remote text must not be displayed.',
              },
            },
          ],
        },
      }),
    );

    expect(target.textContent).toContain(
      'If DLSS is unavailable after changing OptiScaler settings, restart the game.',
    );
    expect(target.textContent).not.toContain('Untrusted remote text must not be displayed.');
  });

  it('renders typed launch policy as a copyable launcher-aware callout', async () => {
    await render(
      report({
        launcher: 'Steam',
        compatibility: {
          status: 'working',
          declared_inputs: [],
          launch: { arguments: ['-dx12'], requirement: 'required' },
          guidance: [],
        },
      }),
    );

    expect(target.textContent).toContain('Launch arguments required');
    expect(target.textContent).toContain('-dx12');
    expect(target.textContent).toContain('right-click the game');
    expect(target.querySelector('button[aria-label="Copy arguments"]')).not.toBeNull();
  });

  it('presents a catalog conditional separately from an unverified configuration', async () => {
    await render(
      report({
        install: { installed: false, release: null },
        compatibility: { status: 'conditional', declared_inputs: [], launch: null, guidance: [] },
      }),
    );

    expect(target.textContent).toContain('Conditional');
    expect(findButton('Install', target)?.disabled).toBe(false);
  });

  it('keeps a disabled Install action visible with the next Luma prerequisite step as an informational callout', async () => {
    await render(
      report({
        install: { installed: false, release: null },
        prerequisite: { state: 'install_luma' },
      }),
    );

    expect(target.textContent).toContain(
      'Install Luma Framework before installing OptiScaler for this configuration.',
    );
    expect(findButton('Install', target)?.disabled).toBe(true);
    expect(target.querySelector('.lucide-info')).not.toBeNull();
    expect(target.querySelector('.lucide-triangle-alert')).toBeNull();
    expect(target.querySelector('[role="status"]')?.className).not.toContain('text-warning');
  });

  it('renders conflicting prerequisites as a warning callout', async () => {
    await render(
      report({
        install: { installed: false, release: null },
        prerequisite: { state: 'remove_reno_dx' },
      }),
    );

    expect(target.textContent).toContain(
      'Remove RenoDX before installing Luma Framework for this OptiScaler configuration.',
    );
    expect(findButton('Install', target)?.disabled).toBe(true);
    expect(target.querySelector('.lucide-triangle-alert')).not.toBeNull();
    expect(target.querySelector('.lucide-info')).toBeNull();
    expect(target.querySelector('[role="status"]')?.className).toContain('text-warning');
  });

  it('preserves catalog compatibility status while installation is blocked by prerequisite', async () => {
    await render(
      report({
        install: { installed: false, release: null },
        compatibility: { status: 'working', declared_inputs: [], launch: null, guidance: [] },
        prerequisite: { state: 'install_luma' },
      }),
    );

    expect(target.textContent).toContain('Confirmed');
    expect(target.textContent).not.toContain('Unavailable');
    expect(findButton('Install', target)?.disabled).toBe(true);
  });

  it('keeps a disabled Install action visible when installation is blocked by proxy conflict', async () => {
    await render(
      report({
        install: { installed: false, release: null },
        proxy_conflict: 'dxgi.dll',
      }),
    );

    expect(findButton('Install', target)).toBeDefined();
    expect(findButton('Install', target)?.disabled).toBe(true);
  });

  it('presents a lifecycle maintenance block without enabling managed mutations', async () => {
    await render(
      report({
        lifecycle: {
          update_available: false,
          repair_required: true,
          drifted: false,
          unmanaged: false,
          maintenance_available: false,
          maintenance_block_code: 'catalog_unsupported',
        },
      }),
    );

    expect(target.textContent).toContain('This game is marked as incompatible with OptiScaler.');
    expect(findButton('Repair', target)?.disabled).toBe(true);
    expect(findButton('Configure', target)?.disabled).toBe(true);
  });

  it('does not offer an install mutation for an unmanaged installation', async () => {
    await render(
      report({
        install: { installed: false, release: null },
        lifecycle: {
          update_available: false,
          repair_required: false,
          drifted: false,
          unmanaged: true,
          maintenance_available: false,
          maintenance_block_code: null,
        },
      }),
    );

    expect(findButton('Install', target)).toBeUndefined();
    expect(target.textContent).toContain('left unchanged');
  });

  it('keeps the install action visible and presents its in-flight state', async () => {
    const current = report({ install: { installed: false, release: null } });
    const operationResult = deferred<OptiScalerOperationResult>();
    const currentApi = api(current);
    currentApi.install = vi.fn(() => operationResult.promise);
    const store = createOptiScalerStore({ api: currentApi });
    await store.load(gameId);
    component = mount(OptiScalerCardTestHost, {
      target,
      props: { gameId, store },
    });
    flushSync();

    findButton('Install', target)?.click();
    await vi.waitFor(() => {
      expect(currentApi.install).toHaveBeenCalledOnce();
      const installing = findButton('Installing…', target);
      expect(installing?.disabled).toBe(true);
      expect(installing?.querySelector('svg.animate-spin')).not.toBeNull();
    });

    operationResult.resolve(operation());
    await vi.waitFor(() => {
      expect(store.busy).toBe(false);
    });
  });

  it('keeps the project link and actions in the card footer area', async () => {
    await render(report());

    expect(target.textContent).toContain('OptiScaler by cdozdil.');
    expect(target.querySelector<HTMLAnchorElement>('a')?.href).toBe(
      'https://github.com/optiscaler/OptiScaler',
    );
    expect(findButton('Configure', target)).toBeDefined();
    expect(findButton('Check for updates', target)).toBeDefined();
  });

  it('keeps check for updates label stable, disables button, and spins RefreshCwIcon while checking', async () => {
    const current = report();
    const checkDeferred = deferred<{
      overall: 'current';
      installed_release: null;
      available_release: null;
      update_available: boolean;
      repair_required: boolean;
      drifted: boolean;
    }>();
    const currentApi = api(current);
    currentApi.checkUpdate = vi.fn(() => checkDeferred.promise);
    const store = createOptiScalerStore({ api: currentApi });
    await store.load(gameId);
    component = mount(OptiScalerCardTestHost, {
      target,
      props: { gameId, store },
    });
    flushSync();

    const checkButton = findButton('Check for updates', target);
    expect(checkButton).toBeDefined();
    expect(checkButton?.disabled).toBe(false);
    expect(checkButton?.querySelector('svg.animate-spin')).toBeNull();
    expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('');

    checkButton?.click();
    await vi.waitFor(() => {
      expect(currentApi.checkUpdate).toHaveBeenCalledOnce();
      const checking = findButton('Check for updates', target);
      expect(checking).toBeDefined();
      expect(checking?.disabled).toBe(true);
      expect(checking?.getAttribute('aria-busy')).toBe('true');
      expect(checking?.querySelector('svg.animate-spin')).not.toBeNull();
      expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('Checking…');
    });

    checkDeferred.resolve({
      overall: 'current',
      installed_release: null,
      available_release: null,
      update_available: false,
      repair_required: false,
      drifted: false,
    });
    await vi.waitFor(() => {
      expect(store.checkingUpdates).toBe(false);
      const finished = findButton('Check for updates', target);
      expect(finished?.disabled).toBe(false);
      expect(finished?.getAttribute('aria-busy')).toBe('false');
      expect(finished?.querySelector('svg.animate-spin')).toBeNull();
      expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('');
    });
  });
});
