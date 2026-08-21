/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import GameFileSafetyRow from './GameFileSafetyRow.svelte';

describe('GameFileSafetyRow', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    document.body.replaceChildren();
  });

  it('renders one standard compact alert with the neutral copy', () => {
    component = mount(GameFileSafetyRow, { target, props: { assessment: null } });
    flushSync();

    expect(target.querySelectorAll('[data-file-safety-row]')).toHaveLength(1);
    const row = target.querySelector<HTMLElement>('[data-file-safety-row]');
    expect(row?.textContent).toContain(
      'Changes to multiplayer game files may result in account restrictions or a ban.',
    );
    expect(row?.dataset.slot).toBe('alert');
    expect(row?.className).toContain('rounded-md');
    expect(row?.className).toContain('bg-card');
    expect(row?.querySelector('[data-slot="alert-description"]')).not.toBeNull();
  });

  it('shows detected engines without adding a second warning row', () => {
    component = mount(GameFileSafetyRow, {
      target,
      props: {
        assessment: {
          game_id: 'steam:123',
          context_token: 'context',
          detected_engines: ['EasyAntiCheat'],
          scan_completeness: 'complete',
        },
      },
    });
    flushSync();

    expect(target.querySelectorAll('[data-file-safety-row]')).toHaveLength(1);
    expect(target.textContent).toContain('Easy Anti-Cheat detected.');
    const obsoleteDetectionCopy = ['anti', '-cheat not detected'].join('');
    const safeWord = ['s', 'a', 'f', 'e'].join('');
    const text = target.textContent.toLocaleLowerCase();
    expect(text).not.toContain(obsoleteDetectionCopy);
    expect(text).not.toContain(safeWord);
    expect(text).not.toContain(`${safeWord} likely ${safeWord}`);
  });

  it('keeps detected engines visible with the standard alert style when the scan is limited', () => {
    component = mount(GameFileSafetyRow, {
      target,
      props: {
        assessment: {
          game_id: 'steam:123',
          context_token: 'context',
          detected_engines: ['EasyAntiCheat'],
          scan_completeness: 'limited',
        },
      },
    });
    flushSync();

    expect(target.querySelectorAll('[data-file-safety-row]')).toHaveLength(1);
    expect(target.textContent).toContain('Easy Anti-Cheat detected.');
    const row = target.querySelector<HTMLElement>('[data-file-safety-row]');
    expect(row?.className).toContain('bg-card');
    expect(row?.className).not.toContain('border-amber');
    expect(row?.className).not.toContain('bg-amber');
  });
});
