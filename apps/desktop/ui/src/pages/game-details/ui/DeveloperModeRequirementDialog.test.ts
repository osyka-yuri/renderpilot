/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

const openDeveloperModeSettings = vi.hoisted(() => vi.fn<() => Promise<void>>());
const previewMode = vi.hoisted(() => ({ enabled: false }));

vi.mock('../model/developer-mode-links', () => ({ openDeveloperModeSettings }));
vi.mock('@shared/api-preview', () => ({
  isDesktopPreviewMode: () => previewMode.enabled,
}));

import DeveloperModeRequirementDialogTestHost from './DeveloperModeRequirementDialog.test-host.svelte';

describe('DeveloperModeRequirementDialog', () => {
  let target: HTMLDivElement;
  let component:
    | {
        close: () => void;
        completeRetryAsDisabled: () => void;
        show: () => void;
        showUnavailable: () => void;
      }
    | undefined;

  beforeEach(() => {
    previewMode.enabled = false;
    openDeveloperModeSettings.mockReset();
    openDeveloperModeSettings.mockResolvedValue();
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      component.close();
      flushSync();
      await settleOverlays();
      await unmount(component);
      component = undefined;
    }
    vi.unstubAllGlobals();
    document.body.replaceChildren();
  });

  it('presents a concise requirement, restart caveat, status check, and cancellation', async () => {
    let resolveOpen: (() => void) | undefined;
    openDeveloperModeSettings.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveOpen = resolve;
        }),
    );
    const onRetry = vi.fn();
    const onCancel = vi.fn();
    component = mount(DeveloperModeRequirementDialogTestHost, {
      target,
      props: { onRetry, onCancel },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain('Windows Developer Mode is off');
      expect(document.body.textContent).toContain(
        'Microsoft D3D12 Agility Preview requires this Windows setting.',
      );
      expect(document.body.textContent).toContain(
        'In some cases, Windows applies this setting only after a restart.',
      );
    });
    expect(document.body.querySelector('[data-slot="alert"][role="note"]')).not.toBeNull();
    expect(footerLayout()).toEqual({
      cancel: 'Cancel',
      actions: ['Check status', 'Open Settings'],
    });
    expect(footerButtonLabels()).toEqual(['Cancel', 'Check status', 'Open Settings']);

    findButton('Open Settings').click();
    await vi.waitFor(() => {
      expect(openDeveloperModeSettings).toHaveBeenCalledOnce();
      expect(footerButtonLabels()).toEqual(['Cancel', 'Check status', 'Open Settings']);
      expect(findButton('Check status').disabled).toBe(true);
    });

    resolveOpen?.();
    await vi.waitFor(() => {
      expect(findButton('Check status').disabled).toBe(false);
    });

    findButton('Check status').click();
    flushSync();
    expect(onRetry).toHaveBeenCalledOnce();
    expect(document.body.textContent).not.toContain('Developer Mode is still off');
    expect(document.body.querySelector('[data-slot="alert"][role="status"]')).toBeNull();

    component.completeRetryAsDisabled();
    flushSync();
    expect(document.body.textContent).toContain('Developer Mode is still off');
    expect(
      document.body.textContent.match(
        /In some cases, Windows applies this setting only after a restart\./g,
      ),
    ).toHaveLength(1);
    expect(document.body.querySelector('[data-slot="alert"][role="status"]')).not.toBeNull();

    findButton('Check status').click();
    flushSync();
    expect(onRetry).toHaveBeenCalledTimes(2);
    expect(document.body.textContent).toContain('Developer Mode is still off');
    expect(document.body.querySelector('[data-slot="alert"][role="status"]')).not.toBeNull();

    findButton('Cancel').click();
    flushSync();
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('keeps verification fail-closed and reports an error opening Settings', async () => {
    openDeveloperModeSettings.mockRejectedValueOnce(new Error('unavailable'));
    const onRetry = vi.fn();
    component = mount(DeveloperModeRequirementDialogTestHost, {
      target,
      props: { onRetry, onCancel: vi.fn() },
    });
    flushSync();

    findButton('Open Settings').click();
    await vi.waitFor(() => {
      expect(document.body.textContent).toContain('Could not open Windows Settings');
    });

    component.showUnavailable();
    flushSync();
    expect(document.body.textContent).toContain('Could not check Developer Mode');
    expect(document.body.textContent).toContain(
      'A successful check is required before continuing.',
    );
    expect(document.body.querySelector('[data-slot="alert"][role="alert"]')).not.toBeNull();
    expect(buttonNamed('Open Settings')).toBeUndefined();

    findButton('Retry check').click();
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it('uses documentation-specific copy when opening the browser target throws', async () => {
    previewMode.enabled = true;
    openDeveloperModeSettings.mockRejectedValueOnce(new Error('popup blocked'));
    component = mount(DeveloperModeRequirementDialogTestHost, {
      target,
      props: { onRetry: vi.fn(), onCancel: vi.fn() },
    });
    flushSync();

    expect(buttonNamed('Open Settings')).toBeUndefined();
    findButton('Open documentation').click();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain('Could not open Microsoft documentation.');
    });
  });

  it('keeps cancellation available while Windows Settings is opening', async () => {
    let rejectOpen: ((error: Error) => void) | undefined;
    openDeveloperModeSettings.mockImplementationOnce(
      () =>
        new Promise<void>((_resolve, reject) => {
          rejectOpen = reject;
        }),
    );
    const onCancel = vi.fn();
    component = mount(DeveloperModeRequirementDialogTestHost, {
      target,
      props: { onRetry: vi.fn(), onCancel },
    });
    flushSync();

    findButton('Open Settings').click();
    await vi.waitFor(() => {
      expect(findButton('Cancel').disabled).toBe(false);
      expect(findButton('Check status').disabled).toBe(true);
      expect(findButton('Open Settings').disabled).toBe(true);
    });

    findButton('Cancel').click();
    expect(onCancel).toHaveBeenCalledOnce();

    rejectOpen?.(new Error('late failure'));
    await vi.waitFor(() => {
      expect(openDeveloperModeSettings).toHaveBeenCalledOnce();
    });
    component.show();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.textContent).not.toContain('Could not open Windows Settings');
    });
  });
});

function buttonNamed(label: string): HTMLButtonElement | undefined {
  return [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
    (button) => button.textContent.trim() === label,
  );
}

function findButton(label: string): HTMLButtonElement {
  const button = buttonNamed(label);
  if (!button) {
    throw new Error(`Button not found: ${label}`);
  }
  return button;
}

function footerButtonLabels(): string[] {
  return [
    ...document.body.querySelectorAll<HTMLButtonElement>('[data-slot="dialog-footer"] button'),
  ].map((button) => button.textContent.trim());
}

function footerLayout(): { cancel: string; actions: string[] } {
  const footer = document.body.querySelector<HTMLElement>('[data-slot="dialog-footer"]');
  const cancel = footer?.querySelector<HTMLButtonElement>(':scope > button');
  const actionGroup = footer?.querySelector<HTMLElement>(':scope > div');
  if (!cancel || !actionGroup) {
    throw new Error('Expected the dialog footer to contain separate cancel and action groups.');
  }

  return {
    cancel: cancel.textContent.trim(),
    actions: [...actionGroup.querySelectorAll<HTMLButtonElement>('button')].map((button) =>
      button.textContent.trim(),
    ),
  };
}

async function settleOverlays(): Promise<void> {
  await tick();
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        resolve();
      });
    });
  });
  await tick();
}
