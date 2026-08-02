import { LocaleLoadError } from './errors';
import type { LanguageMode, Locale } from './locale-model';
import type { LocalePack } from './packs/types';
import {
  createActiveState,
  createErrorState,
  createLoadingState,
  createReadyState,
  shouldObserveSystemLanguage,
} from './runtime-state';
import type { I18nInitializationResult, I18nRuntimeState, I18nSwitchResult } from './runtime-types';

type TransitionRequest = Readonly<{
  mode: LanguageMode;
  locale: Locale;
  persistModeOnCommit: boolean;
}>;

type UserOperation = {
  readonly initialRequest: TransitionRequest;
  readonly promise: Promise<I18nSwitchResult>;
  readonly resolve: (result: I18nSwitchResult) => void;
  readonly reject: (error: LocaleLoadError) => void;
  settled: boolean;
};

type ActiveTransition = Readonly<{
  id: number;
  request: TransitionRequest;
  userOperation: UserOperation | null;
}>;

export type I18nCoordinatorDependencies = Readonly<{
  fallbackPack: LocalePack;
  readStoredMode: () => LanguageMode;
  persistMode: (mode: LanguageMode) => void;
  resolveMode: (mode: LanguageMode) => Locale;
  observeSystemLanguage: (listener: () => void) => () => void;
  getLoadedPack: (locale: Locale) => LocalePack | undefined;
  loadPack: (locale: Locale) => Promise<LocalePack>;
}>;

export type I18nCoordinatorHost = Readonly<{
  getState: () => I18nRuntimeState;
  publishState: (state: I18nRuntimeState) => void;
  activatePack: (pack: LocalePack) => void;
}>;

export function createI18nCoordinator(
  deps: I18nCoordinatorDependencies,
  host: I18nCoordinatorHost,
) {
  let latestTransitionId = 0;
  let systemLanguageRevision = 0;
  let initialized = false;
  let initializationPromise: Promise<I18nInitializationResult> | null = null;
  let activeTransition: ActiveTransition | null = null;
  let unsubscribeSystemLanguage: (() => void) | null = null;

  function initializeI18n(): Promise<I18nInitializationResult> {
    initializationPromise ??= initializeInternal();
    return initializationPromise;
  }

  async function initializeInternal(): Promise<I18nInitializationResult> {
    const storedMode = deps.readStoredMode();
    let targetLocale = deps.resolveMode(storedMode);

    publishState(createLoadingState(host.getState(), storedMode, targetLocale));

    for (;;) {
      const observedSystemRevision = systemLanguageRevision;

      try {
        const pack = await deps.loadPack(targetLocale);
        const latestTarget = resolveLatestStartupTarget(
          storedMode,
          targetLocale,
          observedSystemRevision,
        );
        if (latestTarget !== targetLocale) {
          targetLocale = latestTarget;
          publishState(createLoadingState(host.getState(), storedMode, targetLocale));
          continue;
        }

        host.activatePack(pack);
        initialized = true;
        publishState(createReadyState(storedMode, pack));

        return {
          activeMode: storedMode,
          activeLocale: pack.locale,
          fallbackUsed: false,
          error: null,
        };
      } catch (cause) {
        const latestTarget = resolveLatestStartupTarget(
          storedMode,
          targetLocale,
          observedSystemRevision,
        );
        if (latestTarget !== targetLocale) {
          targetLocale = latestTarget;
          publishState(createLoadingState(host.getState(), storedMode, targetLocale));
          continue;
        }

        const error = toLocaleLoadError(storedMode, targetLocale, cause);
        try {
          host.activatePack(deps.fallbackPack);
        } catch {
          // The host starts with the fallback pack active. Reflecting it in the
          // document is best-effort when initialization is already failing.
        }
        initialized = true;
        publishState(
          createErrorState(
            storedMode === 'system' ? 'system' : 'en',
            deps.fallbackPack.locale,
            error,
          ),
        );

        const state = host.getState();
        return {
          activeMode: state.activeMode,
          activeLocale: state.activeLocale,
          fallbackUsed: true,
          error,
        };
      }
    }
  }

  function setLanguageMode(mode: LanguageMode): Promise<I18nSwitchResult> {
    if (initializationPromise !== null && !initialized) {
      return initializationPromise.then(() => setLanguageMode(mode));
    }

    const request: TransitionRequest = {
      mode,
      locale: deps.resolveMode(mode),
      persistModeOnCommit: true,
    };
    const currentTransition = activeTransition;
    const currentUserOperation = currentTransition?.userOperation;
    if (
      currentTransition !== null &&
      currentUserOperation !== null &&
      currentUserOperation !== undefined &&
      isSameTransitionRequest(currentTransition.request, request)
    ) {
      return currentUserOperation.promise;
    }

    const userOperation = createUserOperation(request);
    requestTransition(request, userOperation);
    return userOperation.promise;
  }

  function requestTransition(
    request: TransitionRequest,
    userOperation: UserOperation | null,
  ): void {
    if (
      activeTransition !== null &&
      isSameTransitionRequest(activeTransition.request, request) &&
      activeTransition.userOperation === userOperation
    ) {
      return;
    }

    supersedeActiveUserOperation(userOperation);

    const transitionId = ++latestTransitionId;
    const transition: ActiveTransition = {
      id: transitionId,
      request,
      userOperation,
    };

    const loadedPack = deps.getLoadedPack(request.locale);
    if (loadedPack !== undefined) {
      finishWinningTransition(transition, loadedPack);
      return;
    }

    activeTransition = transition;

    publishState(createLoadingState(host.getState(), request.mode, request.locale));
    void loadAndCommit(transition);
  }

  async function loadAndCommit(transition: ActiveTransition): Promise<void> {
    try {
      const pack = await deps.loadPack(transition.request.locale);
      if (!isWinningTransition(transition)) {
        return;
      }

      finishWinningTransition(transition, pack);
    } catch (cause) {
      if (!isWinningTransition(transition)) {
        return;
      }

      failWinningTransition(transition, cause);
    }
  }

  function finishWinningTransition(transition: ActiveTransition, pack: LocalePack): void {
    activeTransition = null;

    try {
      host.activatePack(pack);
      publishState(createActiveState(initialized, transition.request.mode, pack));
      persistCommittedMode(transition.request);
      resolveUserOperation(transition.userOperation, {
        outcome: 'applied',
        mode: transition.request.mode,
        locale: transition.request.locale,
      });
    } catch (cause) {
      publishTransitionFailure(transition, cause);
    }
  }

  function failWinningTransition(transition: ActiveTransition, cause: unknown): void {
    activeTransition = null;
    publishTransitionFailure(transition, cause);
  }

  function publishTransitionFailure(transition: ActiveTransition, cause: unknown): void {
    const error = toLocaleLoadError(transition.request.mode, transition.request.locale, cause);
    const current = host.getState();
    publishState(createErrorState(current.activeMode, current.activeLocale, error));
    rejectUserOperation(transition.userOperation, error);

    if (transition.request.mode !== 'system' && host.getState().activeMode === 'system') {
      queueMicrotask(reconcileSystemLocale);
    }
  }

  function persistCommittedMode(request: TransitionRequest): void {
    if (!request.persistModeOnCommit) {
      return;
    }

    try {
      deps.persistMode(request.mode);
    } catch {
      // Persistence failures must not roll back an already committed pack.
    }
  }

  function resolveLatestStartupTarget(
    mode: LanguageMode,
    loadedTarget: Locale,
    observedSystemRevision: number,
  ): Locale {
    if (mode !== 'system' || observedSystemRevision === systemLanguageRevision) {
      return loadedTarget;
    }

    return deps.resolveMode('system');
  }

  function publishState(nextState: I18nRuntimeState): void {
    host.publishState(nextState);
    synchronizeSystemLanguageObserver(nextState);
  }

  function synchronizeSystemLanguageObserver(state: I18nRuntimeState): void {
    if (shouldObserveSystemLanguage(state)) {
      if (unsubscribeSystemLanguage === null) {
        try {
          unsubscribeSystemLanguage = deps.observeSystemLanguage(handleSystemLanguageChange);
        } catch {
          unsubscribeSystemLanguage = noop;
        }
      }
      return;
    }

    if (unsubscribeSystemLanguage !== null) {
      try {
        unsubscribeSystemLanguage();
      } catch {
        // A teardown failure must not block the requested explicit locale.
      }
      unsubscribeSystemLanguage = null;
    }
  }

  function handleSystemLanguageChange(): void {
    systemLanguageRevision += 1;

    const state = host.getState();
    const intendedMode = state.pending?.mode ?? state.activeMode;
    if (!initialized || intendedMode !== 'system') {
      return;
    }

    const retargetedTransition =
      activeTransition?.request.mode === 'system' ? activeTransition : null;
    requestTransition(
      {
        mode: 'system',
        locale: deps.resolveMode('system'),
        persistModeOnCommit: retargetedTransition?.request.persistModeOnCommit ?? false,
      },
      retargetedTransition?.userOperation ?? null,
    );
  }

  function reconcileSystemLocale(): void {
    const state = host.getState();
    if (state.pending !== null || state.activeMode !== 'system') {
      return;
    }

    const locale = deps.resolveMode('system');
    if (locale === state.activeLocale) {
      return;
    }

    requestTransition(
      {
        mode: 'system',
        locale,
        persistModeOnCommit: false,
      },
      null,
    );
  }

  function isWinningTransition(transition: ActiveTransition): boolean {
    return activeTransition?.id === transition.id && latestTransitionId === transition.id;
  }

  function supersedeActiveUserOperation(nextUserOperation: UserOperation | null): void {
    const currentUserOperation = activeTransition?.userOperation;
    if (
      currentUserOperation === undefined ||
      currentUserOperation === null ||
      currentUserOperation === nextUserOperation
    ) {
      return;
    }

    resolveUserOperation(currentUserOperation, {
      outcome: 'superseded',
      mode: currentUserOperation.initialRequest.mode,
      locale: currentUserOperation.initialRequest.locale,
    });
  }

  return { initializeI18n, setLanguageMode };
}

function isSameTransitionRequest(first: TransitionRequest, second: TransitionRequest): boolean {
  return (
    first.mode === second.mode &&
    first.locale === second.locale &&
    first.persistModeOnCommit === second.persistModeOnCommit
  );
}

function createUserOperation(initialRequest: TransitionRequest): UserOperation {
  let resolve!: (result: I18nSwitchResult) => void;
  let reject!: (error: LocaleLoadError) => void;
  const promise = new Promise<I18nSwitchResult>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });

  return { initialRequest, promise, resolve, reject, settled: false };
}

function resolveUserOperation(userOperation: UserOperation | null, result: I18nSwitchResult): void {
  if (userOperation === null || userOperation.settled) {
    return;
  }

  userOperation.settled = true;
  userOperation.resolve(result);
}

function rejectUserOperation(userOperation: UserOperation | null, error: LocaleLoadError): void {
  if (userOperation === null || userOperation.settled) {
    return;
  }

  userOperation.settled = true;
  userOperation.reject(error);
}

function toLocaleLoadError(mode: LanguageMode, locale: Locale, cause: unknown): LocaleLoadError {
  return cause instanceof LocaleLoadError ? cause : new LocaleLoadError(mode, locale, cause);
}

function noop(): void {
  return;
}
