import {
  type GameCardViewModel,
  type GameCardMenuHandle,
  type Launcher,
  getLauncherDisplayLabel,
} from '@entities/game';

export type GameId = GameCardViewModel['id'];

export type CoverBusyPredicate = (gameId: GameId) => boolean;

export type ActionMenuRefs = Readonly<Partial<Record<GameId, GameCardMenuHandle>>>;

export type CardStateContext = {
  busy: boolean;
  hasManualCoverAction: boolean;
  pickDisabled: boolean;
  coversAutoFetchingIds: ReadonlySet<GameId>;
  menuOpenFor: GameId | null;
  actionMenuRefs: ActionMenuRefs;
  isCoverOperationBusy: CoverBusyPredicate;
};

export type GameCardState = {
  game: GameCardViewModel;
  id: GameId;
  isCoverBusy: boolean;
  isBackgroundCoverFetching: boolean;
  isMenuDisabled: boolean;
  isPickDisabled: boolean;
  isMenuOpen: boolean;
  menuRef?: GameCardMenuHandle;
};

export type LauncherGroup = {
  launcher: Launcher;
  label: string;
  cards: readonly GameCardState[];
};

type LauncherGameGroup = {
  launcher: Launcher;
  label: string;
  games: readonly GameCardViewModel[];
};

/**
 * Groups games by launcher (in display order) and resolves each card's derived
 * UI state. Pure: the caller passes the reactive `cardContext`, so this can be
 * unit-tested and kept out of the component markup.
 */
export function createLauncherGroups(
  games: readonly GameCardViewModel[],
  launcherOrder: readonly Launcher[],
  cardContext: CardStateContext,
): LauncherGroup[] {
  return groupGamesByLauncher(games, launcherOrder).map((group) => ({
    launcher: group.launcher,
    label: group.label,
    cards: group.games.map((game) => createGameCardState(game, cardContext)),
  }));
}

function createGameCardState(game: GameCardViewModel, context: CardStateContext): GameCardState {
  const id = game.id;
  const isBackgroundCoverFetching = context.coversAutoFetchingIds.has(id);

  return {
    game,
    id,
    isCoverBusy: context.isCoverOperationBusy(id),
    isBackgroundCoverFetching,
    isMenuDisabled: context.busy || context.hasManualCoverAction || isBackgroundCoverFetching,
    isPickDisabled: context.pickDisabled,
    isMenuOpen: context.menuOpenFor === id,
    menuRef: context.actionMenuRefs[id],
  };
}

function groupGamesByLauncher(
  games: readonly GameCardViewModel[],
  launcherOrder: readonly Launcher[],
): LauncherGameGroup[] {
  if (games.length === 0) {
    return [];
  }

  const gamesByLauncher = createGamesByLauncherIndex(games);
  const launchers = getLaunchersInDisplayOrder(gamesByLauncher, launcherOrder);

  return launchers.map((launcher) =>
    createLauncherGameGroup(launcher, gamesByLauncher.get(launcher) ?? []),
  );
}

function createGamesByLauncherIndex(
  games: readonly GameCardViewModel[],
): Map<Launcher, GameCardViewModel[]> {
  const gamesByLauncher = new Map<Launcher, GameCardViewModel[]>();

  for (const game of games) {
    const groupGames = gamesByLauncher.get(game.launcher);

    if (groupGames) {
      groupGames.push(game);
    } else {
      gamesByLauncher.set(game.launcher, [game]);
    }
  }

  return gamesByLauncher;
}

function getLaunchersInDisplayOrder(
  gamesByLauncher: ReadonlyMap<Launcher, readonly GameCardViewModel[]>,
  launcherOrder: readonly Launcher[],
): Launcher[] {
  const orderedLaunchers = getExistingLaunchersFromOrder(gamesByLauncher, launcherOrder);
  const remainingLaunchers = getRemainingLaunchers(gamesByLauncher, orderedLaunchers);

  return [...orderedLaunchers, ...remainingLaunchers];
}

function getExistingLaunchersFromOrder(
  gamesByLauncher: ReadonlyMap<Launcher, readonly GameCardViewModel[]>,
  launcherOrder: readonly Launcher[],
): Launcher[] {
  const launchers: Launcher[] = [];

  for (const launcher of launcherOrder) {
    if (launchers.includes(launcher) || !gamesByLauncher.has(launcher)) {
      continue;
    }

    launchers.push(launcher);
  }

  return launchers;
}

function getRemainingLaunchers(
  gamesByLauncher: ReadonlyMap<Launcher, readonly GameCardViewModel[]>,
  orderedLaunchers: readonly Launcher[],
): Launcher[] {
  return Array.from(gamesByLauncher.keys())
    .filter((launcher) => !orderedLaunchers.includes(launcher))
    .sort(compareLaunchersByLabel);
}

function createLauncherGameGroup(
  launcher: Launcher,
  games: readonly GameCardViewModel[],
): LauncherGameGroup {
  return {
    launcher,
    label: getLauncherDisplayLabel(launcher),
    games,
  };
}

function compareLaunchersByLabel(left: Launcher, right: Launcher): number {
  return getLauncherDisplayLabel(left).localeCompare(getLauncherDisplayLabel(right), undefined, {
    sensitivity: 'base',
  });
}
