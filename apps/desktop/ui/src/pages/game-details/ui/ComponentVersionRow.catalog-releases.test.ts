/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import { setLanguageMode } from '@shared/i18n';

import {
  catalogCandidate,
  component as componentFixture,
  group,
} from '../model/candidate-group-fixtures';
import ComponentVersionRowTestHost from './ComponentVersionRow.test-host.svelte';

describe('ComponentVersionRow catalog releases', () => {
  let target: HTMLDivElement;
  let mounted: object | undefined;

  beforeEach(() => {
    setLanguageMode('en');
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
    Object.defineProperties(HTMLElement.prototype, {
      hasPointerCapture: {
        configurable: true,
        value: vi.fn(() => false),
      },
      releasePointerCapture: {
        configurable: true,
        value: vi.fn(),
      },
    });
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (mounted) {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      flushSync();
      await settleOverlays();
      await unmount(mounted);
      mounted = undefined;
    }
    vi.unstubAllGlobals();
    delete (HTMLElement.prototype as Partial<HTMLElement>).hasPointerCapture;
    delete (HTMLElement.prototype as Partial<HTMLElement>).releasePointerCapture;
    document.body.replaceChildren();
  });

  it('shows each stable and preview release exactly once', async () => {
    mounted = mountRow([
      catalogCandidate('1.721.3', { artifact_id: 'stable' }),
      catalogCandidate('1.721.2-preview', {
        artifact_id: 'preview',
        catalog_package: {
          package_id: 'preview',
          release: { version: '1.721.2-preview', channel: 'preview', label: null },
          availability: 'available',
          automatic_selection_allowed: false,
        },
      }),
    ]);

    await openVersionSelect();
    expect(selectItemLabels()).toContain('v1.721.3');
    expect(selectItemLabels()).toContain('v1.721.2-preview');
    expect(versionOccurrenceCount('v1.721.3')).toBe(1);
    expect(versionOccurrenceCount('v1.721.2-preview')).toBe(1);
    expect(document.querySelector('[data-slot="select-content"] [data-slot="badge"]')).toBeNull();
  });

  it('keeps a verified local-only preview available to manual selection', async () => {
    mounted = mountRow([
      catalogCandidate('1.721.2-preview', {
        artifact_id: 'local-preview',
        catalog_package: {
          package_id: 'local-preview',
          release: { version: '1.721.2-preview', channel: 'preview', label: null },
          availability: 'local_only',
          automatic_selection_allowed: false,
        },
        is_downloaded: true,
      }),
    ]);

    await openVersionSelect();
    expect(selectItemLabels()).toContain('v1.721.2-preview');
    expect(versionOccurrenceCount('v1.721.2-preview')).toBe(1);
  });

  it('shows an installed preview as current alongside other preview releases', async () => {
    const candidateGroup = group('component:preview', 'microsoft_dxc', '10.0.0', [
      catalogCandidate('1.721.1-preview', {
        artifact_id: 'older-preview',
        catalog_package: {
          package_id: 'older-preview',
          release: { version: '1.721.1-preview', channel: 'preview', label: null },
          availability: 'available',
          automatic_selection_allowed: false,
        },
      }),
    ]);
    candidateGroup.version_report = {
      kind: 'known',
      technical_version: null,
      release_label: null,
      catalog_release: {
        version: '1.721.2-preview',
        channel: 'preview',
        label: null,
      },
    };
    mounted = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: componentFixture('component:preview', 'microsoft_dxc'),
        group: candidateGroup,
        busy: false,
        onSwap: vi.fn(),
        onRollback: vi.fn(),
      },
    });
    flushSync();

    expect(versionSelectTrigger().textContent).toContain('1.721.2-preview');
    await openVersionSelect();
    expect(selectItemLabels()).toContain('v1.721.2-preview');
    expect(selectItemLabels()).toContain('v1.721.1-preview');
  });

  function mountRow(candidates: Parameters<typeof group>[3]): object {
    const result = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: componentFixture('component:preview', 'microsoft_dxc'),
        group: group('component:preview', 'microsoft_dxc', '10.0.0', candidates),
        busy: false,
        onSwap: vi.fn(),
        onRollback: vi.fn(),
      },
    });
    flushSync();
    return result;
  }

  async function openVersionSelect(): Promise<void> {
    const trigger = versionSelectTrigger();
    trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
    trigger.click();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="listbox"]')).not.toBeNull();
    });
  }

  function versionSelectTrigger(): HTMLButtonElement {
    const trigger = target.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]');
    if (!trigger) {
      throw new Error('Version selector was not rendered');
    }
    return trigger;
  }
});

function selectItemLabels(): string[] {
  return [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].map((option) =>
    option.textContent.replace(/\s+/gu, ''),
  );
}

function versionOccurrenceCount(versionLabel: string): number {
  return [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].filter((option) =>
    option.textContent.includes(versionLabel),
  ).length;
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
