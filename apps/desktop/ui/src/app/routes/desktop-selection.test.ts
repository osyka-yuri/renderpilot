import { describe, expect, it } from 'vitest';
import type { Screen } from '@app/routes/screen';
import type { GameCard, GameDetails } from '@shared/api/types';
import {
  canonicalGameIdentityId,
  findGameCardForSelection,
  normalizeSelectableGameId,
  resolveSelectedGameDetails,
  type ResolveSelectedGameDetailsInput,
  workspaceShellGameTitle,
} from '@app/routes/desktop-selection';

const DEFAULT_GAME_ID = 'manual:D:/SteamLibrary/x';
const DEFAULT_GAME_TITLE = 'Sample';

const DETAILS_SCREEN: Screen = 'details';
const OPERATIONS_SCREEN: Screen = 'operations';
const GAMES_SCREEN: Screen = 'games';

const WORKSPACE_SCREENS: readonly Screen[] = [DETAILS_SCREEN, OPERATIONS_SCREEN];

type GameDetailsOverrides = {
  readonly identityId?: unknown;
  readonly title?: string;
};

function createGameDetails(overrides: GameDetailsOverrides = {}): GameDetails {
  const identityId = 'identityId' in overrides ? overrides.identityId : DEFAULT_GAME_ID;
  const title = overrides.title ?? DEFAULT_GAME_TITLE;

  return {
    game: {
      identity: {
        id: identityId as GameDetails['game']['identity']['id'],
        title,
        launcher: 'manual',
      },
      platform: 'windows',
      runtime: 'unknown',
      install_path: 'D:/games/x',
      executable_candidates: [],
    },
    components: [],
    candidate_groups: [],
    operations: [],
  };
}

function createGameCard(gameId = DEFAULT_GAME_ID, title = DEFAULT_GAME_TITLE): GameCard {
  return {
    game_id: gameId,
    title,
    launcher: 'manual',
    platform: 'windows',
    runtime: 'unknown',
    install_path: 'D:/games/x',
    library_tags: [],
    component_count: 0,
    updates_available: false,
    update_count: 0,
    risk_level: 'unknown',
    backup_available: false,
    operation_count: 0,
  };
}

function expectResolvedDetails(
  input: ResolveSelectedGameDetailsInput,
  expectedDetails: GameDetails,
): void {
  expect(resolveSelectedGameDetails(input)).toBe(expectedDetails);
}

function expectRejectedDetails(input: ResolveSelectedGameDetailsInput): void {
  expect(resolveSelectedGameDetails(input)).toBeNull();
}

function expectCanonicalIdentity(identityId: unknown, expected: string | null): void {
  expect(canonicalGameIdentityId(createGameDetails({ identityId }))).toBe(expected);
}

describe('desktop-selection', () => {
  describe('normalizeSelectableGameId', () => {
    it.each([
      ['  manual:a  ', 'manual:a'],
      ['\n\tmanual:a\r\n', 'manual:a'],
      ['', ''],
      ['   ', ''],
    ])('normalizes "%s" to "%s"', (input, expected) => {
      expect(normalizeSelectableGameId(input)).toBe(expected);
    });
  });

  describe('canonicalGameIdentityId', () => {
    it.each<[unknown, string]>([
      ['  steam:123  ', 'steam:123'],
      [1091500, '1091500'],
      [0, '0'],
      [true, 'true'],
      [false, 'false'],
      [10n, '10'],
    ])('normalizes supported identity id %#', (identityId, expected) => {
      expectCanonicalIdentity(identityId, expected);
    });

    it.each<[unknown]>([
      [''],
      ['   '],
      [Number.NaN],
      [Number.POSITIVE_INFINITY],
      [Number.NEGATIVE_INFINITY],
      [{ nested: true }],
      [[]],
      [Symbol('game-id')],
      [undefined],
      [null],
    ])('returns null for unsupported or empty identity id %#', (identityId) => {
      expectCanonicalIdentity(identityId, null);
    });

    it('returns null when details are absent', () => {
      expect(canonicalGameIdentityId(null)).toBeNull();
    });
  });

  describe('resolveSelectedGameDetails', () => {
    it.each(WORKSPACE_SCREENS)(
      'resolves %s screen when details id matches explicit selected id',
      (activeScreen) => {
        const details = createGameDetails();

        expectResolvedDetails(
          {
            activeScreen,
            selectedGameId: DEFAULT_GAME_ID,
            currentDetails: details,
          },
          details,
        );
      },
    );

    it.each(WORKSPACE_SCREENS)(
      'resolves %s screen without selected id when details have a valid canonical id',
      (activeScreen) => {
        const details = createGameDetails();

        expectResolvedDetails(
          {
            activeScreen,
            selectedGameId: null,
            currentDetails: details,
          },
          details,
        );
      },
    );

    it.each(WORKSPACE_SCREENS)(
      'rejects %s screen when explicit selected id mismatches details id',
      (activeScreen) => {
        expectRejectedDetails({
          activeScreen,
          selectedGameId: 'manual:OTHER',
          currentDetails: createGameDetails(),
        });
      },
    );

    it('resolves non-workspace screen only when selected id matches details id', () => {
      const details = createGameDetails();

      expectResolvedDetails(
        {
          activeScreen: GAMES_SCREEN,
          selectedGameId: DEFAULT_GAME_ID,
          currentDetails: details,
        },
        details,
      );
    });

    it('rejects non-workspace screen when selected id is missing', () => {
      expectRejectedDetails({
        activeScreen: GAMES_SCREEN,
        selectedGameId: null,
        currentDetails: createGameDetails(),
      });
    });

    it('rejects non-workspace screen when selected id mismatches details id', () => {
      expectRejectedDetails({
        activeScreen: GAMES_SCREEN,
        selectedGameId: 'manual:OTHER',
        currentDetails: createGameDetails(),
      });
    });

    it('normalizes selected id and details id before comparing them', () => {
      const details = createGameDetails({
        identityId: `  ${DEFAULT_GAME_ID}  `,
      });

      expectResolvedDetails(
        {
          activeScreen: GAMES_SCREEN,
          selectedGameId: `  ${DEFAULT_GAME_ID}\n`,
          currentDetails: details,
        },
        details,
      );
    });

    it('rejects blank selected id as an explicit mismatch', () => {
      expectRejectedDetails({
        activeScreen: DETAILS_SCREEN,
        selectedGameId: '   ',
        currentDetails: createGameDetails(),
      });
    });

    it('returns null when details are absent', () => {
      expectRejectedDetails({
        activeScreen: DETAILS_SCREEN,
        selectedGameId: DEFAULT_GAME_ID,
        currentDetails: null,
      });
    });

    it('returns null when details have no canonical id', () => {
      expectRejectedDetails({
        activeScreen: DETAILS_SCREEN,
        selectedGameId: DEFAULT_GAME_ID,
        currentDetails: createGameDetails({ identityId: '   ' }),
      });
    });
  });

  describe('findGameCardForSelection', () => {
    const cards: readonly GameCard[] = [
      createGameCard('  manual:A ', 'Alpha'),
      createGameCard('manual:B', 'Beta'),
      createGameCard('manual:C', 'Gamma'),
    ];

    it.each([null, '', '   '])('returns null for empty selection %#', (selectionId) => {
      expect(findGameCardForSelection(selectionId, cards)).toBeNull();
    });

    it('matches after trimming selection id and card id', () => {
      expect(findGameCardForSelection('manual:A ', cards)).toBe(cards[0]);
    });

    it('returns the matching card by canonical id', () => {
      expect(findGameCardForSelection('manual:B', cards)).toBe(cards[1]);
    });

    it('returns null when no card matches the selection', () => {
      expect(findGameCardForSelection('manual:UNKNOWN', cards)).toBeNull();
    });

    it('returns the first matching card when duplicated normalized ids exist', () => {
      const duplicatedCards: readonly GameCard[] = [
        createGameCard(' manual:A ', 'First'),
        createGameCard('manual:A', 'Second'),
      ];

      expect(findGameCardForSelection('manual:A', duplicatedCards)).toBe(duplicatedCards[0]);
    });
  });

  describe('workspaceShellGameTitle', () => {
    it('prefers non-empty card title over details title', () => {
      expect(
        workspaceShellGameTitle(
          createGameCard(DEFAULT_GAME_ID, '  Card wins  '),
          createGameDetails({ title: 'From details' }),
        ),
      ).toBe('Card wins');
    });

    it('falls back to details title when card is absent', () => {
      expect(workspaceShellGameTitle(null, createGameDetails({ title: '  From details  ' }))).toBe(
        'From details',
      );
    });

    it('falls back to details title when card title is blank', () => {
      expect(
        workspaceShellGameTitle(
          createGameCard(DEFAULT_GAME_ID, '   '),
          createGameDetails({ title: '  From details  ' }),
        ),
      ).toBe('From details');
    });

    it('returns null when card and details titles are blank', () => {
      expect(
        workspaceShellGameTitle(
          createGameCard(DEFAULT_GAME_ID, '   '),
          createGameDetails({ title: '  ' }),
        ),
      ).toBeNull();
    });

    it('returns null when both card and details are absent', () => {
      expect(workspaceShellGameTitle(null, null)).toBeNull();
    });
  });
});
