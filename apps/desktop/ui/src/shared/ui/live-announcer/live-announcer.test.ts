/**
 * @vitest-environment jsdom
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';
import { createClassComponent } from 'svelte/legacy';

import LiveAnnouncer from './live-announcer.svelte';

type LiveAnnouncerProps = {
  message: string;
  politeness?: 'polite' | 'assertive';
};

type LiveAnnouncerInstance = {
  $destroy: () => void;
  $set: (props: Partial<LiveAnnouncerProps>) => void;
};

function mountAnnouncer(target: HTMLDivElement, props: LiveAnnouncerProps): LiveAnnouncerInstance {
  // oxlint-disable-next-line typescript/no-deprecated -- test-only bridge needed to drive public prop updates.
  return createClassComponent({ component: LiveAnnouncer, target, props });
}

describe('LiveAnnouncer', () => {
  let target: HTMLDivElement | undefined;
  let component: LiveAnnouncerInstance | undefined;

  afterEach(() => {
    if (component) {
      component.$destroy();
      component = undefined;
    }
    target?.remove();
    target = undefined;
    vi.useRealTimers();
  });

  it('mounts an empty atomic status region before announcing the first message', () => {
    vi.useFakeTimers();
    target = document.createElement('div');
    document.body.append(target);
    component = mountAnnouncer(target, { message: 'Saved' });
    flushSync();

    const announcer = target.querySelector('p');
    expect(announcer?.classList.contains('sr-only')).toBe(true);
    expect(announcer?.getAttribute('role')).toBe('status');
    expect(announcer?.getAttribute('aria-live')).toBe('polite');
    expect(announcer?.getAttribute('aria-atomic')).toBe('true');
    expect(announcer?.textContent).toBe('');

    vi.runOnlyPendingTimers();
    flushSync();
    expect(announcer?.textContent).toBe('Saved');
  });

  it('supports assertive delivery where the caller requires it', () => {
    target = document.createElement('div');
    document.body.append(target);
    const props: LiveAnnouncerProps = {
      message: 'Connection lost',
      politeness: 'assertive',
    };
    component = mountAnnouncer(target, props);
    flushSync();

    expect(target.querySelector('[role="status"]')?.getAttribute('aria-live')).toBe('assertive');
  });

  it('keeps the same status node while later prop updates replace its message', () => {
    vi.useFakeTimers();
    target = document.createElement('div');
    document.body.append(target);
    component = mountAnnouncer(target, { message: 'Saved' });
    flushSync();

    const announcer = target.querySelector('[role="status"]');
    vi.runOnlyPendingTimers();
    flushSync();
    expect(announcer?.textContent).toBe('Saved');

    component.$set({ message: 'Deleted' });
    flushSync();
    expect(target.querySelector('[role="status"]')).toBe(announcer);
    expect(announcer?.textContent).toBe('Saved');

    vi.runOnlyPendingTimers();
    flushSync();
    expect(announcer?.textContent).toBe('Deleted');
  });

  it('keeps only the latest rapid message and cancels a pending update on unmount', () => {
    vi.useFakeTimers();
    target = document.createElement('div');
    document.body.append(target);
    component = mountAnnouncer(target, { message: 'First' });
    flushSync();

    component.$set({ message: 'Second' });
    flushSync();
    component.$set({ message: 'Third' });
    flushSync();
    expect(vi.getTimerCount()).toBe(1);
    vi.runOnlyPendingTimers();
    flushSync();
    expect(target.querySelector('[role="status"]')?.textContent).toBe('Third');

    component.$set({ message: 'Ignored after unmount' });
    flushSync();
    component.$destroy();
    component = undefined;

    expect(vi.getTimerCount()).toBe(0);
    vi.runOnlyPendingTimers();
    expect(target.querySelector('[role="status"]')).toBeNull();
  });
});
