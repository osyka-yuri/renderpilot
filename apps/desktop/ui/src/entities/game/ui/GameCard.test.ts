/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { createGameSummary } from '../model/test-support';
import { toGameCardViewModel } from '../model/game-card-view-model';
import GameCard from './GameCard.svelte';

describe('GameCard details navigation intent', () => {
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
    target.remove();
  });

  it('prefetches details on pointer and keyboard intent before opening them', () => {
    const onPreloadDetails = vi.fn();
    const onOpenDetails = vi.fn();

    component = mount(GameCard, {
      target,
      props: {
        game: toGameCardViewModel(createGameSummary()),
        onPreloadDetails,
        onOpenDetails,
      },
    });
    flushSync();

    const details = target.querySelector<HTMLButtonElement>('[data-game-details-trigger]');
    expect(details).not.toBeNull();

    details?.dispatchEvent(new Event('pointerenter'));
    details?.focus();
    details?.click();

    expect(onPreloadDetails).toHaveBeenCalledTimes(2);
    expect(onOpenDetails).toHaveBeenCalledTimes(1);
  });
});
