export async function loadGameDetailsPage() {
  return (await import('./ui/GameDetailsPage.svelte')).default;
}

export {
  createGameDetailsPageModel,
  type GameDetailsPageModelDeps,
  type RollbackHandler,
} from './model/create-game-details-page-model';
