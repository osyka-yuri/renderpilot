import type { Screen } from '@app/navigation/screen';
import { loadGameDetailsPage } from '@pages/game-details';
import { loadLibrariesPage } from '@pages/libraries';
import { loadOperationsPage } from '@pages/operations';
import { loadSettingsPage } from '@pages/settings';

import { createLazyPageResource, type LazyPageResource } from './lazy-page-resource.svelte';

type LazyPageScreen = Exclude<Screen, 'games'>;

export function createDesktopPageRegistry() {
  const pages = {
    details: createLazyPageResource({
      id: 'details',
      loader: loadGameDetailsPage,
    }),
    operations: createLazyPageResource({
      id: 'operations',
      loader: loadOperationsPage,
    }),
    settings: createLazyPageResource({
      id: 'settings',
      loader: loadSettingsPage,
    }),
    libraries: createLazyPageResource({
      id: 'libraries',
      loader: loadLibrariesPage,
    }),
  } satisfies Record<LazyPageScreen, LazyPageResource<unknown>>;

  function preload(screen: Screen): Promise<void> {
    return screen === 'games' ? Promise.resolve() : pages[screen].preload();
  }

  return {
    ...pages,
    preload,
  };
}
