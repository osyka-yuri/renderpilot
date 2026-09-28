/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';
import { t } from '@shared/i18n';
import {
  clearAllNotifications,
  subscribeToNotificationEvents,
  type Notification,
} from '@shared/notifications';

import FileSafetyContextTestHost from './create-file-safety-context.test-host.svelte';

describe('mutation confirmation preference feedback', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  let disposeInvoker: (() => void) | undefined;
  let disposeNotifications: (() => void) | undefined;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
    clearAllNotifications();
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
    }
    disposeInvoker?.();
    disposeNotifications?.();
    clearAllNotifications();
    document.body.replaceChildren();
  });

  it('continues the accepted mutation and publishes one error if saving the opt-out fails', async () => {
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        return Promise.resolve({
          game_id: 'preference-save-game',
          context_token: 'game-token',
          detected_engines: [],
          scan_completeness: 'complete',
        });
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: null });
      }
      if (command === 'set_catalog_setting') {
        return Promise.reject(new Error('database unavailable'));
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    const notifications: Notification[] = [];
    disposeNotifications = subscribeToNotificationEvents((event) => {
      if (event.type === 'published') {
        notifications.push(event.notification);
      }
    });

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'preference-save-game' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): { notice: { kind: string } | null } | null;
    };

    const action = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice?.kind).toBe('general');
    });
    host.resolveMutationConfirmation(true, true);

    await expect(action).resolves.toEqual({ gameContextToken: 'game-token' });
    await vi.waitFor(() => {
      expect(notifications).toHaveLength(1);
    });
    expect(notifications[0]).toMatchObject({
      severity: 'error',
      title: t('notify.statusError'),
      description: t('gameDetails.fileSafety.preferenceSaveFailed'),
    });

    const retry = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice?.kind).toBe('general');
    });
    host.resolveMutationConfirmation(false, false);
    await expect(retry).resolves.toBeNull();
  });
});
