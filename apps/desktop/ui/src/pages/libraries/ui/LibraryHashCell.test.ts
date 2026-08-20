/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { packageSummary } from '../model/library-package-test-fixtures';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';

const reportClientError = vi.hoisted(() => vi.fn());

vi.mock('@shared/errors', () => ({ reportClientError }));

import LibraryLegalCellTestHost from './LibraryLegalCell.test-host.svelte';

const row = packageSummary({ id: 'hash-cell', version: '1.2.3' });

describe('LibraryHashCell', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  const writeText = vi.fn<Navigator['clipboard']['writeText']>();

  beforeEach(() => {
    clearAllNotifications();
    target = document.createElement('div');
    document.body.append(target);
    Object.assign(navigator, { clipboard: { writeText } });
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
    clearAllNotifications();
    vi.clearAllMocks();
  });

  function render(): HTMLButtonElement {
    component = mount(LibraryLegalCellTestHost, {
      target,
      props: { row, onOpen: vi.fn(), cell: 'hash' },
    });
    flushSync();
    const copy = target.querySelector<HTMLButtonElement>('button');
    if (!copy) {
      throw new Error('Expected a hash copy button');
    }

    return copy;
  }

  it('keeps its copy name stable and delegates successful feedback to the notification bus', async () => {
    writeText.mockResolvedValueOnce(undefined);
    const copy = render();
    const nameBefore = copy.getAttribute('aria-label');

    copy.click();

    await vi.waitFor(() => {
      expect(writeText).toHaveBeenCalledExactlyOnceWith(row.primary_sha256);
      expect(getActiveNotifications()).toEqual([
        expect.objectContaining({ severity: 'success', title: 'Hash copied to clipboard' }),
      ]);
    });

    expect(copy.getAttribute('aria-label')).toBe(nameBefore);
    expect(reportClientError).not.toHaveBeenCalled();
  });

  it('keeps its copy name stable while reporting clipboard failure and publishing one error notification', async () => {
    const failure = new Error('clipboard unavailable');
    writeText.mockRejectedValueOnce(failure);
    const copy = render();
    const nameBefore = copy.getAttribute('aria-label');

    copy.click();

    await vi.waitFor(() => {
      expect(reportClientError).toHaveBeenCalledExactlyOnceWith('copy_library_hash', failure);
      expect(getActiveNotifications()).toEqual([
        expect.objectContaining({
          severity: 'error',
          title: 'Failed to copy',
          important: undefined,
        }),
      ]);
    });

    expect(copy.getAttribute('aria-label')).toBe(nameBefore);
  });
});
