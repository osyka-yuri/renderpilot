import type { GameCardsQuery } from './types';

/**
 * Desktop UI loads the full filtered catalog snapshot in one request (no paging UX).
 * Both the shell (`DesktopApp`) and the games grid must use the same cap so lists
 * cannot diverge when the catalog grows.
 */
export const DEFAULT_DESKTOP_GAME_CARDS_PAGE_LIMIT = 10_000;

export const DEFAULT_GAME_CARDS_CATALOG_SORT = {
  field: 'title',
  direction: 'asc',
} as const satisfies GameCardsQuery['sort'];

export const DEFAULT_GAME_CARDS_CATALOG_PAGE = {
  limit: DEFAULT_DESKTOP_GAME_CARDS_PAGE_LIMIT,
  offset: 0,
} as const satisfies GameCardsQuery['page'];
