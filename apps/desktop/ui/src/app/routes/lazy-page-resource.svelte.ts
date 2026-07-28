export type LazyPageState<TComponent> =
  | { readonly status: 'idle' }
  | { readonly status: 'loading' }
  | { readonly status: 'ready'; readonly component: TComponent }
  | { readonly status: 'error'; readonly error: unknown };

export type LazyPageResource<TComponent> = {
  readonly state: LazyPageState<TComponent>;
  preload: () => Promise<void>;
  activate: () => Promise<void>;
  retry: () => Promise<void>;
};

type LoadMode = 'preload' | 'foreground';

type CreateLazyPageResourceOptions<TComponent> = {
  id: string;
  loader: () => Promise<TComponent>;
};

export function createLazyPageResource<TComponent>(
  options: CreateLazyPageResourceOptions<TComponent>,
): LazyPageResource<TComponent> {
  let state = $state<LazyPageState<TComponent>>({ status: 'idle' });
  let inFlight: Promise<void> | null = null;
  let foregroundRequested = false;
  let preloadBlocked = false;

  function request(mode: LoadMode): Promise<void> {
    if (state.status === 'ready') {
      return Promise.resolve();
    }

    if (mode === 'preload' && (preloadBlocked || state.status === 'error')) {
      return Promise.resolve();
    }

    if (mode === 'foreground') {
      foregroundRequested = true;
      preloadBlocked = false;
    }

    if (inFlight !== null) {
      return inFlight;
    }

    state = { status: 'loading' };
    foregroundRequested = mode === 'foreground';

    const attempt = Promise.resolve()
      .then(options.loader)
      .then((component) => {
        preloadBlocked = false;
        state = { status: 'ready', component };
      })
      .catch((error: unknown) => {
        if (foregroundRequested) {
          state = { status: 'error', error };
          console.error(`Failed to load ${options.id} page.`, error);
        } else {
          preloadBlocked = true;
          state = { status: 'idle' };
          console.warn(`Failed to preload ${options.id} page.`, error);
        }
      })
      .finally(() => {
        if (inFlight === attempt) {
          inFlight = null;
          foregroundRequested = false;
        }
      });

    inFlight = attempt;
    return attempt;
  }

  function preload(): Promise<void> {
    return request('preload');
  }

  function activate(): Promise<void> {
    return request('foreground');
  }

  function retry(): Promise<void> {
    if (state.status !== 'error') {
      return inFlight ?? Promise.resolve();
    }

    return request('foreground');
  }

  return {
    get state() {
      return state;
    },
    preload,
    activate,
    retry,
  };
}
