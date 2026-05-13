import { describe, expect, it, vi } from 'vitest';
import type { GameCardMenuHandle } from '@entities/game';
import { focusMenuTrigger } from './games-page-cover-ops';

describe('games-page-cover-ops', () => {
  describe('focusMenuTrigger', () => {
    it('focuses trigger via rAF when available', () => {
      const focusSpy = vi.fn();
      const refs: Record<string, GameCardMenuHandle | undefined> = {
        'game-1': { focusTrigger: focusSpy },
      };

      const rafStub = vi.fn<(cb: FrameRequestCallback) => number>().mockImplementation((cb) => {
        cb(0);
        return 0;
      });

      (globalThis as unknown as { requestAnimationFrame: typeof rafStub }).requestAnimationFrame =
        rafStub;

      focusMenuTrigger(refs, 'game-1');
      expect(focusSpy).toHaveBeenCalledOnce();

      // @ts-expect-error intentionally removing rAF after test
      delete globalThis.requestAnimationFrame;
    });

    it('focuses trigger directly when rAF is unavailable', () => {
      const focusSpy = vi.fn();
      const refs: Record<string, GameCardMenuHandle | undefined> = {
        'game-1': { focusTrigger: focusSpy },
      };

      const originalRaf = globalThis.requestAnimationFrame;
      // @ts-expect-error intentionally removing rAF for test
      globalThis.requestAnimationFrame = undefined;

      focusMenuTrigger(refs, 'game-1');
      expect(focusSpy).toHaveBeenCalledOnce();

      globalThis.requestAnimationFrame = originalRaf;
    });

    it('does not throw when ref is missing', () => {
      const refs: Record<string, GameCardMenuHandle | undefined> = {};

      expect(() => {
        focusMenuTrigger(refs, 'game-1');
      }).not.toThrow();
    });
  });
});
