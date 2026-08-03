/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import type { AvailabilityReport } from '../model/types';
import { availability, fakeApi } from '../model/renodx-store-test-fixtures';
import { createRenoDxStore } from '../model/create-renodx-store.svelte';

const openExternal = vi.hoisted(() => vi.fn<(url: string) => Promise<void>>());
const publishPresentedErrorNotification = vi.hoisted(() => vi.fn());

vi.mock('@shared/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openExternal,
}));
vi.mock('@shared/notifications', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  publishPresentedErrorNotification,
}));

import RenoDxExternalView from './RenoDxExternalView.svelte';

describe('RenoDxExternalView', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    openExternal.mockReset();
    publishPresentedErrorNotification.mockReset();
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

  it('reports a rejected external link instead of leaving an unhandled promise', async () => {
    const report: AvailabilityReport = availability({
      state: { status: 'not_installed' },
      outcome: {
        kind: 'external',
        url: 'https://discord.gg/example',
        message: {
          id: 'renodx.external.discord',
          fallback_text: 'Open the RenoDX Discord',
        },
        file_install: null,
      },
      manual_install: null,
    });
    const store = createRenoDxStore({
      api: fakeApi({ getAvailability: vi.fn(() => Promise.resolve(report)) }),
    });
    await store.load('steam:1091500');
    openExternal.mockRejectedValueOnce(new Error('popup blocked'));

    component = mount(RenoDxExternalView, {
      target,
      props: {
        gameId: 'steam:1091500',
        store,
        busy: false,
      },
    });
    flushSync();

    const link = target.querySelector<HTMLButtonElement>('button');
    expect(link?.textContent).toContain('Open the RenoDX Discord');
    link?.click();

    await vi.waitFor(() => {
      expect(openExternal).toHaveBeenCalledWith('https://discord.gg/example');
      expect(publishPresentedErrorNotification).toHaveBeenCalledWith(
        'Open the RenoDX Discord',
        expect.any(Error),
      );
    });
  });
});
