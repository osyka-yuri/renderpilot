import { describe, expect, it, vi } from 'vitest';
import type { PresentedError } from '@shared/error-presentation';

import type { AddGameInspection } from './add-game';
import {
  createAddGameFlow,
  type AddGameFlowDeps,
  type AddGameSubmitOutcome,
} from './add-game-flow.svelte';

describe('add-game flow', () => {
  it('drops an inspection response after close and can be opened again', async () => {
    const firstInspection = deferred<AddGameInspection>();
    const inspect = vi
      .fn<(root: string) => Promise<AddGameInspection>>()
      .mockImplementationOnce(() => firstInspection.promise)
      .mockResolvedValueOnce(inspection('D:/Games/Second'));
    const roots = ['D:/Games/First', 'D:/Games/Second'];
    const flow = createAddGameFlow(
      deps({ chooseFolder: () => Promise.resolve(roots.shift() ?? null), inspect }),
    );

    const firstRun = flow.chooseFolder();
    await Promise.resolve();
    expect(flow.state.kind).toBe('inspecting');
    flow.close();
    firstInspection.resolve(inspection('D:/Games/First'));
    await firstRun;
    expect(flow.state).toEqual({ kind: 'idle' });

    await flow.chooseFolder();
    expect(flow.state.kind).toBe('review');
    expect(flow.dialog?.inspection.selectedRoot).toBe('D:/Games/Second');
  });

  it('ignores a stale submit response after the dialog is closed', async () => {
    const submission = deferred<AddGameSubmitOutcome>();
    const flow = createAddGameFlow(
      deps({
        submit: vi.fn(() => submission.promise),
      }),
    );
    await flow.chooseFolder();

    const confirmation = {
      rootChoice: 'selected' as const,
      allowRootCorrection: false,
      chosenExecutable: null,
    };
    const pending = flow.confirm(confirmation);
    expect(flow.state.kind).toBe('submitting');
    flow.close();
    submission.resolve({ kind: 'busy' });
    await pending;

    expect(flow.state).toEqual({ kind: 'idle' });
  });

  it('returns to review with a precise error when rollback fails', async () => {
    const failure = new Error('rollback failed');
    const inspected = inspection('D:/Games/Needs Cleanup', {
      rootCorrection: {
        gameId: 'game:cleanup',
        status: 'cleanup_required',
        cleanupActions: [{ kind: 'rollback_component', componentId: 'component:a' }],
        blockers: [],
      },
      decision: {
        kind: 'review',
        defaultOption: { rootChoice: 'selected', catalogAction: 'correct_existing_root' },
        options: [{ rootChoice: 'selected', catalogAction: 'correct_existing_root' }],
      },
    });
    const rollback = vi.fn(() => Promise.resolve({ kind: 'failed' as const, error: failure }));
    const flow = createAddGameFlow(
      deps({
        inspect: vi.fn(() => Promise.resolve(inspected)),
        rollback,
        presentError: (error) => presented((error as Error).message),
      }),
    );
    await flow.chooseFolder();

    await flow.rollbackAndConfirm({
      rootChoice: 'selected',
      allowRootCorrection: true,
      chosenExecutable: null,
    });

    expect(rollback).toHaveBeenCalledWith('game:cleanup', ['component:a']);
    expect(flow.state.kind).toBe('review');
    expect(flow.state.kind === 'review' ? flow.state.errorPresentation?.message : null).toBe(
      'rollback failed',
    );
  });

  it('returns to idle and publishes a rejected folder-picker error', async () => {
    const failure = new Error('picker unavailable');
    const publishError = vi.fn();
    const flow = createAddGameFlow(
      deps({
        chooseFolder: () => Promise.reject(failure),
        publishError,
      }),
    );

    await flow.chooseFolder();

    expect(flow.state).toEqual({ kind: 'idle' });
    expect(publishError).toHaveBeenCalledWith(failure);
  });

  it('returns a rejected reviewed submission to review', async () => {
    const failure = new Error('submit rejected');
    const flow = createAddGameFlow(
      deps({
        submit: () => Promise.reject(failure),
        presentError: (error) => presented((error as Error).message),
      }),
    );
    await flow.chooseFolder();

    await flow.confirm({
      rootChoice: 'selected',
      allowRootCorrection: false,
      chosenExecutable: null,
    });

    expect(flow.state.kind).toBe('review');
    expect(flow.state.kind === 'review' ? flow.state.errorPresentation?.message : null).toBe(
      'submit rejected',
    );
  });

  it('returns a reviewed busy submission to review with the catalog warning', async () => {
    const presentCatalogBusyError = vi.fn(() => presented('catalog busy', 'warning'));
    const flow = createAddGameFlow(
      deps({
        submit: () => Promise.resolve({ kind: 'busy' }),
        presentCatalogBusyError,
      }),
    );
    await flow.chooseFolder();

    await flow.confirm(confirmation());

    expect(presentCatalogBusyError).toHaveBeenCalledOnce();
    expect(flow.state.kind).toBe('review');
    expect(flow.state.kind === 'review' ? flow.state.errorPresentation : null).toEqual(
      presented('catalog busy', 'warning'),
    );
  });

  it('preserves a failed outcome with a null error instead of treating it as busy', async () => {
    const presentError = vi.fn((error: unknown) =>
      presented(error === null ? 'null error' : 'error'),
    );
    const presentCatalogBusyError = vi.fn(() => presented('catalog busy', 'warning'));
    const flow = createAddGameFlow(
      deps({
        submit: () => Promise.resolve({ kind: 'failed', error: null }),
        presentError,
        presentCatalogBusyError,
      }),
    );
    await flow.chooseFolder();

    await flow.confirm(confirmation());

    expect(presentError).toHaveBeenCalledWith(null);
    expect(presentCatalogBusyError).not.toHaveBeenCalled();
    expect(flow.state.kind === 'review' ? flow.state.errorPresentation?.message : null).toBe(
      'null error',
    );
  });

  it('uses a completed reinspection after a stale reviewed submission', async () => {
    const initial = inspection('D:/Games/Stale');
    const refreshed = inspection('D:/Games/Stale', { catalogGeneration: 2 });
    const inspect = vi
      .fn<(root: string) => Promise<AddGameInspection>>()
      .mockResolvedValueOnce(initial)
      .mockResolvedValueOnce(refreshed);
    const flow = createAddGameFlow(
      deps({
        inspect,
        submit: () => Promise.resolve({ kind: 'failed', error: new Error('stale') }),
        requiresReinspection: () => true,
      }),
    );
    await flow.chooseFolder();

    await flow.confirm(confirmation());

    expect(inspect).toHaveBeenCalledTimes(2);
    expect(inspect).toHaveBeenLastCalledWith('D:/Games/Stale');
    expect(flow.state).toEqual({
      kind: 'review',
      inspection: refreshed,
      errorPresentation: null,
    });
  });

  it('keeps the prior inspection and presents a failed reinspection', async () => {
    const initial = inspection('D:/Games/Stale');
    const reinspectionError = new Error('reinspection failed');
    const inspect = vi
      .fn<(root: string) => Promise<AddGameInspection>>()
      .mockResolvedValueOnce(initial)
      .mockRejectedValueOnce(reinspectionError);
    const presentError = vi.fn((error: unknown) => presented((error as Error).message));
    const flow = createAddGameFlow(
      deps({
        inspect,
        submit: () => Promise.resolve({ kind: 'failed', error: new Error('stale') }),
        presentError,
        requiresReinspection: () => true,
      }),
    );
    await flow.chooseFolder();

    await flow.confirm(confirmation());

    expect(presentError).toHaveBeenCalledOnce();
    expect(presentError).toHaveBeenCalledWith(reinspectionError);
    expect(flow.state).toEqual({
      kind: 'review',
      inspection: initial,
      errorPresentation: presented('reinspection failed'),
    });
  });

  it('returns a rejected rollback to review', async () => {
    const failure = new Error('rollback rejected');
    const inspected = inspection('D:/Games/Needs Cleanup', {
      rootCorrection: {
        gameId: 'game:cleanup',
        status: 'cleanup_required',
        cleanupActions: [{ kind: 'rollback_component', componentId: 'component:a' }],
        blockers: [],
      },
      decision: {
        kind: 'review',
        defaultOption: { rootChoice: 'selected', catalogAction: 'correct_existing_root' },
        options: [{ rootChoice: 'selected', catalogAction: 'correct_existing_root' }],
      },
    });
    const flow = createAddGameFlow(
      deps({
        inspect: () => Promise.resolve(inspected),
        rollback: () => Promise.reject(failure),
        presentError: (error) => presented((error as Error).message),
      }),
    );
    await flow.chooseFolder();

    await flow.rollbackAndConfirm({
      rootChoice: 'selected',
      allowRootCorrection: true,
      chosenExecutable: null,
    });

    expect(flow.state.kind).toBe('review');
    expect(flow.state.kind === 'review' ? flow.state.errorPresentation?.message : null).toBe(
      'rollback rejected',
    );
  });
});

function deps(overrides: Partial<AddGameFlowDeps> = {}): AddGameFlowDeps {
  return {
    chooseFolder: () => Promise.resolve('D:/Games/Example'),
    inspect: (root) => Promise.resolve(inspection(root)),
    submit: () => Promise.resolve({ kind: 'busy' }),
    rollback: () => Promise.resolve({ kind: 'completed' }),
    presentError: (error) => presented(String(error)),
    presentCatalogBusyError: () => presented('catalog busy', 'warning'),
    publishError: vi.fn(),
    requiresReinspection: () => false,
    ...overrides,
  };
}

function presented(message: string, severity: 'error' | 'warning' = 'error'): PresentedError {
  return {
    code: 'test_error',
    severity,
    message,
    suggestedActions: [],
    contractStatus: 'known',
  };
}

function confirmation() {
  return {
    rootChoice: 'selected' as const,
    allowRootCorrection: false,
    chosenExecutable: null,
  };
}

function inspection(
  selectedRoot: string,
  overrides: Partial<AddGameInspection> = {},
): AddGameInspection {
  return {
    selectedRoot,
    inspectionFingerprint: `inspection:v1:${selectedRoot}`,
    catalogGeneration: 1,
    boundary: {
      kind: 'single_install',
      completeness: 'complete',
      candidateRoots: [selectedRoot],
      evidence: ['root_executable'],
    },
    recommendation: null,
    relationship: { kind: 'new', gameIds: [], provenInstallRoots: [] },
    executables: [],
    requiresExplicitExecutable: false,
    rootCorrection: null,
    decision: {
      kind: 'review',
      defaultOption: { rootChoice: 'selected', catalogAction: 'add' },
      options: [{ rootChoice: 'selected', catalogAction: 'add' }],
    },
    warnings: [],
    ...overrides,
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}
