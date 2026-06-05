import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import { normalizeGameCardsQuery } from './game-cards-query';
import type {
  CoverArtworkResult,
  GameCardsQuery,
  GameCardsResult,
  GameDetails,
} from '../model/types';

export async function queryGameCards(query: GameCardsQuery): Promise<GameCardsResult> {
  return invokeDesktop<GameCardsResult>('query_game_cards', {
    query: normalizeGameCardsQuery(query),
  });
}

export async function fetchGameCover(gameId: string): Promise<CoverArtworkResult> {
  return invokeDesktop<CoverArtworkResult>('fetch_game_cover', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function clearGameCover(gameId: string): Promise<{ cleared: boolean }> {
  return invokeDesktop<{ cleared: boolean }>('clear_game_cover', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function setGameCover(
  gameId: string,
  sourcePath: string,
): Promise<CoverArtworkResult> {
  return invokeDesktop<CoverArtworkResult>('set_game_cover', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    sourcePath: requireNonBlankString(sourcePath, 'sourcePath'),
  });
}

export async function getGameDetails(gameId: string): Promise<GameDetails> {
  return invokeDesktop('get_game_details', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

export async function setGameFavorite(
  gameId: string,
  isFavorite: boolean,
): Promise<{ saved: boolean }> {
  return invokeDesktop('set_game_favorite', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    isFavorite,
  });
}

export async function setGameHidden(
  gameId: string,
  isHidden: boolean,
): Promise<{ saved: boolean }> {
  return invokeDesktop('set_game_hidden', {
    gameId: requireNonBlankString(gameId, 'gameId'),
    isHidden,
  });
}
