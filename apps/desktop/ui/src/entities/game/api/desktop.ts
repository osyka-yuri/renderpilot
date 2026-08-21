import { invokeDesktop } from '@shared/api';
import { requireNonBlankString } from '@shared/validation';
import { normalizeGameCardsQuery } from './game-cards-query';
import type {
  CoverArtworkResult,
  GameCardsQuery,
  GameCardsResult,
  GameDetails,
  GameFileSafetyAssessment,
  GamesCatalogBootstrap,
  RemoveGameFromCatalogResult,
  SharedVulkanSafetyAssessment,
} from '../model/types';

export async function bootstrapGamesCatalog(limit = 120): Promise<GamesCatalogBootstrap> {
  return invokeDesktop<GamesCatalogBootstrap>('bootstrap_games_catalog', { limit });
}

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

/** Fetches a fresh file-safety assessment outside the cached Game Details query. */
export async function getGameFileSafetyAssessment(
  gameId: string,
): Promise<GameFileSafetyAssessment> {
  return invokeDesktop<GameFileSafetyAssessment>('get_game_file_safety_assessment', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}

/** Fetches a fresh assessment for changes to the shared Vulkan layer. */
export async function getSharedVulkanSafetyAssessment(): Promise<SharedVulkanSafetyAssessment> {
  return invokeDesktop<SharedVulkanSafetyAssessment>('get_shared_vulkan_safety_assessment');
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

export async function removeGameFromCatalog(gameId: string): Promise<RemoveGameFromCatalogResult> {
  return invokeDesktop<RemoveGameFromCatalogResult>('remove_game_from_catalog', {
    gameId: requireNonBlankString(gameId, 'gameId'),
  });
}
