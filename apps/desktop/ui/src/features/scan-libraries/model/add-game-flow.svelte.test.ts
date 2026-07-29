import { describe, expect, it, vi } from 'vitest';

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
        describeError: (error) => (error as Error).message,
      }),
    );
    await flow.chooseFolder();

    await flow.rollbackAndConfirm({
      rootChoice: 'selected',
      allowRootCorrection: true,
      chosenExecutable: null,
    });

    expect(rollback).toHaveBeenCalledWith('game:cleanup', ['component:a']);
    expect(flow.state).toMatchObject({
      kind: 'review',
      errorMessage: 'rollback failed',
    });
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
        describeError: (error) => (error as Error).message,
      }),
    );
    await flow.chooseFolder();

    await flow.confirm({
      rootChoice: 'selected',
      allowRootCorrection: false,
      chosenExecutable: null,
    });

    expect(flow.state).toMatchObject({
      kind: 'review',
      errorMessage: 'submit rejected',
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
        describeError: (error) => (error as Error).message,
      }),
    );
    await flow.chooseFolder();

    await flow.rollbackAndConfirm({
      rootChoice: 'selected',
      allowRootCorrection: true,
      chosenExecutable: null,
    });

    expect(flow.state).toMatchObject({
      kind: 'review',
      errorMessage: 'rollback rejected',
    });
  });
});

function deps(overrides: Partial<AddGameFlowDeps> = {}): AddGameFlowDeps {
  return {
    chooseFolder: () => Promise.resolve('D:/Games/Example'),
    inspect: (root) => Promise.resolve(inspection(root)),
    submit: () => Promise.resolve({ kind: 'busy' }),
    rollback: () => Promise.resolve({ kind: 'completed' }),
    describeError: String,
    publishError: vi.fn(),
    requiresReinspection: () => false,
    catalogBusyMessage: () => 'catalog busy',
    ...overrides,
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
