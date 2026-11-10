/** @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';
import { setLanguageMode } from '@shared/i18n';
import { createExclusiveTaskRunner } from '@shared/concurrency';
import { settingsTabMemory } from '../model/settings-page-model';
import SettingsPageTestHost from './SettingsPage.test-host.svelte';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

let component: ReturnType<typeof mount> | undefined;
let disposeInvoker: (() => void) | undefined;
let resizeObserverDescriptor: PropertyDescriptor | undefined;
let animateDescriptor: PropertyDescriptor | undefined;
let target: HTMLDivElement;

beforeEach(async () => {
  await setLanguageMode('en');
  resizeObserverDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'ResizeObserver');
  Object.defineProperty(globalThis, 'ResizeObserver', {
    configurable: true,
    value: class ResizeObserverMock {
      private readonly targets = new Set<Element>();
      observe(target: Element): void {
        this.targets.add(target);
      }
      unobserve(target: Element): void {
        this.targets.delete(target);
      }
      disconnect(): void {
        this.targets.clear();
      }
    },
  });
  animateDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'animate');
  Object.defineProperty(HTMLElement.prototype, 'animate', {
    configurable: true,
    value: () => {
      const animation: { onfinish: (() => void) | null; cancel: () => void } = {
        onfinish: null,
        cancel: () => undefined,
      };
      queueMicrotask(() => animation.onfinish?.());
      return animation;
    },
  });
  settingsTabMemory.rememberTab('general');
  target = document.createElement('div');
  document.body.append(target);
});

afterEach(async () => {
  if (component) {
    await unmount(component);
    component = undefined;
  }
  disposeInvoker?.();
  disposeInvoker = undefined;
  target.remove();
  settingsTabMemory.rememberTab('general');
  if (resizeObserverDescriptor) {
    Object.defineProperty(globalThis, 'ResizeObserver', resizeObserverDescriptor);
  } else {
    Reflect.deleteProperty(globalThis, 'ResizeObserver');
  }
  resizeObserverDescriptor = undefined;
  if (animateDescriptor) {
    Object.defineProperty(HTMLElement.prototype, 'animate', animateDescriptor);
  } else {
    Reflect.deleteProperty(HTMLElement.prototype, 'animate');
  }
  animateDescriptor = undefined;
});

function tab(name: string): HTMLButtonElement {
  const match = [...target.querySelectorAll<HTMLButtonElement>('[role="tab"]')].find((button) =>
    button.textContent.includes(name),
  );
  if (!match) {
    throw new Error(`Expected a settings tab named ${name}`);
  }
  return match;
}

describe('SettingsPage NVAPI tab refresh', () => {
  it('uses only the initial load on the first NVIDIA visit', async () => {
    let globalReads = 0;
    const invoker: DesktopInvoker = <T>(command: string) => {
      if (command === 'list_global_nvapi_setting_states') {
        globalReads += 1;
        return Promise.resolve([] as T);
      }
      if (command === 'get_dlss_indicator_state') {
        return Promise.resolve({ enabled: false, supported: true } as T);
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: '' } as T);
      }
      return Promise.resolve(undefined as T);
    };
    disposeInvoker = registerPreviewInvoker(invoker);
    component = mount(SettingsPageTestHost, {
      target,
      props: { nvapiMutationRunner: createExclusiveTaskRunner() },
    });
    flushSync();
    expect(globalReads).toBe(0);

    tab('NVIDIA').click();
    flushSync();
    await vi.waitFor(() => {
      expect(globalReads).toBe(1);
    });
    await vi.waitFor(() => {
      expect(target.textContent).toContain('System-wide');
    });

    expect(globalReads).toBe(1);
  });

  it('uses its first read once, then refreshes after re-entering during that read', async () => {
    const firstRead = deferred<unknown[]>();
    let globalReads = 0;
    const invoker: DesktopInvoker = <T>(command: string) => {
      if (command === 'list_global_nvapi_setting_states') {
        globalReads += 1;
        return globalReads === 1 ? (firstRead.promise as Promise<T>) : Promise.resolve([] as T);
      }
      if (command === 'get_dlss_indicator_state') {
        return Promise.resolve({ enabled: false, supported: true } as T);
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: '' } as T);
      }
      return Promise.resolve(undefined as T);
    };
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(SettingsPageTestHost, {
      target,
      props: { nvapiMutationRunner: createExclusiveTaskRunner() },
    });
    flushSync();
    expect(globalReads).toBe(0);

    tab('NVIDIA').click();
    flushSync();
    await vi.waitFor(() => {
      expect(globalReads).toBe(1);
    });

    tab('General').click();
    flushSync();
    tab('NVIDIA').click();
    flushSync();
    expect(globalReads).toBe(1);

    firstRead.resolve([]);
    await vi.waitFor(() => {
      expect(globalReads).toBe(2);
    });
    await vi.waitFor(() => {
      expect(target.textContent).toContain('System-wide');
    });
    expect(globalReads).toBe(2);
  });
});
