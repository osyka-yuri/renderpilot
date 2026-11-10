import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as NvidiaSettingsApi from '@features/nvapi-settings';
import {
  clearGameExecutableOverride,
  listGameExecutableCandidates,
  listNvapiSettingStates,
  resolveGameExecutable,
  setGameExecutableOverride,
  setNvapiSettingValue,
  type EffectiveExecutable,
  type SettingStateResponse,
} from '@features/nvapi-settings';
import { createExclusiveTaskRunner } from '@shared/concurrency';
import { setLanguageMode } from '@shared/i18n';
import {
  createGameExecutableContext,
  type GameExecutableContext,
} from './create-game-executable-context.svelte';
import { createNvidiaDriverContext } from './create-nvidia-driver-context.svelte';

vi.mock('@features/nvapi-settings', async (importOriginal) => {
  const actual = await importOriginal<typeof NvidiaSettingsApi>();
  return {
    ...actual,
    clearGameExecutableOverride: vi.fn(),
    listGameExecutableCandidates: vi.fn(),
    listNvapiSettingStates: vi.fn(),
    resolveGameExecutable: vi.fn(),
    setGameExecutableOverride: vi.fn(),
    setNvapiSettingValue: vi.fn(),
  };
});

const GAME = 'manual:test-game';
const SETTING_KEY = 'dlss_sr_render_preset';
const EXE_A = 'C:/Games/Test/game.exe';
const EXE_B = 'C:/Games/Test/alternate.exe';

function effectiveExecutable(path: string): EffectiveExecutable {
  return {
    file_name: path.split('/').at(-1) ?? path,
    absolute_path: path,
    auto_absolute_path: EXE_A,
    source: path === EXE_A ? 'auto' : 'override',
  };
}

function setting(exe: string, wire = 'default'): SettingStateResponse {
  return {
    setting_key: SETTING_KEY,
    setting_label: 'DLSS preset',
    value_type: 'dword',
    dll_kind: 'sr',
    family: 'sr',
    category: 'DLSS Super Resolution',
    description: null,
    min_driver: null,
    current: { wire, label: wire, dword: wire === 'default' ? 0 : 1 },
    current_is_explicit: wire !== 'default',
    predefined: null,
    original: null,
    is_current_predefined: wire === 'default',
    effective_exe: exe,
    effective_exe_source: exe === EXE_A ? 'auto' : 'override',
    has_profile_for_exe: true,
    nvapi_available: true,
    catalog_readiness: 'ready',
    available_values: [],
    dll_info: null,
    warnings: [],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function createInterlockedContexts() {
  const mutationRunner = createExclusiveTaskRunner();
  const driver = createNvidiaDriverContext(undefined, () => !executable.busy, mutationRunner);
  const executable: GameExecutableContext = createGameExecutableContext({
    canMutate: () => !driver.busy,
    mutationRunner,
    onChange: async (gameId) => {
      await driver.reload(gameId);
    },
  });
  return { executable, driver };
}

async function loadInitialState(
  executable: ReturnType<typeof createGameExecutableContext>,
  driver: ReturnType<typeof createNvidiaDriverContext>,
): Promise<void> {
  vi.mocked(resolveGameExecutable).mockResolvedValue(effectiveExecutable(EXE_A));
  vi.mocked(listGameExecutableCandidates).mockResolvedValue([]);
  vi.mocked(listNvapiSettingStates).mockResolvedValue([setting(EXE_A)]);
  await executable.reload(GAME);
  await driver.reload(GAME);
}

describe('game executable and NVIDIA mutation interlock', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLanguageMode('en');
    vi.mocked(clearGameExecutableOverride).mockResolvedValue(undefined);
    vi.mocked(listGameExecutableCandidates).mockResolvedValue([]);
    vi.mocked(listNvapiSettingStates).mockResolvedValue([setting(EXE_A)]);
    vi.mocked(resolveGameExecutable).mockResolvedValue(effectiveExecutable(EXE_A));
    vi.mocked(setGameExecutableOverride).mockResolvedValue(undefined);
    vi.mocked(setNvapiSettingValue).mockResolvedValue(setting(EXE_A, 'quality'));
  });

  it('blocks NVAPI writes through executable persistence and dependent reload, then resumes', async () => {
    const { executable, driver } = createInterlockedContexts();
    await loadInitialState(executable, driver);

    const finishPersistence = deferred<undefined>();
    const finishDriverReload = deferred<SettingStateResponse[]>();
    vi.mocked(setGameExecutableOverride).mockReturnValueOnce(finishPersistence.promise);
    vi.mocked(resolveGameExecutable).mockResolvedValueOnce(effectiveExecutable(EXE_B));
    vi.mocked(listNvapiSettingStates).mockReturnValueOnce(finishDriverReload.promise);

    const selection = executable.setOverride(GAME, EXE_B);
    expect(executable.busy).toBe(true);
    await driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(setNvapiSettingValue).not.toHaveBeenCalled();

    finishPersistence.resolve(undefined);
    await vi.waitFor(() => {
      expect(executable.effectiveExe).toBe('alternate.exe');
    });
    await vi.waitFor(() => {
      expect(listNvapiSettingStates).toHaveBeenCalledTimes(2);
    });
    expect(executable.busy).toBe(true);

    await driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(setNvapiSettingValue).not.toHaveBeenCalled();

    finishDriverReload.resolve([setting(EXE_B)]);
    await expect(selection).resolves.toBe(true);
    expect(executable.busy).toBe(false);
    expect(driver.effectiveExe).toBe(EXE_B);

    await driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(setNvapiSettingValue).toHaveBeenCalledOnce();
  });

  it('blocks executable changes during an NVAPI write, then resumes', async () => {
    const { executable, driver } = createInterlockedContexts();
    await loadInitialState(executable, driver);

    const finishWrite = deferred<SettingStateResponse>();
    vi.mocked(setNvapiSettingValue).mockReturnValueOnce(finishWrite.promise);
    const write = driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(driver.busy).toBe(true);

    await expect(executable.setOverride(GAME, EXE_B)).resolves.toBe(false);
    expect(setGameExecutableOverride).not.toHaveBeenCalled();

    finishWrite.resolve(setting(EXE_A, 'quality'));
    await write;
    await expect(executable.setOverride(GAME, EXE_B)).resolves.toBe(true);
    expect(setGameExecutableOverride).toHaveBeenCalledOnce();
  });

  it('keeps the selection boundary across A to B to A when A persistence fails', async () => {
    const { executable, driver } = createInterlockedContexts();
    await loadInitialState(executable, driver);

    let rejectPersistence!: (error: unknown) => void;
    vi.mocked(setGameExecutableOverride).mockReturnValueOnce(
      new Promise<undefined>((_resolve, reject) => {
        rejectPersistence = reject;
      }),
    );
    vi.mocked(resolveGameExecutable).mockImplementation((gameId) =>
      Promise.resolve(effectiveExecutable(gameId === GAME ? EXE_A : EXE_B)),
    );

    const selection = executable.setOverride(GAME, EXE_B);
    expect(executable.busy).toBe(true);
    await executable.reload('manual:other-game');
    expect(executable.effectiveExe).toBe('alternate.exe');
    expect(executable.busy).toBe(true);
    await executable.reload(GAME);
    expect(executable.effectiveExe).toBe('game.exe');
    expect(executable.busy).toBe(true);

    await driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(setNvapiSettingValue).not.toHaveBeenCalled();

    rejectPersistence(new Error('override write failed'));
    await expect(selection).resolves.toBe(false);
    expect(executable.busy).toBe(false);
    expect(executable.effectiveExe).toBe('game.exe');

    await driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(setNvapiSettingValue).toHaveBeenCalledOnce();
  });

  it('refreshes committed A selection after returning from B before persistence settles', async () => {
    const { executable, driver } = createInterlockedContexts();
    await loadInitialState(executable, driver);

    const persistence = deferred<undefined>();
    let gameAReads = 0;
    let selectionCommitted = false;
    vi.mocked(setGameExecutableOverride).mockReturnValueOnce(persistence.promise);
    vi.mocked(resolveGameExecutable).mockImplementation((gameId) => {
      if (gameId !== GAME) {
        return Promise.resolve(effectiveExecutable(EXE_B));
      }
      gameAReads += 1;
      return Promise.resolve(effectiveExecutable(gameAReads === 1 ? EXE_A : EXE_B));
    });
    vi.mocked(listNvapiSettingStates).mockImplementation((gameId) =>
      Promise.resolve([setting(gameId === GAME ? (selectionCommitted ? EXE_B : EXE_A) : EXE_B)]),
    );

    const selection = executable.setOverride(GAME, EXE_B);
    await executable.reload('manual:other-game');
    expect(executable.effectiveExe).toBe('alternate.exe');
    await executable.reload(GAME);
    expect(executable.effectiveExe).toBe('game.exe');
    expect(executable.busy).toBe(true);

    selectionCommitted = true;
    persistence.resolve(undefined);
    await expect(selection).resolves.toBe(true);

    expect(executable.busy).toBe(false);
    expect(executable.effectiveExe).toBe('alternate.exe');
    expect(driver.effectiveExe).toBe(EXE_B);
    await driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(setNvapiSettingValue).toHaveBeenCalledOnce();
  });

  it('retains an old NVAPI write fence across A to B to A and refreshes after it commits', async () => {
    const { executable, driver } = createInterlockedContexts();
    await loadInitialState(executable, driver);

    const oldWrite = deferred<SettingStateResponse>();
    let writeCommitted = false;
    vi.mocked(setNvapiSettingValue)
      .mockReturnValueOnce(oldWrite.promise)
      .mockResolvedValue(setting(EXE_A, 'quality'));
    vi.mocked(listNvapiSettingStates).mockImplementation((gameId) =>
      Promise.resolve([
        setting(
          gameId === GAME ? EXE_A : EXE_B,
          gameId === GAME && writeCommitted ? 'quality' : 'default',
        ),
      ]),
    );

    const oldAction = driver.setValue(GAME, SETTING_KEY, 'quality');
    expect(driver.busy).toBe(true);
    await executable.reload('manual:other-game');
    await driver.reload('manual:other-game');
    await executable.reload(GAME);
    await driver.reload(GAME);
    expect(driver.settingsForFamily('sr')[0].current.wire).toBe('default');
    expect(driver.busy).toBe(true);

    await driver.setValue(GAME, SETTING_KEY, 'performance');
    await expect(executable.setOverride(GAME, EXE_B)).resolves.toBe(false);
    expect(setNvapiSettingValue).toHaveBeenCalledOnce();
    expect(setGameExecutableOverride).not.toHaveBeenCalled();

    writeCommitted = true;
    oldWrite.resolve(setting(EXE_A, 'quality'));
    await oldAction;

    expect(driver.settingsForFamily('sr')[0].effective_exe).toBe(EXE_A);
    expect(driver.settingsForFamily('sr')[0].current.wire).toBe('quality');
    expect(driver.busy).toBe(false);
    expect(driver.busy).toBe(false);

    await driver.setValue(GAME, SETTING_KEY, 'performance');
    expect(setNvapiSettingValue).toHaveBeenCalledTimes(2);
  });
});
