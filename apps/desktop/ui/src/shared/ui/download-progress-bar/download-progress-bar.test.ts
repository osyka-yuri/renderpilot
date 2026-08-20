/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

vi.mock('@shared/lib', () => ({
  latestDownloadProgress: vi.fn(),
}));

import { latestDownloadProgress } from '@shared/lib';

import DownloadProgressBar from './download-progress-bar.svelte';

const mockLatestDownloadProgress = vi.mocked(latestDownloadProgress);

describe('DownloadProgressBar', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  function render(): void {
    component = mount(DownloadProgressBar, {
      target,
      props: { ids: ['library-artifact'], active: true },
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
    vi.clearAllMocks();
  });

  it('shows only the progress bar for a byte-tracked download', () => {
    mockLatestDownloadProgress.mockReturnValue({
      id: 'library-artifact',
      downloaded: 50,
      total: 100,
      phase: 'Library artifact download',
    });

    render();

    expect(target.textContent).not.toContain('Library artifact download');
    expect(target.querySelector('[data-slot="progress"]')?.getAttribute('aria-label')).toBe(
      'Download progress',
    );
    expect(target.querySelector('[role="status"]')).toBeNull();
  });

  it('shows the spinner and its phase text for an indeterminate phase', () => {
    mockLatestDownloadProgress.mockReturnValue({
      id: 'library-artifact',
      downloaded: 100,
      total: 0,
      phase: 'renodx.phase.finalizing',
    });

    render();

    expect(target.textContent).toContain('Finalizing…');
    expect(target.querySelector('[data-slot="progress"]')).toBeNull();
    expect(target.querySelector('[aria-hidden="true"].animate-spin')).not.toBeNull();
    expect(target.querySelector('[role="status"]')).toBeNull();
  });
});
