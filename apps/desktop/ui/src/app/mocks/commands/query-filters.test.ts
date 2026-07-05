import { describe, expect, it } from 'vitest';
import { createGameSummary } from '@entities/game';

import { buildGameCardFilterContext, matchesGameCardFilters, sortGameCards } from './query-filters';

function ctx(
  overrides: Partial<Omit<Parameters<typeof buildGameCardFilterContext>[0], 'selectedAddons'>> & {
    /** Accept unregistered names so filter normalization can be regression-tested. */
    selectedAddons?: readonly string[];
  } = {},
): ReturnType<typeof buildGameCardFilterContext> {
  const { selectedAddons, ...rest } = overrides;
  return buildGameCardFilterContext({
    searchQuery: '',
    selectedLibraries: [],
    selectedLaunchers: [],
    showHidden: false,
    favoritesOnly: false,
    sort: { field: 'title', direction: 'asc' },
    page: { limit: 50, offset: 0 },
    ...rest,
    // Cast: production query type is AddonCapability[], but this helper
    // intentionally accepts unregistered names for normalize regressions.
    selectedAddons: (selectedAddons ?? []) as Parameters<
      typeof buildGameCardFilterContext
    >[0]['selectedAddons'],
  });
}

describe('matchesGameCardFilters', () => {
  it('hides hidden cards unless showHidden is set', () => {
    const card = createGameSummary({ is_hidden: true });
    expect(matchesGameCardFilters(card, ctx())).toBe(false);
    expect(matchesGameCardFilters(card, ctx({ showHidden: true }))).toBe(true);
  });

  it('requires favorites when favoritesOnly is set', () => {
    const plain = createGameSummary({ is_favorite: false });
    const fav = createGameSummary({ is_favorite: true });
    expect(matchesGameCardFilters(plain, ctx({ favoritesOnly: true }))).toBe(false);
    expect(matchesGameCardFilters(fav, ctx({ favoritesOnly: true }))).toBe(true);
  });

  it('ORs library and addon filters when both are active', () => {
    const libraryOnly = createGameSummary({
      library_tags: ['dlss_super_resolution'],
      addon_capabilities: [],
    });
    const addonOnly = createGameSummary({
      library_tags: [],
      addon_capabilities: ['renodx'],
    });
    const neither = createGameSummary({ library_tags: [], addon_capabilities: [] });
    const filter = ctx({
      selectedLibraries: ['dlss_super_resolution'],
      selectedAddons: ['renodx'],
    });

    expect(matchesGameCardFilters(libraryOnly, filter)).toBe(true);
    expect(matchesGameCardFilters(addonOnly, filter)).toBe(true);
    expect(matchesGameCardFilters(neither, filter)).toBe(false);
  });

  it('ANDs launcher with library when both are active', () => {
    const card = createGameSummary({
      launcher: 'Steam',
      library_tags: ['dlss_super_resolution'],
    });
    expect(
      matchesGameCardFilters(
        card,
        ctx({
          selectedLibraries: ['dlss_super_resolution'],
          selectedLaunchers: ['Epic'],
        }),
      ),
    ).toBe(false);
    expect(
      matchesGameCardFilters(
        card,
        ctx({
          selectedLibraries: ['dlss_super_resolution'],
          selectedLaunchers: ['Steam'],
        }),
      ),
    ).toBe(true);
  });
  it('unknown selected addons do not empty the catalog', () => {
    const card = createGameSummary({
      library_tags: [],
      addon_capabilities: ['renodx'],
    });
    const plain = createGameSummary({ library_tags: [], addon_capabilities: [] });
    // Unregistered names (incl. a future/removed kind) are intentional: the
    // filter path must normalize them away rather than empty the catalog.
    const filter = ctx({
      selectedAddons: ['luma', 'unknown'],
    });

    expect(matchesGameCardFilters(card, filter)).toBe(true);
    expect(matchesGameCardFilters(plain, filter)).toBe(true);
  });
});

describe('sortGameCards', () => {
  it('keeps favorites above non-favorites regardless of title sort', () => {
    const cards = [
      createGameSummary({ game_id: 'a', title: 'Alpha', is_favorite: false }),
      createGameSummary({ game_id: 'b', title: 'Bravo', is_favorite: true }),
    ];

    const sorted = sortGameCards(cards, { field: 'title', direction: 'asc' });
    expect(sorted.map((card) => card.game_id)).toEqual(['b', 'a']);
  });

  it('sorts by risk using the full severity ladder', () => {
    const cards = [
      createGameSummary({ game_id: 'high', risk_level: 'high' }),
      createGameSummary({ game_id: 'safe', risk_level: 'safe' }),
      createGameSummary({ game_id: 'blocked', risk_level: 'blocked' }),
    ];

    const sorted = sortGameCards(cards, { field: 'risk', direction: 'asc' });
    expect(sorted.map((card) => card.game_id)).toEqual(['safe', 'high', 'blocked']);
  });
});
