/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import {
  createNvapiProfile,
  deleteNvapiProfile,
  getNvapiProfileStatus,
  type NvapiProfileStatus,
} from '@features/nvapi-settings';
import type { NvapiProfileContext } from '../model/create-nvapi-profile-context.svelte';
import { createNvapiProfileContext } from '../model/create-nvapi-profile-context.svelte';
import NvidiaProfileControl from './NvidiaProfileControl.svelte';
import NvidiaProfileControlTestHost from './NvidiaProfileControl.test-host.svelte';

vi.mock('@features/nvapi-settings', () => ({
  createNvapiProfile: vi.fn(),
  deleteNvapiProfile: vi.fn(),
  getNvapiProfileStatus: vi.fn(),
  moveNvapiProfile: vi.fn(),
}));

vi.mock('@shared/error-presentation', () => ({
  formatPresentedError: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
}));

vi.mock('@shared/errors', () => ({
  reportClientError: vi.fn(),
}));

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
  publishWarningNotification: vi.fn(),
}));

const GAME_ID = 'manual:test-game';
const EXECUTABLE = 'C:/Games/Test/game.exe';

let component: ReturnType<typeof mount> | null = null;
let target: HTMLDivElement;

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe = vi.fn();
      unobserve = vi.fn();
      disconnect = vi.fn();
    },
  );
  target = document.createElement('div');
  document.body.append(target);
});

afterEach(async () => {
  if (component) {
    await unmount(component);
    component = null;
  }
  vi.unstubAllGlobals();
  document.body.replaceChildren();
});

function profileStatus(
  state: NvapiProfileStatus['state'],
  overrides: Partial<NvapiProfileStatus> = {},
): NvapiProfileStatus {
  return {
    selectedExecutable: EXECUTABLE,
    bindingPath: EXECUTABLE,
    profileName: 'RenderPilot - Test',
    state,
    isPredefined: false,
    ownedByThisGame: state === 'owned',
    canCreate: state === 'missing',
    canDelete: state === 'owned',
    pendingOperation: null,
    pendingOperationGameId: null,
    detail: null,
    ...overrides,
  };
}

function staticProfile(
  status: NvapiProfileStatus | null,
  overrides: Partial<NvapiProfileContext> = {},
): NvapiProfileContext {
  return {
    status,
    loading: false,
    busy: false,
    loadError: null,
    actionError: null,
    refreshError: null,
    reload: vi.fn().mockResolvedValue(undefined),
    create: vi.fn().mockResolvedValue(true),
    remove: vi.fn().mockResolvedValue(true),
    move: vi.fn().mockResolvedValue(true),
    moveTo: vi.fn().mockResolvedValue(true),
    clear: vi.fn(),
    ownedBindingPath: null,
    ...overrides,
  };
}

function render(
  profile: NvapiProfileContext,
  mode: 'settings' | 'recovery' = 'settings',
  onOpenGameDetails = vi.fn(),
  onRecoveryDeleteComplete = vi.fn(),
): HTMLDivElement {
  component = mount(NvidiaProfileControl, {
    target,
    props: {
      gameId: GAME_ID,
      mode,
      profile,
      onOpenGameDetails,
      onRecoveryDeleteComplete,
      canCreate: true,
    },
  });
  flushSync();
  return target;
}

function renderWithReactiveGameId(
  profile: NvapiProfileContext,
  gameId: string,
  mode: 'settings' | 'recovery' = 'settings',
  onRecoveryDeleteComplete = vi.fn(),
): HTMLDivElement {
  component = mount(NvidiaProfileControlTestHost, {
    target,
    props: {
      gameId,
      mode,
      profile,
      onOpenGameDetails: vi.fn(),
      onRecoveryDeleteComplete,
      canCreate: true,
    },
  });
  flushSync();
  return target;
}

function setRenderedGameId(gameId: string): void {
  (component as { setGameId: (gameId: string) => void }).setGameId(gameId);
  flushSync();
}

function controlRow(): HTMLDivElement | null {
  return target.querySelector<HTMLDivElement>('[role="group"][aria-label="NVIDIA profile"]');
}

function buttonContaining(text: string, root: ParentNode = target): HTMLButtonElement | undefined {
  return [...root.querySelectorAll<HTMLButtonElement>('button')].find((button) =>
    button.textContent.includes(text),
  );
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe('NvidiaProfileControl', () => {
  it.each(['predefined', 'external'] as const)(
    'keeps the status row for a %s profile without management actions',
    (state) => {
      render(
        staticProfile(
          profileStatus(state, {
            profileName: `Driver ${state} profile`,
            ownedByThisGame: true,
            canDelete: true,
          }),
        ),
        'settings',
      );
      expect(controlRow()?.textContent).toContain(`Driver ${state} profile`);
      expect(controlRow()?.textContent).toContain('NVIDIA profile');
      expect(controlRow()?.querySelector('[title]')).toBeNull();
      expect(buttonContaining('Delete profile')).toBeUndefined();
      expect(buttonContaining('Create profile')).toBeUndefined();
    },
  );

  it('keeps the settings row mounted across missing, owned, predefined, and external states', async () => {
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('missing'))
      .mockResolvedValueOnce(profileStatus('owned', { profileName: 'RenderPilot - Owned' }))
      .mockResolvedValueOnce(
        profileStatus('predefined', {
          profileName: 'Built-in NVIDIA profile',
          ownedByThisGame: false,
          canDelete: false,
        }),
      )
      .mockResolvedValueOnce(
        profileStatus('external', {
          profileName: 'Other NVIDIA profile',
          ownedByThisGame: false,
          canDelete: false,
        }),
      );
    const profile = createNvapiProfileContext();
    await profile.reload(GAME_ID);
    render(profile);
    const row = controlRow();

    expect(row?.textContent).toContain('No NVIDIA profile.');
    expect(buttonContaining('Create profile')).toBeDefined();

    await profile.reload(GAME_ID);
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.textContent).toContain('RenderPilot - Owned');
    expect(row?.textContent).toContain('NVIDIA profile');
    expect(row?.querySelector('[title]')).toBeNull();
    expect(buttonContaining('Delete profile')?.dataset.variant).toBe('destructive');

    await profile.reload(GAME_ID);
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.textContent).toContain('Built-in NVIDIA profile');
    expect(buttonContaining('Delete profile')).toBeUndefined();
    expect(buttonContaining('Create profile')).toBeUndefined();

    await profile.reload(GAME_ID);
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.textContent).toContain('Other NVIDIA profile');
    expect(buttonContaining('Delete profile')).toBeUndefined();
    expect(buttonContaining('Create profile')).toBeUndefined();
  });

  it('renders a checking row before the first profile lookup completes', async () => {
    const firstStatus = deferred<NvapiProfileStatus>();
    vi.mocked(getNvapiProfileStatus).mockReturnValueOnce(firstStatus.promise);
    const profile = createNvapiProfileContext();
    const reload = profile.reload(GAME_ID);
    render(profile);

    const row = controlRow();
    expect(row?.textContent).toContain('Checking NVIDIA profile…');
    expect(row?.getAttribute('aria-busy')).toBe('true');
    expect(buttonContaining('Create profile')).toBeUndefined();

    firstStatus.resolve(
      profileStatus('predefined', {
        profileName: 'Built-in NVIDIA profile',
        ownedByThisGame: false,
        canDelete: false,
      }),
    );
    await reload;
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.textContent).toContain('Built-in NVIDIA profile');
  });

  it('shows checking and hides stale actions while reloading an executable profile', async () => {
    const nextStatus = deferred<NvapiProfileStatus>();
    const finalStatus = deferred<NvapiProfileStatus>();
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('owned', { profileName: 'Old executable profile' }))
      .mockReturnValueOnce(nextStatus.promise)
      .mockReturnValueOnce(finalStatus.promise);
    const profile = createNvapiProfileContext();
    await profile.reload(GAME_ID);
    render(profile);
    const row = controlRow();

    const reload = profile.reload(GAME_ID);
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.textContent).toContain('Checking NVIDIA profile…');
    expect(row?.textContent).not.toContain('Old executable profile');
    expect(buttonContaining('Delete profile')).toBeUndefined();
    expect(buttonContaining('Create profile')).toBeUndefined();

    nextStatus.resolve(
      profileStatus('missing', {
        selectedExecutable: 'C:/Games/Test/alternate.exe',
        bindingPath: 'C:/Games/Test/alternate.exe',
      }),
    );
    await reload;
    flushSync();
    expect(controlRow()).toBe(row);
    expect(buttonContaining('Create profile')).toBeDefined();

    const nextReload = profile.reload(GAME_ID);
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.textContent).toContain('Checking NVIDIA profile…');
    expect(buttonContaining('Create profile')).toBeUndefined();

    finalStatus.resolve(
      profileStatus('external', {
        selectedExecutable: 'C:/Games/Test/third.exe',
        bindingPath: 'C:/Games/Test/third.exe',
        profileName: 'External executable profile',
        ownedByThisGame: false,
        canDelete: false,
      }),
    );
    await nextReload;
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.textContent).toContain('External executable profile');
    expect(buttonContaining('Create profile')).toBeUndefined();
  });

  it('keeps the same fixed-height row while creating and after the owned profile appears', async () => {
    const create = deferred<undefined>();
    const refresh = deferred<undefined>();
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('missing'))
      .mockResolvedValueOnce(profileStatus('owned'));
    vi.mocked(createNvapiProfile).mockReturnValueOnce(create.promise);

    const profile = createNvapiProfileContext(() => {
      return refresh.promise;
    });
    await profile.reload(GAME_ID);
    render(profile);

    const row = controlRow();
    expect(row?.classList.contains('h-16')).toBe(true);
    expect(row?.textContent).toContain('No NVIDIA profile.');

    buttonContaining('Create profile')?.click();
    flushSync();
    expect(controlRow()).toBe(row);
    expect(row?.classList.contains('h-16')).toBe(true);
    expect(row?.getAttribute('aria-busy')).toBe('true');
    expect(row?.textContent).toContain('Creating profile…');

    create.resolve(undefined);
    await vi.waitFor(() => {
      expect(row?.textContent).toContain('RenderPilot - Test');
      expect(buttonContaining('Creating profile…')).toBeDefined();
    });
    expect(controlRow()).toBe(row);
    expect(row?.classList.contains('h-16')).toBe(true);
    expect(target.textContent).not.toContain('This profile cannot be removed right now.');

    refresh.resolve(undefined);
    await vi.waitFor(() => {
      expect(buttonContaining('Delete profile')).toBeDefined();
    });
    expect(controlRow()).toBe(row);
  });

  it('keeps the same fixed-height row while deleting and after the create action returns', async () => {
    const remove = deferred<undefined>();
    const refresh = deferred<undefined>();
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('owned'))
      .mockResolvedValueOnce(profileStatus('missing'));
    vi.mocked(deleteNvapiProfile).mockReturnValueOnce(remove.promise);

    const profile = createNvapiProfileContext(() => {
      return refresh.promise;
    });
    await profile.reload(GAME_ID);
    render(profile);

    const row = controlRow();
    expect(row?.classList.contains('h-16')).toBe(true);
    buttonContaining('Delete profile')?.click();
    flushSync();

    const dialog = await vi.waitFor(() => {
      const content = document.body.querySelector<HTMLElement>('[data-slot="dialog-content"]');
      expect(content?.textContent).toContain('Delete RenderPilot - Test?');
      if (!content) {
        throw new Error('Expected delete confirmation');
      }
      return content;
    });
    expect(dialog.textContent).toContain('driver profile will be removed');
    expect(dialog.textContent).not.toContain(EXECUTABLE);

    buttonContaining('Delete profile', dialog)?.click();
    flushSync();
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(row);
    });
    expect(controlRow()).toBe(row);
    expect(row?.classList.contains('h-16')).toBe(true);
    expect(row?.getAttribute('aria-busy')).toBe('true');
    expect(row?.textContent).toContain('Deleting profile…');
    expect(target.textContent).not.toContain('This profile cannot be removed right now.');

    remove.resolve(undefined);
    await vi.waitFor(() => {
      expect(row?.textContent).toContain('No NVIDIA profile.');
      expect(buttonContaining('Deleting profile…')).toBeDefined();
    });
    expect(controlRow()).toBe(row);
    expect(row?.classList.contains('h-16')).toBe(true);

    refresh.resolve(undefined);
    await vi.waitFor(() => {
      expect(buttonContaining('Create profile')).toBeDefined();
    });
    expect(controlRow()).toBe(row);
  });

  it('does not return focus to the newly selected game when delete closes after a switch', async () => {
    const remove = deferred<undefined>();
    vi.mocked(deleteNvapiProfile).mockReturnValueOnce(remove.promise);
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('owned'))
      .mockResolvedValueOnce(
        profileStatus('owned', {
          profileName: 'Other game profile',
          selectedExecutable: 'C:/Games/Other/game.exe',
          bindingPath: 'C:/Games/Other/game.exe',
        }),
      );
    const profile = createNvapiProfileContext();
    await profile.reload(GAME_ID);
    renderWithReactiveGameId(profile, GAME_ID);

    const row = controlRow();
    const deleteTrigger = buttonContaining('Delete profile');
    deleteTrigger?.click();
    const dialog = await vi.waitFor(() => {
      const content = document.body.querySelector<HTMLElement>('[data-slot="dialog-content"]');
      expect(content?.textContent).toContain('Delete RenderPilot - Test?');
      if (!content) {
        throw new Error('Expected delete confirmation');
      }
      return content;
    });
    buttonContaining('Delete profile', dialog)?.click();

    setRenderedGameId('manual:other-game');
    await profile.reload('manual:other-game');
    flushSync();
    expect(controlRow()?.textContent).toContain('Other game profile');
    expect(buttonContaining('Delete profile')?.disabled).toBe(true);

    await vi.waitFor(() => {
      expect(dialog.getAttribute('data-state')).toBe('closed');
    });
    expect(document.activeElement).not.toBe(row);
    expect(document.activeElement).not.toBe(deleteTrigger);

    remove.resolve(undefined);
    await vi.waitFor(() => {
      expect(profile.busy).toBe(false);
    });
  });

  it('offers no create action in recovery-only mode when the profile is missing', () => {
    render(staticProfile(profileStatus('missing')), 'recovery');
    expect(controlRow()).toBeNull();
    expect(buttonContaining('Create profile')).toBeUndefined();
  });

  it('keeps an owned profile removable in recovery-only mode', () => {
    render(staticProfile(profileStatus('owned')), 'recovery');
    expect(controlRow()?.textContent).toContain('RenderPilot - Test');
    expect(buttonContaining('Delete profile')).toBeDefined();
    expect(buttonContaining('Create profile')).toBeUndefined();
  });

  it('shows the delete action and a reason when an owned profile is blocked', () => {
    render(
      staticProfile(profileStatus('conflict', { ownedByThisGame: true, canDelete: false })),
      'recovery',
    );
    const button = buttonContaining('Delete profile');
    expect(button?.disabled).toBe(true);
    expect(button?.getAttribute('aria-disabled')).toBeNull();
    expect(target.textContent).toContain('This profile cannot be removed right now.');
  });

  it('announces a status error while using the detailed load error as the only alert', () => {
    render(staticProfile(null, { loadError: 'The NVIDIA profile could not be inspected.' }));

    expect(controlRow()?.querySelector('[role="alert"]')).toBeNull();
    expect(controlRow()?.querySelector('[role="status"]')).toBeNull();
    expect(target.querySelectorAll('[role="alert"]')).toHaveLength(1);
    expect(target.querySelector('[role="alert"]')?.textContent).toContain(
      'The NVIDIA profile could not be inspected.',
    );
  });

  it('announces a profile status error when there is no detailed load error', () => {
    render(staticProfile(profileStatus('error')));

    expect(controlRow()?.querySelector('[role="alert"]')?.textContent).toContain(
      'The NVIDIA profile could not be inspected.',
    );
    expect(target.querySelectorAll('[role="alert"]')).toHaveLength(1);
  });

  it('shows the selected game status and disables its action while an old create is in flight', async () => {
    const create = deferred<undefined>();
    vi.mocked(createNvapiProfile).mockReturnValueOnce(create.promise);
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('missing'))
      .mockResolvedValueOnce(
        profileStatus('owned', {
          profileName: 'New game profile',
          selectedExecutable: 'C:/Games/Other/game.exe',
          bindingPath: 'C:/Games/Other/game.exe',
        }),
      );
    const profile = createNvapiProfileContext();
    await profile.reload(GAME_ID);
    renderWithReactiveGameId(profile, GAME_ID);

    buttonContaining('Create profile')?.click();
    flushSync();
    expect(buttonContaining('Creating profile…')).toBeDefined();

    setRenderedGameId('manual:other-game');
    const reloadOtherGame = profile.reload('manual:other-game');
    await reloadOtherGame;
    flushSync();

    expect(controlRow()?.textContent).toContain('New game profile');
    expect(controlRow()?.getAttribute('aria-busy')).toBe('false');
    expect(buttonContaining('Delete profile')?.disabled).toBe(true);
    expect(controlRow()?.querySelector('.animate-spin')).toBeNull();
    expect(target.textContent).not.toContain('Creating profile…');
    expect(buttonContaining('Creating profile…')).toBeUndefined();

    create.resolve(undefined);
    await vi.waitFor(() => {
      expect(profile.busy).toBe(false);
      expect(controlRow()?.textContent).toContain('New game profile');
      expect(buttonContaining('Delete profile')?.disabled).toBe(false);
    });
    expect(buttonContaining('Creating profile…')).toBeUndefined();
  });

  it('does not let an old retry clear the retry started for the newly selected game', async () => {
    const oldRetryStatus = deferred<NvapiProfileStatus>();
    const newRetryStatus = deferred<NvapiProfileStatus>();
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('error'))
      .mockReturnValueOnce(oldRetryStatus.promise)
      .mockResolvedValueOnce(profileStatus('error'))
      .mockReturnValueOnce(newRetryStatus.promise);
    const profile = createNvapiProfileContext();
    await profile.reload(GAME_ID);
    renderWithReactiveGameId(profile, GAME_ID);

    buttonContaining('Retry')?.click();
    flushSync();
    expect(buttonContaining('Checking NVIDIA profile…')).toBeDefined();

    setRenderedGameId('manual:other-game');
    await profile.reload('manual:other-game');
    flushSync();
    expect(buttonContaining('Retry')).toBeDefined();
    expect(controlRow()?.textContent).not.toContain('Checking NVIDIA profile…');

    buttonContaining('Retry')?.click();
    flushSync();
    expect(buttonContaining('Checking NVIDIA profile…')).toBeDefined();

    oldRetryStatus.resolve(profileStatus('error'));
    await vi.waitFor(() => {
      expect(profile.loading).toBe(true);
      expect(buttonContaining('Checking NVIDIA profile…')).toBeDefined();
    });

    newRetryStatus.resolve(
      profileStatus('owned', {
        profileName: 'New game profile',
        selectedExecutable: 'C:/Games/Other/game.exe',
        bindingPath: 'C:/Games/Other/game.exe',
      }),
    );
    await vi.waitFor(() => {
      expect(controlRow()?.textContent).toContain('New game profile');
    });
    expect(buttonContaining('Checking NVIDIA profile…')).toBeUndefined();
  });

  it('does not finish recovery for the old game when its delete completes after a switch', async () => {
    const remove = deferred<undefined>();
    vi.mocked(deleteNvapiProfile).mockReturnValueOnce(remove.promise);
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('owned'))
      .mockResolvedValueOnce(
        profileStatus('missing', {
          selectedExecutable: 'C:/Games/Other/game.exe',
          bindingPath: 'C:/Games/Other/game.exe',
          profileName: null,
          ownedByThisGame: false,
          canDelete: false,
        }),
      );
    const onRecoveryDeleteComplete = vi.fn();
    const profile = createNvapiProfileContext();
    await profile.reload(GAME_ID);
    renderWithReactiveGameId(profile, GAME_ID, 'recovery', onRecoveryDeleteComplete);

    buttonContaining('Delete profile')?.click();
    const dialog = await vi.waitFor(() => {
      const content = document.body.querySelector<HTMLElement>('[data-slot="dialog-content"]');
      expect(content?.textContent).toContain('Delete RenderPilot - Test?');
      if (!content) {
        throw new Error('Expected delete confirmation');
      }
      return content;
    });
    buttonContaining('Delete profile', dialog)?.click();
    flushSync();

    setRenderedGameId('manual:other-game');
    await profile.reload('manual:other-game');
    flushSync();
    expect(controlRow()).toBeNull();

    remove.resolve(undefined);
    await vi.waitFor(() => {
      expect(profile.busy).toBe(false);
    });
    expect(onRecoveryDeleteComplete).not.toHaveBeenCalled();
  });

  it('shows a same-game recovery action in recovery-only mode', () => {
    render(
      staticProfile(
        profileStatus('pending', {
          pendingOperation: 'setting',
          pendingOperationGameId: GAME_ID,
        }),
      ),
      'recovery',
    );
    expect(buttonContaining('Retry recovery')).toBeDefined();
    expect(buttonContaining('Create profile')).toBeUndefined();
  });

  it('hides another game’s pending operation in recovery-only mode', () => {
    const otherGameStatus = profileStatus('pending', {
      pendingOperation: 'setting',
      pendingOperationGameId: 'manual:other-game',
    });
    render(staticProfile(otherGameStatus), 'recovery');
    expect(controlRow()).toBeNull();
  });

  it('keeps an error retry visible during recovery-only lookup and never offers create', async () => {
    const retry = deferred<NvapiProfileStatus>();
    const profile = createNvapiProfileContext();
    vi.mocked(getNvapiProfileStatus).mockRejectedValueOnce(new Error('Profile lookup failed'));
    await profile.reload(GAME_ID);
    vi.mocked(getNvapiProfileStatus).mockReturnValueOnce(retry.promise);
    render(profile, 'recovery');

    expect(target.textContent).toContain('Profile lookup failed');
    buttonContaining('Retry')?.click();
    flushSync();
    expect(controlRow()).not.toBeNull();
    expect(controlRow()?.textContent).toContain('Checking NVIDIA profile…');
    expect(buttonContaining('Create profile')).toBeUndefined();

    retry.resolve(profileStatus('predefined', { isPredefined: true }));
    await vi.waitFor(() => {
      expect(controlRow()).toBeNull();
    });
  });

  it.each([
    { state: 'error', ownedByThisGame: false, mode: 'recovery' },
    { state: 'error', ownedByThisGame: true, mode: 'recovery' },
    { state: 'error', ownedByThisGame: true, mode: 'settings' },
    { state: 'nvapiUnavailable', ownedByThisGame: true, mode: 'recovery' },
    { state: 'nvapiUnavailable', ownedByThisGame: true, mode: 'settings' },
  ] as const)(
    'offers Retry for owned=$ownedByThisGame $state status in $mode mode',
    ({ state, ownedByThisGame, mode }) => {
      render(
        staticProfile(
          profileStatus(state, {
            ownedByThisGame,
            canDelete: false,
          }),
        ),
        mode,
      );
      expect(buttonContaining('Retry')).toBeDefined();
      expect(buttonContaining('Delete profile')).toBeUndefined();
      expect(buttonContaining('Create profile')).toBeUndefined();
    },
  );

  it('keeps delete blocked for an owned receipt when the profile status is noExecutable', () => {
    render(
      staticProfile(
        profileStatus('noExecutable', {
          ownedByThisGame: true,
          canDelete: true,
        }),
      ),
      'recovery',
    );
    expect(buttonContaining('Delete profile')?.disabled).toBe(true);
    expect(target.textContent).toContain('This profile cannot be removed right now.');
    expect(buttonContaining('Retry')).toBeUndefined();
  });

  it('closes delete confirmation when its executable or profile identity changes', async () => {
    let resolveNext!: (value: NvapiProfileStatus) => void;
    const nextStatus = new Promise<NvapiProfileStatus>((resolve) => {
      resolveNext = resolve;
    });
    vi.mocked(getNvapiProfileStatus)
      .mockResolvedValueOnce(profileStatus('owned'))
      .mockReturnValueOnce(nextStatus)
      .mockResolvedValueOnce(profileStatus('owned'));
    const profile = createNvapiProfileContext();
    await profile.reload(GAME_ID);
    render(profile);

    buttonContaining('Delete profile')?.click();
    flushSync();
    const dialog = await vi.waitFor(() => {
      const content = document.body.querySelector<HTMLElement>('[data-slot="dialog-content"]');
      expect(content?.textContent).toContain('Delete RenderPilot - Test?');
      if (!content) {
        throw new Error('Expected delete confirmation');
      }
      return content;
    });

    const reload = profile.reload(GAME_ID);
    flushSync();
    expect(dialog.getAttribute('data-state')).toBe('closed');
    resolveNext(
      profileStatus('owned', {
        selectedExecutable: 'C:/Games/Test/alternate.exe',
        bindingPath: 'C:/Games/Test/alternate.exe',
        profileName: 'RenderPilot - Alternate',
      }),
    );
    await reload;
    expect(dialog.textContent).toContain('Delete RenderPilot - Test?');
    expect(buttonContaining('Delete profile')).toBeDefined();

    const returnToOriginalStatus = profile.reload(GAME_ID);
    await returnToOriginalStatus;
    flushSync();
    expect(dialog.getAttribute('data-state')).toBe('closed');
  });
});
