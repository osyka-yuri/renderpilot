import type { GameCardViewModel } from '@features/games/games-screen-model';

type RequiredGameFixtureInput = Pick<GameCardViewModel, 'id' | 'title'> & {
  libraries: readonly GameCardViewModel['libraries'][number][];
};

export type GameFixtureInput = RequiredGameFixtureInput &
  Partial<Omit<GameCardViewModel, keyof RequiredGameFixtureInput>>;

const DEFAULT_UPDATE_BADGE = {
  label: 'Up to date',
  tone: 'muted',
} satisfies GameCardViewModel['updateBadge'];

const DEFAULT_COVER_SRC = null satisfies GameCardViewModel['coverSrc'];

function createDefaultInstallPath(id: GameCardViewModel['id']): GameCardViewModel['installPath'] {
  return `/games/${id}`;
}

function createDefaultMonogram(title: GameCardViewModel['title']): GameCardViewModel['monogram'] {
  const normalizedTitle = title.trim();

  if (normalizedTitle.length === 0) {
    return '';
  }

  return normalizedTitle.slice(0, 2).toUpperCase();
}

export function createGameCardViewModel(input: GameFixtureInput): GameCardViewModel {
  return {
    id: input.id,
    title: input.title,
    installPath: input.installPath ?? createDefaultInstallPath(input.id),
    monogram: input.monogram ?? createDefaultMonogram(input.title),
    updateBadge: input.updateBadge ?? DEFAULT_UPDATE_BADGE,
    libraries: [...input.libraries],
    coverSrc: input.coverSrc ?? DEFAULT_COVER_SRC,
    hasCover: input.hasCover ?? false,
  };
}

export const regressionLibraryCatalog = ['LibraryAlpha', 'LibraryBeta'] as const;
