import { describe, expect, it } from 'vitest';
import type { Screen } from './screen';
import type { GameDetails, GameSummary } from '@entities/game';
import {
  canonicalGameIdentityId,
  findGameSummaryForSelection,
  normalizeSelectableGameId,
} from '@entities/game';
import {
  isGameSelected,
  resolveSelectedGameDetails,
  type ResolveSelectedGameDetailsInput,
  workspaceShellGameTitle,
} from './selection';

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

function createGameSummary(gameId = DEFAULT_GAME_ID, title = DEFAULT_GAME_TITLE): GameSummary {
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
    rollback_available: false,
    operation_count: 0,
    is_favorite: false,
    is_hidden: false,
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

describe('selection helpers', () => {
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
    it('rejects when current details are absent', () => {
      expectRejectedDetails({
        activeScreen: DETAILS_SCREEN,
        selectedGameId: DEFAULT_GAME_ID,
        currentDetails: null,
      });
    });

    it('rejects when details do not have a canonical identity id', () => {
      expectRejectedDetails({
        activeScreen: DETAILS_SCREEN,
        selectedGameId: DEFAULT_GAME_ID,
        currentDetails: createGameDetails({ identityId: '   ' }),
      });
    });

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
  });

  describe('workspaceShellGameTitle', () => {
    it('prefers the game card title when present', () => {
      expect(
        workspaceShellGameTitle(createGameSummary('id', 'Card Title'), createGameDetails()),
      ).toBe('Card Title');
    });

    it('falls back to details title when card title is missing', () => {
      expect(workspaceShellGameTitle(createGameSummary('id', '   '), createGameDetails())).toBe(
        DEFAULT_GAME_TITLE,
      );
    });

    it('returns null when no usable title exists', () => {
      expect(workspaceShellGameTitle(null, createGameDetails({ title: '   ' }))).toBeNull();
    });
  });

  describe('findGameSummaryForSelection interplay', () => {
    it('keeps the same game summary lookup semantics', () => {
      const summary = createGameSummary();

      expect(findGameSummaryForSelection(DEFAULT_GAME_ID, [summary])).toBe(summary);
      expect(findGameSummaryForSelection('missing', [summary])).toBeNull();
    });
  });

  describe('isGameSelected', () => {
    it('returns true only for the same canonical game id', () => {
      expect(isGameSelected(DEFAULT_GAME_ID, DEFAULT_GAME_ID)).toBe(true);
      expect(isGameSelected(DEFAULT_GAME_ID, 'manual:OTHER')).toBe(false);
      expect(isGameSelected(null, DEFAULT_GAME_ID)).toBe(false);
    });
  });
});
