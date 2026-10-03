import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@shared/api', () => ({ invokeDesktop: invoke }));
import {
  cleanRetiredGameLeftovers,
  leaveRetiredGameLeftovers,
  listRetiredGameLeftovers,
} from './desktop';

describe('retired-game cleanup transport', () => {
  beforeEach(() => invoke.mockReset());

  it('lists without a payload and sends only the issued game identity and intent', async () => {
    await listRetiredGameLeftovers();
    expect(invoke).toHaveBeenCalledWith('list_retired_game_leftovers');
    const input = {
      gameId: 'game:a',
      intent: 'intent:a',
      path: 'C:/untrusted',
      leaveRevision: 'forged',
    };
    await cleanRetiredGameLeftovers(input);
    expect(invoke).toHaveBeenLastCalledWith('clean_retired_game_leftovers', {
      gameId: 'game:a',
      intent: 'intent:a',
    });
    await leaveRetiredGameLeftovers(input);
    expect(invoke).toHaveBeenLastCalledWith('leave_retired_game_leftovers', {
      gameId: 'game:a',
      intent: 'intent:a',
    });
  });

  it('rejects a blank identity or intent before invoking desktop', () => {
    expect(() => cleanRetiredGameLeftovers({ gameId: '', intent: 'intent:a' })).toThrow();
    expect(() => leaveRetiredGameLeftovers({ gameId: 'game:a', intent: ' ' })).toThrow();
    expect(invoke).not.toHaveBeenCalled();
  });
});
