/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { registerPreviewInvoker, type DesktopInvoker } from '@shared/api-preview';
import type { CapturedFileSafetyContext } from './create-file-safety-context.svelte';

import FileSafetyContextTestHost from './create-file-safety-context.test-host.svelte';

describe('createFileSafetyContext', () => {
  let target: HTMLDivElement;
  let component: object | undefined;
  let disposeInvoker: (() => void) | undefined;

  beforeEach(() => {
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
    }
    disposeInvoker?.();
    document.body.replaceChildren();
  });

  it('repeats general and detected warnings after unchecked acceptance', async () => {
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return Promise.resolve({
          game_id: 'general-preference-game',
          context_token: `game-token-${calls}`,
          detected_engines: calls > 2 ? ['BattlEye'] : [],
          scan_completeness: 'complete',
        });
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: null });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'general-preference-game' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): {
        notice: { kind: string; assessment?: { detected_engines: string[] } };
      } | null;
    };
    expect(calls).toBe(0);

    const tokens = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
      expect(host.getMutationConfirmation()?.notice.kind).toBe('general');
    });
    host.resolveMutationConfirmation(true, false);
    await expect(tokens).resolves.toEqual({ gameContextToken: 'game-token-1' });
    const repeatedGeneralTokens = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(2);
      expect(host.getMutationConfirmation()?.notice.kind).toBe('general');
    });
    host.resolveMutationConfirmation(true, false);
    await expect(repeatedGeneralTokens).resolves.toEqual({ gameContextToken: 'game-token-2' });

    const detectedTokens = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(3);
      expect(host.getMutationConfirmation()?.notice.kind).toBe('detected');
      expect(host.getMutationConfirmation()?.notice.assessment?.detected_engines).toEqual([
        'BattlEye',
      ]);
    });
    host.resolveMutationConfirmation(true);
    await expect(detectedTokens).resolves.toEqual({ gameContextToken: 'game-token-3' });
    expect(host.getMutationConfirmation()).toBeNull();

    const repeatedDetectedTokens = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(4);
      expect(host.getMutationConfirmation()?.notice.kind).toBe('detected');
      expect(host.getMutationConfirmation()?.notice.assessment?.detected_engines).toEqual([
        'BattlEye',
      ]);
    });
    host.resolveMutationConfirmation(true);
    await expect(repeatedDetectedTokens).resolves.toEqual({ gameContextToken: 'game-token-4' });
  });

  it('requires one explicit confirmation for a detected anti-cheat and preserves the tokens', async () => {
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return Promise.resolve({
          game_id: 'game-a',
          context_token: `game-token-${calls}`,
          detected_engines: ['EasyAntiCheat'],
          scan_completeness: 'complete',
        });
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: null });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'game-a' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): {
        notice: { kind: string; assessment: { detected_engines: string[] } };
      } | null;
    };
    const tokens = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
      expect(host.getMutationConfirmation()?.notice.kind).toBe('detected');
      expect(host.getMutationConfirmation()?.notice.assessment.detected_engines).toEqual([
        'EasyAntiCheat',
      ]);
    });
    host.resolveMutationConfirmation(true);

    await expect(tokens).resolves.toEqual({ gameContextToken: 'game-token-1' });
    expect(host.getMutationConfirmation()).toBeNull();
  });

  it('treats a rejected anti-cheat confirmation as a skipped install', async () => {
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        return Promise.resolve({
          game_id: 'game-a',
          context_token: 'game-token',
          detected_engines: ['BattlEye'],
          scan_completeness: 'complete',
        });
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: null });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'game-a' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): unknown;
    };

    const tokens = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()).not.toBeNull();
    });
    host.resolveMutationConfirmation(false);

    await expect(tokens).resolves.toBeNull();
  });

  it('shows limited scans as repeatable general notices after cancellation and acceptance', async () => {
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return Promise.resolve({
          game_id: 'limited-game',
          context_token: `limited-token-${calls}`,
          detected_engines: [],
          scan_completeness: 'limited',
        });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'limited-game' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): { notice: { kind: string } } | null;
    };

    const canceled = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice.kind).toBe('general');
    });
    const requestCountWhilePending = calls;
    await expect(host.requireMutationTokens('game')).resolves.toBeNull();
    expect(calls).toBe(requestCountWhilePending);
    host.resolveMutationConfirmation(false);
    await expect(canceled).resolves.toBeNull();

    const accepted = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice.kind).toBe('general');
    });
    host.resolveMutationConfirmation(true, false);
    await expect(accepted).resolves.toEqual({ gameContextToken: `limited-token-${calls}` });

    const retryAfterAcceptedAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(requestCountWhilePending + 2);
      expect(host.getMutationConfirmation()?.notice.kind).toBe('general');
    });
    host.resolveMutationConfirmation(false);
    await expect(retryAfterAcceptedAction).resolves.toBeNull();
  });

  it('shows current detected engines on each fresh same-install action', async () => {
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return Promise.resolve({
          game_id: 'engine-change-game',
          context_token: `engine-change-token-${calls}`,
          detected_engines: [calls === 1 ? 'BattlEye' : 'EasyAntiCheat'],
          scan_completeness: 'complete',
        });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'engine-change-game' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): {
        notice: { assessment: { detected_engines: string[] } };
      } | null;
    };

    const first = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
      expect(host.getMutationConfirmation()?.notice.assessment.detected_engines).toEqual([
        'BattlEye',
      ]);
    });
    host.resolveMutationConfirmation(true);
    await expect(first).resolves.toEqual({ gameContextToken: 'engine-change-token-1' });

    const changed = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice.assessment.detected_engines).toEqual([
        'EasyAntiCheat',
      ]);
    });
    host.resolveMutationConfirmation(true);
    await expect(changed).resolves.toEqual({ gameContextToken: 'engine-change-token-2' });
  });

  it('discards an in-flight fresh action capture when the same game moves to another install path', async () => {
    let resolveOriginalAssessment!: (assessment: unknown) => void;
    const originalAssessment = new Promise<unknown>((resolve) => {
      resolveOriginalAssessment = resolve;
    });
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        if (calls === 1) {
          return originalAssessment;
        }
        return Promise.resolve({
          game_id: 'same-id-game',
          context_token: `install-token-${calls}`,
          detected_engines: ['EasyAntiCheat'],
          scan_completeness: 'complete',
        });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'same-id-game', initialInstallPath: '/games/old' },
    });
    const host = component as {
      replaceInstallPath(path: string): void;
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): { notice: { kind: string } } | null;
    };
    const oldAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
    });

    host.replaceInstallPath('/games/new');
    flushSync();
    await expect(oldAction).resolves.toBeNull();

    resolveOriginalAssessment({
      game_id: 'same-id-game',
      context_token: 'stale-old-install-token',
      detected_engines: [],
      scan_completeness: 'complete',
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    expect(host.getMutationConfirmation()).toBeNull();

    const newAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(2);
      expect(host.getMutationConfirmation()?.notice.kind).toBe('detected');
    });
    host.resolveMutationConfirmation(true);
    await expect(newAction).resolves.toEqual({ gameContextToken: 'install-token-2' });

    expect(host.getMutationConfirmation()).toBeNull();
  });

  it('settles a cancelled capture and ignores its later assessment failure', async () => {
    let rejectAssessment!: (error: Error) => void;
    const assessment = new Promise<never>((_resolve, reject) => {
      rejectAssessment = reject;
    });
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return assessment;
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'cancelled-capture-game' },
    });
    const host = component as {
      cancelMutationConfirmation(): void;
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      getMutationConfirmation(): unknown;
    };
    const action = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
    });

    host.cancelMutationConfirmation();
    await expect(action).resolves.toBeNull();
    rejectAssessment(new Error('late assessment failure'));
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    expect(host.getMutationConfirmation()).toBeNull();
  });

  it('retries the same installation after cancellation while old and queued captures are pending', async () => {
    let resolveOriginalAssessment!: (assessment: unknown) => void;
    const originalAssessment = new Promise<unknown>((resolve) => {
      resolveOriginalAssessment = resolve;
    });
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        if (calls === 1) {
          return originalAssessment;
        }
        return Promise.resolve({
          game_id: 'retry-game',
          context_token: `retry-token-${calls}`,
          detected_engines: [],
          scan_completeness: 'complete',
        });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'retry-game' },
    });
    const host = component as {
      cancelMutationConfirmation(): void;
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      captureFreshContext(scope: 'game'): Promise<CapturedFileSafetyContext>;
      isCurrentCapture(captured: CapturedFileSafetyContext): boolean;
      getMutationConfirmation(): unknown;
    };

    const cancelledAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
    });
    const queuedCapture = host.captureFreshContext('game');

    host.cancelMutationConfirmation();
    await expect(cancelledAction).resolves.toBeNull();
    await expect(queuedCapture).rejects.toMatchObject({ code: 'safety_context_scope_mismatch' });

    const retry = await host.captureFreshContext('game');
    expect(calls).toBe(2);
    expect(retry.tokens).toEqual({ gameContextToken: 'retry-token-2' });

    resolveOriginalAssessment({
      game_id: 'retry-game',
      context_token: 'stale-token',
      detected_engines: ['EasyAntiCheat'],
      scan_completeness: 'complete',
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    expect(host.isCurrentCapture(retry)).toBe(true);
    expect(host.getMutationConfirmation()).toBeNull();
  });

  it('does not revoke tokens when cancellation follows confirmation acceptance', async () => {
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        return Promise.resolve({
          game_id: 'accepted-game',
          context_token: 'accepted-token',
          detected_engines: ['EasyAntiCheat'],
          scan_completeness: 'complete',
        });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'accepted-game' },
    });
    const host = component as {
      cancelMutationConfirmation(): void;
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean): void;
      getMutationConfirmation(): unknown;
    };

    const acceptedMutation = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()).not.toBeNull();
    });
    host.resolveMutationConfirmation(true);
    host.cancelMutationConfirmation();

    await expect(acceptedMutation).resolves.toEqual({ gameContextToken: 'accepted-token' });
  });

  it('settles a capture when the page is destroyed even if assessment never returns', async () => {
    let calls = 0;
    const stalledAssessment = new Promise<never>(() => undefined);
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return stalledAssessment;
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'destroyed-capture-game' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
    };
    const action = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
    });

    await unmount(component);
    component = undefined;
    await expect(action).resolves.toBeNull();
  });

  it('re-prompts for detected warnings after the install path changes', async () => {
    let calls = 0;
    const invoker = ((command: string) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return Promise.resolve({
          game_id: 'installation-prompt-game',
          context_token: `install-prompt-token-${calls}`,
          detected_engines: ['EasyAntiCheat'],
          scan_completeness: 'complete',
        });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'installation-prompt-game', initialInstallPath: '/games/first' },
    });
    const host = component as {
      replaceInstallPath(path: string): void;
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): {
        notice: { assessment: { detected_engines: string[] } };
      } | null;
    };
    const oldInstallAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(1);
      expect(host.getMutationConfirmation()).not.toBeNull();
    });
    host.replaceInstallPath('/games/second');
    host.resolveMutationConfirmation(true);
    await expect(oldInstallAction).resolves.toBeNull();
    flushSync();
    expect(host.getMutationConfirmation()).toBeNull();

    const secondInstallAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice.assessment.detected_engines).toEqual([
        'EasyAntiCheat',
      ]);
    });
    host.resolveMutationConfirmation(true);
    await expect(secondInstallAction).resolves.toEqual({
      gameContextToken: 'install-prompt-token-2',
    });

    const repeatedDetectedAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(3);
      expect(host.getMutationConfirmation()?.notice.assessment.detected_engines).toEqual([
        'EasyAntiCheat',
      ]);
    });
    host.resolveMutationConfirmation(true);
    await expect(repeatedDetectedAction).resolves.toEqual({
      gameContextToken: 'install-prompt-token-3',
    });
    host.replaceInstallPath('/games/third');
    flushSync();

    const thirdInstallAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(4);
      expect(host.getMutationConfirmation()?.notice.assessment.detected_engines).toEqual([
        'EasyAntiCheat',
      ]);
    });
    host.resolveMutationConfirmation(true);
    await expect(thirdInstallAction).resolves.toEqual({
      gameContextToken: 'install-prompt-token-4',
    });
  });

  it("captures for a newly selected game without waiting for the previous game's stalled request", async () => {
    let gameACalls = 0;
    let gameBCalls = 0;
    let resolveGameB!: (assessment: {
      game_id: string;
      context_token: string;
      detected_engines: string[];
      scan_completeness: 'complete';
    }) => void;
    const gameBAssessment = new Promise<{
      game_id: string;
      context_token: string;
      detected_engines: string[];
      scan_completeness: 'complete';
    }>((resolve) => {
      resolveGameB = resolve;
    });
    const stalledGameA = new Promise<never>(() => undefined);
    const invoker = ((command: string, payload?: Record<string, unknown>) => {
      if (command === 'get_game_file_safety_assessment') {
        const gameId = (payload as { gameId?: unknown } | undefined)?.gameId;
        if (gameId === 'game-a') {
          gameACalls += 1;
          return stalledGameA;
        }
        if (gameId === 'game-b') {
          gameBCalls += 1;
          return gameBAssessment;
        }
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: null });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'game-a' },
    });
    const host = component as {
      replaceGameId(gameId: string): void;
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): { notice: { kind: string } | null } | null;
    };
    const oldAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(gameACalls).toBe(1);
    });

    host.replaceGameId('game-b');
    flushSync();
    await expect(oldAction).resolves.toBeNull();
    expect(gameBCalls).toBe(0);
    const tokens = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(gameBCalls).toBe(1);
    });

    resolveGameB({
      game_id: 'game-b',
      context_token: 'game-b-token',
      detected_engines: [],
      scan_completeness: 'complete',
    });

    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice?.kind).toBe('general');
    });
    host.resolveMutationConfirmation(true, false);
    await expect(tokens).resolves.toEqual({ gameContextToken: 'game-b-token' });
    expect(host.getMutationConfirmation()).toBeNull();
  });

  it('cancels a delayed shared safety capture when the selected game changes', async () => {
    let resolveShared!: (assessment: { context_token: string }) => void;
    const sharedAssessment = new Promise<{ context_token: string }>((resolve) => {
      resolveShared = resolve;
    });
    let resolveGameB!: (assessment: {
      game_id: string;
      context_token: string;
      detected_engines: string[];
      scan_completeness: 'complete';
    }) => void;
    const gameBAssessment = new Promise<{
      game_id: string;
      context_token: string;
      detected_engines: string[];
      scan_completeness: 'complete';
    }>((resolve) => {
      resolveGameB = resolve;
    });
    let gameACalls = 0;
    let gameBCalls = 0;
    const invoker = ((command: string, payload?: Record<string, unknown>) => {
      if (command === 'get_game_file_safety_assessment') {
        const gameId = (payload as { gameId?: unknown } | undefined)?.gameId;
        if (gameId === 'game-a') {
          gameACalls += 1;
          return Promise.resolve({
            game_id: 'game-a',
            context_token: `game-a-token-${gameACalls}`,
            detected_engines: ['EasyAntiCheat'],
            scan_completeness: 'complete',
          });
        }
        if (gameId === 'game-b') {
          gameBCalls += 1;
          return gameBAssessment;
        }
      }
      if (command === 'get_shared_vulkan_safety_assessment') {
        return sharedAssessment;
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'game-a' },
    });
    const host = component as {
      replaceGameId(gameId: string): void;
      requireMutationTokens(scope: 'game_and_shared'): Promise<unknown>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): { notice: { kind: string } | null } | null;
    };
    const tokens = host.requireMutationTokens('game_and_shared');
    await vi.waitFor(() => {
      expect(gameACalls).toBe(1);
    });
    host.replaceGameId('game-b');
    flushSync();
    await expect(tokens).resolves.toBeNull();
    resolveShared({ context_token: 'shared-token' });

    const gameBTokens = host.requireMutationTokens('game_and_shared');
    await vi.waitFor(() => {
      expect(gameBCalls).toBe(1);
    });
    resolveGameB({
      game_id: 'game-b',
      context_token: 'game-b-token',
      detected_engines: [],
      scan_completeness: 'complete',
    });
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice?.kind).toBe('general');
    });
    host.resolveMutationConfirmation(true, false);
    await expect(gameBTokens).resolves.toEqual({
      gameContextToken: 'game-b-token',
      sharedVulkanContextToken: 'shared-token',
    });
  });

  it('suppresses later limited scans without engines after saving the general opt-out and still confirms detected engines', async () => {
    let calls = 0;
    const settingWrites: unknown[] = [];
    const invoker = ((command: string, payload?: Record<string, unknown>) => {
      if (command === 'get_game_file_safety_assessment') {
        calls += 1;
        return Promise.resolve({
          game_id: 'persisted-opt-out-game',
          context_token: `persisted-token-${calls}`,
          detected_engines: calls === 3 ? ['BattlEye'] : [],
          scan_completeness: calls === 2 ? 'limited' : 'complete',
        });
      }
      if (command === 'get_catalog_setting') {
        return Promise.resolve({ value: null });
      }
      if (command === 'set_catalog_setting') {
        settingWrites.push(payload);
        return Promise.resolve({ saved: true });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    }) as DesktopInvoker;
    disposeInvoker = registerPreviewInvoker(invoker);

    component = mount(FileSafetyContextTestHost, {
      target,
      props: { initialGameId: 'persisted-opt-out-game' },
    });
    const host = component as {
      requireMutationTokens(scope: 'game'): Promise<{ gameContextToken: string } | null>;
      resolveMutationConfirmation(accepted: boolean, remember?: boolean): void;
      getMutationConfirmation(): { notice: { kind: string } | null } | null;
    };

    const generalAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(host.getMutationConfirmation()?.notice?.kind).toBe('general');
    });
    host.resolveMutationConfirmation(true, true);
    await expect(generalAction).resolves.toEqual({ gameContextToken: 'persisted-token-1' });
    await vi.waitFor(() => expect(settingWrites).toHaveLength(1));
    expect(settingWrites).toEqual([{ key: 'game_file_safety_warning_v1', value: 'true' }]);
    expect(host.getMutationConfirmation()).toBeNull();

    await expect(host.requireMutationTokens('game')).resolves.toEqual({
      gameContextToken: 'persisted-token-2',
    });
    expect(host.getMutationConfirmation()).toBeNull();

    const detectedAction = host.requireMutationTokens('game');
    await vi.waitFor(() => {
      expect(calls).toBe(3);
      expect(host.getMutationConfirmation()?.notice?.kind).toBe('detected');
    });
    host.resolveMutationConfirmation(true);
    await expect(detectedAction).resolves.toEqual({ gameContextToken: 'persisted-token-3' });
  });
});
