import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as ErrorsModule from '@shared/errors';
import { reportClientError } from '@shared/errors';
import type * as NotificationsModule from '@shared/notifications';
import { publishWarningNotification } from '@shared/notifications';
import {
  clearGameExecutableOverride,
  listGameExecutableCandidates,
  resolveGameExecutable,
  setGameExecutableOverride,
} from '@features/nvapi-settings';
import { createGameExecutableContext } from './create-game-executable-context.svelte';

vi.mock('@features/nvapi-settings', () => ({
  clearGameExecutableOverride: vi.fn(),
  listGameExecutableCandidates: vi.fn(),
  resolveGameExecutable: vi.fn(),
  setGameExecutableOverride: vi.fn(),
}));

vi.mock('@shared/errors', async (importOriginal) => ({
  ...(await importOriginal<typeof ErrorsModule>()),
  reportClientError: vi.fn(),
}));

vi.mock('@shared/notifications', async (importOriginal) => ({
  ...(await importOriginal<typeof NotificationsModule>()),
  publishWarningNotification: vi.fn(),
}));

describe('createGameExecutableContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(resolveGameExecutable).mockResolvedValue(null);
    vi.mocked(listGameExecutableCandidates).mockResolvedValue([]);
  });

  it('keeps a localized error after saving an executable selection fails', async () => {
    vi.mocked(setGameExecutableOverride).mockRejectedValueOnce(new Error('write denied'));
    const context = createGameExecutableContext();
    await context.reload('steam:123');

    await expect(context.setOverride('steam:123', 'C:/Games/Test/game.exe')).resolves.toBe(false);

    expect(context.changeError).toBeTruthy();
    expect(context.loadError).toBeNull();
    expect(resolveGameExecutable).toHaveBeenCalledOnce();
  });

  it('keeps a localized error after clearing an executable selection fails', async () => {
    vi.mocked(clearGameExecutableOverride).mockRejectedValueOnce(new Error('write denied'));
    const context = createGameExecutableContext();
    await context.reload('steam:123');

    await expect(context.clearOverride('steam:123')).resolves.toBe(false);

    expect(context.changeError).toBeTruthy();
    expect(context.loadError).toBeNull();
    expect(resolveGameExecutable).toHaveBeenCalledOnce();
  });

  it('returns committed success when a dependent refresh fails after setting an override', async () => {
    vi.mocked(setGameExecutableOverride).mockResolvedValueOnce();
    vi.mocked(resolveGameExecutable).mockResolvedValueOnce(null);
    vi.mocked(listGameExecutableCandidates).mockResolvedValueOnce([]);
    const onChange = vi.fn().mockRejectedValueOnce(new Error('dependent refresh failed'));
    const context = createGameExecutableContext({ onChange });
    await context.reload('steam:123');

    await expect(context.setOverride('steam:123', 'C:/Games/Test/game.exe')).resolves.toBe(true);

    expect(setGameExecutableOverride).toHaveBeenCalledOnce();
    expect(onChange).toHaveBeenCalledWith('steam:123');
    expect(context.refreshError).toBeTruthy();
    expect(context.changeError).toBeNull();
    expect(context.loadError).toBeNull();
    expect(reportClientError).toHaveBeenCalledOnce();
    expect(publishWarningNotification).toHaveBeenCalledOnce();
  });

  it('returns committed success when a dependent refresh fails after clearing an override', async () => {
    vi.mocked(clearGameExecutableOverride).mockResolvedValueOnce();
    vi.mocked(resolveGameExecutable).mockResolvedValueOnce(null);
    vi.mocked(listGameExecutableCandidates).mockResolvedValueOnce([]);
    const onChange = vi.fn().mockRejectedValueOnce(new Error('dependent refresh failed'));
    const context = createGameExecutableContext({ onChange });
    await context.reload('steam:123');

    await expect(context.clearOverride('steam:123')).resolves.toBe(true);

    expect(clearGameExecutableOverride).toHaveBeenCalledOnce();
    expect(onChange).toHaveBeenCalledWith('steam:123');
    expect(context.refreshError).toBeTruthy();
    expect(context.changeError).toBeNull();
    expect(publishWarningNotification).toHaveBeenCalledOnce();
  });

  it('reports reload failures for the selector to present', async () => {
    vi.mocked(resolveGameExecutable).mockRejectedValueOnce(new Error('reload failed'));
    vi.mocked(listGameExecutableCandidates).mockResolvedValueOnce([]);
    const context = createGameExecutableContext();

    await expect(context.reload('steam:123')).resolves.toBe(false);

    expect(context.loadError).toBeTruthy();
  });

  it('keeps selection pending through dependent refresh and blocks overlapping actions', async () => {
    let finishDependentRefresh!: () => void;
    const onChange = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finishDependentRefresh = resolve;
        }),
    );
    const context = createGameExecutableContext({ onChange });
    await context.reload('steam:123');

    const action = context.setOverride('steam:123', 'C:/Games/Test/other.exe');
    expect(context.busy).toBe(true);
    await vi.waitFor(() => {
      expect(onChange).toHaveBeenCalledOnce();
    });
    expect(context.busy).toBe(true);
    await expect(context.clearOverride('steam:123')).resolves.toBe(false);
    expect(clearGameExecutableOverride).not.toHaveBeenCalled();

    finishDependentRefresh();
    await expect(action).resolves.toBe(true);
    expect(context.busy).toBe(false);
  });

  it('does not refresh a previous game after its executable write settles', async () => {
    let finishWrite!: () => void;
    vi.mocked(setGameExecutableOverride).mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishWrite = resolve;
      }),
    );
    const onChange = vi.fn();
    const context = createGameExecutableContext({ onChange });
    await context.reload('steam:123');

    const action = context.setOverride('steam:123', 'C:/Games/Test/other.exe');
    await context.reload('steam:456');
    finishWrite();
    await expect(action).resolves.toBe(true);

    expect(context.effectiveExe).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
    expect(context.busy).toBe(false);
  });
});
