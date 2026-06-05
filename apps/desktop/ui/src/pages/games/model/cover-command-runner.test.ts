import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clearGameCover, fetchGameCover, setGameCover } from '@entities/game';
import {
  publishCoverDownloadedNotification,
  publishCoverOperationErrorNotification,
  publishCoverRemovedNotification,
  publishCoverUpdatedNotification,
  withManualCoverBusy,
} from '@features/cover-ops';
import { selectCoverFilePath } from './games-page-cover-ops';
import { createCoverCommandRunner } from './cover-command-runner';

vi.mock('@entities/game', () => ({
  clearGameCover: vi.fn(),
  fetchGameCover: vi.fn(),
  setGameCover: vi.fn(),
}));

vi.mock('@features/cover-ops', () => ({
  withManualCoverBusy: vi.fn(),
  publishCoverDownloadedNotification: vi.fn(),
  publishCoverOperationErrorNotification: vi.fn(),
  publishCoverRemovedNotification: vi.fn(),
  publishCoverUpdatedNotification: vi.fn(),
}));

vi.mock('@shared/api', () => ({
  describeCommandError: vi.fn((error: unknown) => String(error)),
}));

vi.mock('./games-page-cover-ops', () => ({
  focusMenuTrigger: vi.fn(),
  selectCoverFilePath: vi.fn(),
}));

describe('createCoverCommandRunner', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    vi.mocked(withManualCoverBusy).mockImplementation(async (params) => {
      await params.task();
      params.onSuccess?.();
    });
  });

  it('publishes a success notification after setting a custom cover', async () => {
    vi.mocked(selectCoverFilePath).mockResolvedValue('C:/covers/game-1.png');
    vi.mocked(setGameCover).mockResolvedValue({
      file_name: 'game-1.png',
      updated_at_ms: 1,
    });

    const deps = createDeps();
    const runner = createCoverCommandRunner(deps);

    runner.pickCover('game-1');

    await vi.waitFor(() => {
      expect(setGameCover).toHaveBeenCalledWith('game-1', 'C:/covers/game-1.png');
      expect(publishCoverUpdatedNotification).toHaveBeenCalledTimes(1);
      expect(deps.setMenuOpenFor).toHaveBeenCalledWith(null);
    });
  });

  it('publishes an info notification after downloading a cover', async () => {
    vi.mocked(fetchGameCover).mockResolvedValue({
      file_name: 'game-1.png',
      updated_at_ms: 1,
    });

    const runner = createCoverCommandRunner(createDeps());
    runner.fetchCover('game-1');

    await vi.waitFor(() => {
      expect(fetchGameCover).toHaveBeenCalledWith('game-1');
      expect(publishCoverDownloadedNotification).toHaveBeenCalledTimes(1);
    });
  });

  it('publishes a success notification after clearing a cover', async () => {
    vi.mocked(clearGameCover).mockResolvedValue({ cleared: true });

    const runner = createCoverCommandRunner(createDeps());
    runner.clearCover('game-1');

    await vi.waitFor(() => {
      expect(clearGameCover).toHaveBeenCalledWith('game-1');
      expect(publishCoverRemovedNotification).toHaveBeenCalledTimes(1);
    });
  });

  it('routes command failures through the semantic cover error producer', async () => {
    vi.mocked(withManualCoverBusy).mockImplementation((params) => {
      params.onCoverError('reload failed');
      return Promise.resolve();
    });

    const runner = createCoverCommandRunner(createDeps());
    runner.fetchCover('game-1');

    await vi.waitFor(() => {
      expect(publishCoverOperationErrorNotification).toHaveBeenCalledWith('reload failed');
    });
  });
});

function createDeps() {
  return {
    getManualCoverBusyFor: () => null,
    setManualCoverBusyFor: vi.fn(),
    getActionMenuRefs: () => ({}),
    getMenuOpenFor: () => null,
    setMenuOpenFor: vi.fn(),
    getOnClearError: () => vi.fn(),
    getOnReloadCards: () => vi.fn(() => Promise.resolve()),
  };
}
