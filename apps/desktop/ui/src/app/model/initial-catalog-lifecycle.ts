import type { CatalogDelta, CatalogSyncState } from '@entities/game';
import { reportClientError } from '@shared/errors';

type StopListener = () => void;

export type CatalogEventPayloads = {
  'catalog://delta': CatalogDelta;
  'catalog://sync-state': CatalogSyncState;
};

export type CatalogEventListener = <TEvent extends keyof CatalogEventPayloads>(
  event: TEvent,
  onPayload: (payload: CatalogEventPayloads[TEvent]) => void,
) => Promise<StopListener>;

export type InitialCatalogSyncCompletion = {
  forceCatalogRefresh: boolean;
};

export type InitialCatalogLifecycleDeps = {
  previewMode: boolean;
  listenEvent: CatalogEventListener;
  startBackgroundRefresh: () => Promise<{ started: boolean; partialFailureCount: number }>;
  startUpdater: () => void;
  onCatalogDelta: (delta: CatalogDelta) => void;
  onPartialScanFailures: (count: number) => void;
  completeInitialCatalogSync: (completion: InitialCatalogSyncCompletion) => Promise<void>;
  enableCoverHydration: () => void;
  reportError?: (message: string, error: unknown) => void;
};

export function createInitialCatalogLifecycle(deps: InitialCatalogLifecycleDeps) {
  let disposed = false;
  let servicesStarted = false;
  let initialSyncCompletion: Promise<void> | null = null;
  let stopCatalogDelta: StopListener = () => undefined;
  let stopCatalogSyncState: StopListener = () => undefined;

  const reportError =
    deps.reportError ??
    ((_message: string, error: unknown) => {
      reportClientError('initial_catalog_lifecycle', error);
    });

  const retainListener = (
    promise: Promise<StopListener>,
    retain: (stop: StopListener) => void,
    failureMessage: string,
  ): Promise<boolean> =>
    promise
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          retain(stop);
        }
        return true;
      })
      .catch((error: unknown) => {
        reportError(failureMessage, error);
        return false;
      });

  let catalogDeltaListenerAvailable = false;
  const completeInitialSync = (): Promise<void> => {
    initialSyncCompletion ??= deps.completeInitialCatalogSync({
      forceCatalogRefresh: !catalogDeltaListenerAvailable,
    });
    return initialSyncCompletion;
  };
  const catalogDeltaListenerReady = deps.previewMode
    ? Promise.resolve(false)
    : retainListener(
        deps.listenEvent('catalog://delta', deps.onCatalogDelta),
        (stop) => {
          stopCatalogDelta = stop;
        },
        'Failed to listen for catalog deltas.',
      );

  const catalogSyncStateListenerReady = deps.previewMode
    ? Promise.resolve(false)
    : retainListener(
        deps.listenEvent('catalog://sync-state', (state) => {
          if (state === 'ready') {
            void completeInitialSync();
          }
        }),
        (stop) => {
          stopCatalogSyncState = stop;
        },
        'Failed to listen for catalog sync state.',
      );

  function startServices(): void {
    if (servicesStarted || disposed) {
      return;
    }
    servicesStarted = true;
    deps.startUpdater();

    void Promise.all([catalogDeltaListenerReady, catalogSyncStateListenerReady]).then(
      async ([deltaListenerAvailable]) => {
        catalogDeltaListenerAvailable = deltaListenerAvailable;
        if (disposed) {
          return;
        }
        try {
          const refresh = await deps.startBackgroundRefresh();
          reportPartialScanFailuresIfActive(refresh.partialFailureCount);
          // The command resolves after the backend coordinator has attempted
          // both delta and ready publication. This fallback also covers a
          // sync-state listener or ready-event failure and is idempotent with
          // `ready`.
          await completeInitialSync();
        } catch (error: unknown) {
          reportError('Failed to start background catalog refresh.', error);
          await completeInitialSync();
        } finally {
          enableCoverHydrationIfActive();
        }
      },
    );
  }

  function enableCoverHydrationIfActive(): void {
    if (!disposed) {
      // The backend command resolves only after startup cover GC, so cover
      // downloads cannot race an orphan snapshot captured before bootstrap.
      deps.enableCoverHydration();
    }
  }

  function reportPartialScanFailuresIfActive(count: number): void {
    if (!disposed && count > 0) {
      deps.onPartialScanFailures(count);
    }
  }

  function dispose(): void {
    disposed = true;
    stopCatalogDelta();
    stopCatalogSyncState();
  }

  return { startServices, dispose };
}
