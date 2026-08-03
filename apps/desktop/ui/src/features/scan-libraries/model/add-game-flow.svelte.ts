import type { PresentedError } from '@shared/error-presentation';

import type { AddGameConfirmation, AddGameInspection, AddGameResult } from './add-game';
import { automaticAddGameConfirmation } from './add-game';

export type AddGameFlowState =
  | { kind: 'idle' }
  | { kind: 'choosing' }
  | { kind: 'inspecting'; selectedRoot: string }
  | {
      kind: 'review';
      inspection: AddGameInspection;
      errorPresentation: PresentedError | null;
    }
  | {
      kind: 'submitting';
      inspection: AddGameInspection;
      reviewed: boolean;
    }
  | { kind: 'rolling_back'; inspection: AddGameInspection };

export type AddGameDialogState = Extract<
  AddGameFlowState,
  { kind: 'review' | 'submitting' | 'rolling_back' }
>;

export type AddGameSubmitOutcome =
  | { kind: 'completed'; result: AddGameResult }
  | { kind: 'busy' }
  | { kind: 'failed'; error: unknown };

export type AddGameRollbackOutcome = { kind: 'completed' } | { kind: 'failed'; error: unknown };

type InlineInspectionOutcome =
  | { kind: 'completed'; inspection: AddGameInspection }
  | { kind: 'failed'; error: unknown };

export type AddGameFlowDeps = {
  chooseFolder: () => Promise<string | null>;
  inspect: (selectedRoot: string) => Promise<AddGameInspection>;
  submit: (
    inspection: AddGameInspection,
    confirmation: AddGameConfirmation,
  ) => Promise<AddGameSubmitOutcome>;
  rollback: (gameId: string, componentIds: string[]) => Promise<AddGameRollbackOutcome>;
  presentError: (error: unknown) => PresentedError;
  presentCatalogBusyError: () => PresentedError;
  publishError: (error: unknown) => void;
  requiresReinspection: (error: unknown) => boolean;
};

export type AddGameFlow = ReturnType<typeof createAddGameFlow>;

/** Owns the complete cancellable inspect/review/submit/root-correction workflow. */
export function createAddGameFlow(deps: AddGameFlowDeps) {
  let state = $state<AddGameFlowState>({ kind: 'idle' });
  let requestId = 0;

  const isCurrent = (request: number): boolean => request === requestId;

  function close(): void {
    requestId += 1;
    state = { kind: 'idle' };
  }

  async function chooseFolder(): Promise<void> {
    const request = ++requestId;
    state = { kind: 'choosing' };
    try {
      const selectedRoot = await deps.chooseFolder();
      if (!isCurrent(request)) {
        return;
      }
      if (selectedRoot === null) {
        state = { kind: 'idle' };
        return;
      }

      state = { kind: 'inspecting', selectedRoot };
      const inspection = await deps.inspect(selectedRoot);
      if (!isCurrent(request)) {
        return;
      }
      if (inspection.decision.kind !== 'automatic') {
        state = { kind: 'review', inspection, errorPresentation: null };
        return;
      }
      await submitInspection(
        inspection,
        automaticAddGameConfirmation(inspection.decision),
        request,
        false,
      );
    } catch (error) {
      if (!isCurrent(request)) {
        return;
      }
      deps.publishError(error);
      state = { kind: 'idle' };
    }
  }

  async function confirm(confirmation: AddGameConfirmation): Promise<void> {
    if (state.kind !== 'review') {
      return;
    }
    const inspection = state.inspection;
    const request = ++requestId;
    await submitInspection(inspection, confirmation, request, true);
  }

  async function submitInspection(
    inspection: AddGameInspection,
    confirmation: AddGameConfirmation,
    request: number,
    reviewed: boolean,
  ): Promise<void> {
    state = { kind: 'submitting', inspection, reviewed };
    try {
      const outcome = await deps.submit(inspection, confirmation);
      if (!isCurrent(request)) {
        return;
      }
      if (outcome.kind === 'completed') {
        state = { kind: 'idle' };
        return;
      }

      if (!reviewed && outcome.kind === 'failed' && !deps.requiresReinspection(outcome.error)) {
        deps.publishError(outcome.error);
        state = { kind: 'idle' };
        return;
      }

      const reinspection =
        outcome.kind === 'failed' && deps.requiresReinspection(outcome.error)
          ? await inspectInline(inspection.selectedRoot)
          : null;
      if (!isCurrent(request)) {
        return;
      }
      state = selectReviewState(deps, inspection, outcome, reinspection);
    } catch (error) {
      if (!isCurrent(request)) {
        return;
      }
      if (reviewed) {
        state = {
          kind: 'review',
          inspection,
          errorPresentation: deps.presentError(error),
        };
      } else {
        deps.publishError(error);
        state = { kind: 'idle' };
      }
    }
  }

  async function rollbackAndConfirm(confirmation: AddGameConfirmation): Promise<void> {
    if (state.kind !== 'review') {
      return;
    }
    const assessment = state.inspection.rootCorrection;
    if (assessment?.status !== 'cleanup_required' || assessment.cleanupActions.length === 0) {
      return;
    }

    const request = ++requestId;
    const inspection = state.inspection;
    state = { kind: 'rolling_back', inspection };
    try {
      const rollback = await deps.rollback(
        assessment.gameId,
        assessment.cleanupActions.map((action) => action.componentId),
      );
      if (!isCurrent(request)) {
        return;
      }
      const reinspection = await inspectInline(inspection.selectedRoot);
      if (!isCurrent(request)) {
        return;
      }
      if (rollback.kind === 'failed') {
        const latestInspection =
          reinspection.kind === 'completed' ? reinspection.inspection : inspection;
        state = {
          kind: 'review',
          inspection: latestInspection,
          errorPresentation: deps.presentError(rollback.error),
        };
        return;
      }
      if (reinspection.kind === 'failed') {
        state = {
          kind: 'review',
          inspection,
          errorPresentation: deps.presentError(reinspection.error),
        };
        return;
      }
      if (reinspection.inspection.rootCorrection?.status !== 'ready') {
        state = {
          kind: 'review',
          inspection: reinspection.inspection,
          errorPresentation: null,
        };
        return;
      }
      await submitInspection(reinspection.inspection, confirmation, request, true);
    } catch (error) {
      if (!isCurrent(request)) {
        return;
      }
      state = {
        kind: 'review',
        inspection,
        errorPresentation: deps.presentError(error),
      };
    }
  }

  async function inspectInline(selectedRoot: string): Promise<InlineInspectionOutcome> {
    try {
      return { kind: 'completed', inspection: await deps.inspect(selectedRoot) };
    } catch (error) {
      return { kind: 'failed', error };
    }
  }

  return {
    get state(): AddGameFlowState {
      return state;
    },
    get busy(): boolean {
      return state.kind !== 'idle' && state.kind !== 'review';
    },
    get dialog(): AddGameDialogState | null {
      if (state.kind === 'review' || state.kind === 'rolling_back') {
        return state;
      }
      return state.kind === 'submitting' && state.reviewed ? state : null;
    },
    chooseFolder,
    confirm,
    rollbackAndConfirm,
    close,
  };
}

function selectReviewState(
  deps: AddGameFlowDeps,
  inspection: AddGameInspection,
  outcome: Extract<AddGameSubmitOutcome, { kind: 'busy' | 'failed' }>,
  reinspection: InlineInspectionOutcome | null,
): Extract<AddGameFlowState, { kind: 'review' }> {
  if (reinspection?.kind === 'completed') {
    return { kind: 'review', inspection: reinspection.inspection, errorPresentation: null };
  }
  if (reinspection?.kind === 'failed') {
    return {
      kind: 'review',
      inspection,
      errorPresentation: deps.presentError(reinspection.error),
    };
  }
  if (outcome.kind === 'busy') {
    return {
      kind: 'review',
      inspection,
      errorPresentation: deps.presentCatalogBusyError(),
    };
  }
  return { kind: 'review', inspection, errorPresentation: deps.presentError(outcome.error) };
}
