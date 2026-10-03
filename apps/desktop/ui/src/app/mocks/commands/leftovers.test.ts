import { clearPreviewInvoker, invokePreviewCommand } from '@shared/api-preview';
import type {
  CleanupOutcome,
  LeaveOutcome,
  LeftoverProposal,
  ListRetiredGameLeftoversOutput,
} from '@features/cleanup-game-leftovers';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { registerMockInvoker, resetMockDesktopState } from '../desktop';
import { setRetiredGameLeftoversPreview } from './leftovers';

function proposal(revision = 'revision:1'): LeftoverProposal {
  return {
    gameId: 'retired:a',
    gameName: 'Retired game',
    installPath: 'D:/Games/Retired',
    rootState: 'missing',
    intent: '',
    leaveRevision: revision,
    canClean: true,
    items: [
      {
        itemId: 'engine',
        category: 'engineConfig',
        path: 'C:/Preview/Engine.ini',
        disposition: 'cleanable',
        issue: null,
      },
      {
        itemId: 'custody',
        category: 'privateCustody',
        path: 'D:/Games/Retired/backup.dll',
        disposition: 'blocked',
        issue: { code: 'privateOriginalCustody', detail: null },
      },
    ],
  };
}

describe('retired-game preview transport', () => {
  beforeEach(() => {
    clearPreviewInvoker();
    resetMockDesktopState();
    registerMockInvoker();
  });
  afterEach(() => {
    clearPreviewInvoker();
  });

  it('starts empty and supports partial cleanup with explicit nullable response fields', async () => {
    await expect(invokePreviewCommand('list_retired_game_leftovers')).resolves.toEqual({
      proposals: [],
      issues: [],
    });
    setRetiredGameLeftoversPreview([proposal()]);
    const listed = await invokePreviewCommand<ListRetiredGameLeftoversOutput>(
      'list_retired_game_leftovers',
    );
    const { gameId, intent } = listed.proposals[0];
    const result = await invokePreviewCommand<CleanupOutcome>('clean_retired_game_leftovers', {
      gameId,
      intent,
    });
    expect(result.status).toBe('partial');
    expect(result.issue).toBeNull();
    expect(result.steps.map((step) => step.outcome)).toEqual(['released', 'blocked']);
    expect(result.remainingProposal?.items.map((item) => item.itemId)).toEqual(['custody']);
    expect(result.remainingProposal?.canClean).toBe(false);
    const wire: unknown = JSON.parse(JSON.stringify(result));
    expect(wire).toEqual(expect.objectContaining({ issue: null }));
  });

  it('Leaves unchanged proposals through a validated intent and resurfaces changed leftovers', async () => {
    setRetiredGameLeftoversPreview([proposal()]);
    const listed = await invokePreviewCommand<ListRetiredGameLeftoversOutput>(
      'list_retired_game_leftovers',
    );
    const { gameId, intent } = listed.proposals[0];
    const result = await invokePreviewCommand<LeaveOutcome>('leave_retired_game_leftovers', {
      gameId,
      intent,
    });
    expect(result).toEqual({
      gameId,
      status: 'left',
      leaveRevision: 'revision:1',
      remainingProposal: null,
      issue: null,
    });
    await expect(invokePreviewCommand('list_retired_game_leftovers')).resolves.toEqual({
      proposals: [],
      issues: [],
    });
    setRetiredGameLeftoversPreview([proposal('revision:2')]);
    const changed = await invokePreviewCommand<ListRetiredGameLeftoversOutput>(
      'list_retired_game_leftovers',
    );
    expect(changed.proposals).toHaveLength(1);
  });

  it('rejects forged or consumed intents without clearing the remaining items', async () => {
    setRetiredGameLeftoversPreview([proposal()]);
    const forged = await invokePreviewCommand<CleanupOutcome>('clean_retired_game_leftovers', {
      gameId: 'retired:a',
      intent: 'forged',
    });
    expect(forged.status).toBe('stale');
    expect(forged.remainingProposal?.items).toHaveLength(2);
    const consumed = forged.remainingProposal;
    if (consumed === null) {
      throw new Error('Expected a refreshed proposal');
    }
    await invokePreviewCommand('clean_retired_game_leftovers', {
      gameId: consumed.gameId,
      intent: consumed.intent,
    });
    const replay = await invokePreviewCommand<CleanupOutcome>('clean_retired_game_leftovers', {
      gameId: consumed.gameId,
      intent: consumed.intent,
    });
    expect(replay.status).toBe('stale');
    expect(replay.remainingProposal?.items).toHaveLength(1);
    await expect(
      invokePreviewCommand('leave_retired_game_leftovers', { gameId: 'retired:a' }),
    ).rejects.toThrow('intent');
  });
});
