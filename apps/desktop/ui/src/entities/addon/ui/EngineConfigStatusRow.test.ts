/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import type { EngineConfigAvailability } from '../model/types';
import EngineConfigStatusRow from './EngineConfigStatusRow.svelte';
import EngineConfigStatusRowTestHost from './EngineConfigStatusRow.test-host.svelte';

describe('EngineConfigStatusRow', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  function render(
    status: EngineConfigAvailability['status'],
    overrides: Partial<EngineConfigAvailability> = {},
    onApply?: () => void,
  ): void {
    component = mount(EngineConfigStatusRow, {
      target,
      props: {
        availability: {
          status,
          path: null,
          can_apply: false,
          ...overrides,
        },
        onApply,
      },
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

  it('renders the configured state as a normal installed component row', () => {
    render('configured');

    expect(target.textContent).toContain('Engine.ini configuration');
    expect(target.textContent).toContain('Engine.ini settings are applied.');
    expect(target.querySelector('[data-slot="item"]')).not.toBeNull();
    expect(target.querySelector('[data-slot="alert"]')).toBeNull();
  });

  it.each(['ready', 'needs_repair', 'manual_only', 'pending_first_launch'] as const)(
    'renders the actionable %s state in the component group',
    (status) => {
      render(status);
      expect(target.querySelector('[data-slot="item"]')).not.toBeNull();
      expect(target.querySelector('[data-slot="alert"]')).toBeNull();
    },
  );

  it('uses a concise contextual action label', () => {
    const onApply = vi.fn();
    render('needs_repair', { can_apply: true }, onApply);

    const button = target.querySelector('button');
    expect(button?.textContent.trim()).toBe('Apply');
    button?.click();
    expect(onApply).toHaveBeenCalledOnce();
  });

  it('keeps conflict in the same component row', () => {
    render('conflict');
    expect(target.querySelector('[data-slot="item"]')).not.toBeNull();
    expect(target.textContent).toContain('conflicting');
    expect(target.querySelector('[data-slot="alert"]')).toBeNull();
  });

  it('keeps recovery in the same component row and allows retry', () => {
    const onApply = vi.fn();
    render('recovery_required', { can_apply: true }, onApply);

    expect(target.querySelector('[data-slot="item"]')).not.toBeNull();
    expect(target.textContent).toContain('recovery');
    target.querySelector<HTMLButtonElement>('[data-slot="item-actions"] button')?.click();
    expect(onApply).toHaveBeenCalledOnce();
  });

  it('opens manual guidance in a dialog without expanding the component row', () => {
    component = mount(EngineConfigStatusRowTestHost, {
      target,
      props: {
        availability: {
          status: 'manual_only',
          path: null,
          can_apply: false,
        },
        withGuidance: true,
      },
    });
    flushSync();

    const content = target.querySelector<HTMLElement>('[data-slot="item-content"]');
    expect(content?.textContent).toContain('Automatic application is unavailable.');
    expect(content?.querySelector('pre')).toBeNull();
    expect(target.querySelector('[data-slot="item-footer"]')).toBeNull();
    expect(target.querySelector('[data-testid="manual-guidance"]')).toBeNull();
    expect(target.querySelector('[data-slot="item-actions"]')).not.toBeNull();

    target.querySelector<HTMLButtonElement>('[data-slot="item-actions"] button')?.click();
    flushSync();

    expect(document.querySelector('[data-slot="dialog-content"]')).not.toBeNull();
    expect(document.querySelector('[data-testid="manual-guidance"]')).not.toBeNull();
    expect(document.querySelector('[data-slot="dialog-content"] pre')).not.toBeNull();
  });

  it('offers a local availability check for a pending first launch', () => {
    const onRefresh = vi.fn();
    component = mount(EngineConfigStatusRow, {
      target,
      props: {
        availability: {
          status: 'pending_first_launch',
          path: null,
          can_apply: false,
        },
        onRefresh,
      },
    });
    flushSync();

    const button = target.querySelector<HTMLButtonElement>('[data-slot="item-actions"] button');
    expect(button?.textContent).toContain('Check');
    expect(button?.querySelector('svg.animate-spin')).toBeNull();
    button?.click();
    expect(onRefresh).toHaveBeenCalledOnce();
    expect(target.querySelector('pre')).toBeNull();
  });

  it('spins RefreshCwIcon and sets aria-busy only during refreshing', () => {
    component = mount(EngineConfigStatusRow, {
      target,
      props: {
        availability: {
          status: 'pending_first_launch',
          path: null,
          can_apply: false,
        },
        refreshing: true,
        onRefresh: vi.fn(),
      },
    });
    flushSync();

    const button = target.querySelector<HTMLButtonElement>('[data-slot="item-actions"] button');
    expect(button?.textContent).toContain('Check');
    expect(button?.disabled).toBe(true);
    expect(button?.getAttribute('aria-busy')).toBe('true');
    expect(button?.querySelector('svg.animate-spin')).not.toBeNull();
    expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('Checking…');
  });

  it('disables check button without spinning icon when busy but not refreshing', () => {
    component = mount(EngineConfigStatusRow, {
      target,
      props: {
        availability: {
          status: 'pending_first_launch',
          path: null,
          can_apply: false,
        },
        busy: true,
        refreshing: false,
        onRefresh: vi.fn(),
      },
    });
    flushSync();

    const button = target.querySelector<HTMLButtonElement>('[data-slot="item-actions"] button');
    expect(button?.disabled).toBe(true);
    expect(button?.getAttribute('aria-busy')).toBe('false');
    expect(button?.querySelector('svg.animate-spin')).toBeNull();
    expect(target.querySelector('[role="status"]')?.textContent.trim()).toBe('');
  });

  it('does not add a guidance wrapper when no manual guidance is supplied', () => {
    render('manual_only');

    expect(target.querySelector('[data-testid="manual-guidance"]')).toBeNull();
    expect(target.querySelector('[data-slot="item-content"]')).not.toBeNull();
    expect(target.querySelector('[data-slot="item-footer"]')).toBeNull();
    expect(target.querySelector('[data-slot="item-actions"]')).toBeNull();
  });
});
