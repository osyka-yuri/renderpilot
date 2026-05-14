import { describe, expect, it } from 'vitest';
import { buildInitialLauncherOrder, canonicalizeLauncherOrder } from './launcher-order';

const STEAM = 'Steam';
const GOG = 'Gog';
const EPIC = 'Epic';
const UBISOFT = 'Ubisoft';

describe('canonicalizeLauncherOrder', () => {
  it('preserves the given order for available launchers', () => {
    expect(canonicalizeLauncherOrder([GOG, STEAM], [STEAM, GOG])).toEqual([GOG, STEAM]);
  });

  it('drops launchers that are not available', () => {
    expect(canonicalizeLauncherOrder([STEAM, EPIC], [STEAM, GOG])).toEqual([STEAM, GOG]);
  });

  it('appends newly available launchers that are absent from the order', () => {
    expect(canonicalizeLauncherOrder([STEAM], [STEAM, GOG, EPIC])).toEqual([STEAM, GOG, EPIC]);
  });

  it('appends missing launchers in available-list order', () => {
    expect(canonicalizeLauncherOrder([EPIC], [STEAM, GOG, EPIC])).toEqual([EPIC, STEAM, GOG]);
  });

  it('returns all available launchers when order is empty', () => {
    expect(canonicalizeLauncherOrder([], [GOG, STEAM])).toEqual([GOG, STEAM]);
  });

  it('returns an empty array when available launchers are empty', () => {
    expect(canonicalizeLauncherOrder([STEAM, GOG], [])).toEqual([]);
  });

  it('returns the same order when order matches available exactly', () => {
    expect(canonicalizeLauncherOrder([STEAM, GOG], [STEAM, GOG])).toEqual([STEAM, GOG]);
  });

  it('removes duplicate launchers from the persisted order', () => {
    expect(canonicalizeLauncherOrder([GOG, GOG, STEAM, STEAM], [STEAM, GOG, EPIC])).toEqual([
      GOG,
      STEAM,
      EPIC,
    ]);
  });

  it('removes duplicate launchers from the available list', () => {
    expect(canonicalizeLauncherOrder([GOG], [STEAM, GOG, GOG, EPIC, STEAM])).toEqual([
      GOG,
      STEAM,
      EPIC,
    ]);
  });

  it('handles duplicates and unavailable launchers together', () => {
    expect(
      canonicalizeLauncherOrder([UBISOFT, GOG, GOG, STEAM, UBISOFT], [STEAM, GOG, EPIC]),
    ).toEqual([GOG, STEAM, EPIC]);
  });

  it('does not mutate the input arrays', () => {
    const order = [GOG, STEAM, EPIC];
    const availableLaunchers = [STEAM, GOG];

    const originalOrder = [...order];
    const originalAvailableLaunchers = [...availableLaunchers];

    canonicalizeLauncherOrder(order, availableLaunchers);

    expect(order).toEqual(originalOrder);
    expect(availableLaunchers).toEqual(originalAvailableLaunchers);
  });

  it('always returns a new array instance', () => {
    const order = [STEAM, GOG];

    expect(canonicalizeLauncherOrder(order, order)).not.toBe(order);
  });
});

describe('buildInitialLauncherOrder', () => {
  it('returns all available launchers in catalog order when persisted order is null', () => {
    expect(buildInitialLauncherOrder(null, [STEAM, GOG])).toEqual([STEAM, GOG]);
  });

  it('returns all available launchers in catalog order when persisted order is empty', () => {
    expect(buildInitialLauncherOrder([], [STEAM, GOG])).toEqual([STEAM, GOG]);
  });

  it('restores the persisted order, filtering out unavailable launchers', () => {
    expect(buildInitialLauncherOrder([GOG, EPIC, STEAM], [STEAM, GOG])).toEqual([GOG, STEAM]);
  });

  it('appends launchers that are available but missing from persisted order', () => {
    expect(buildInitialLauncherOrder([STEAM], [STEAM, GOG, EPIC])).toEqual([STEAM, GOG, EPIC]);
  });

  it('canonicalizes duplicate persisted launchers', () => {
    expect(buildInitialLauncherOrder([GOG, GOG, STEAM], [STEAM, GOG])).toEqual([GOG, STEAM]);
  });

  it('canonicalizes duplicate available launchers', () => {
    expect(buildInitialLauncherOrder(null, [STEAM, GOG, STEAM])).toEqual([STEAM, GOG]);
  });

  it('returns an empty array when both persisted order and available launchers are empty', () => {
    expect(buildInitialLauncherOrder([], [])).toEqual([]);
  });

  it('returns an empty array when available launchers are empty regardless of persisted order', () => {
    expect(buildInitialLauncherOrder([STEAM, GOG], [])).toEqual([]);
  });
});
