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
