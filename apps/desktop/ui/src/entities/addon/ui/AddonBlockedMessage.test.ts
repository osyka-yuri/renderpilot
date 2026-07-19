/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import AddonBlockedMessage from './AddonBlockedMessage.svelte';

describe('AddonBlockedMessage', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  function render(props: {
    blockedAddon: 'luma' | 'renodx';
    installedAddon?: 'luma' | 'renodx' | null;
    fallbackInstalledAddon: 'luma' | 'renodx';
    unmanaged?: boolean;
    selfUnmanagedMessage?: string | null;
  }): void {
    component = mount(AddonBlockedMessage, {
      target,
      props,
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

  it('explains a tracked peer install blocks this addon', () => {
    render({
      blockedAddon: 'luma',
      installedAddon: 'renodx',
      fallbackInstalledAddon: 'renodx',
    });

    expect(target.textContent).toContain(
      'RenoDX is installed for this game — uninstall it before installing Luma.',
    );
  });

  it('explains unmanaged peer debris on disk', () => {
    render({
      blockedAddon: 'renodx',
      installedAddon: 'luma',
      fallbackInstalledAddon: 'luma',
      unmanaged: true,
    });

    expect(target.textContent).toContain(
      'Luma files were found on disk for this game — remove them before installing RenoDX.',
    );
  });

  it('uses fallbackInstalledAddon when installedAddon is unset', () => {
    render({
      blockedAddon: 'luma',
      installedAddon: null,
      fallbackInstalledAddon: 'renodx',
    });

    expect(target.textContent).toContain(
      'RenoDX is installed for this game — uninstall it before installing Luma.',
    );
  });

  it('prefers selfUnmanagedMessage over i18n templates', () => {
    render({
      blockedAddon: 'luma',
      fallbackInstalledAddon: 'renodx',
      selfUnmanagedMessage: 'custom self-unmanaged debris',
    });

    expect(target.textContent).toContain('custom self-unmanaged debris');
    expect(target.textContent).not.toContain('is installed for this game');
  });
});
