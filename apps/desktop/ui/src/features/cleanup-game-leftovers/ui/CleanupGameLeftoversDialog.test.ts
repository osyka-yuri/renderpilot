/** @vitest-environment jsdom */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick } from 'svelte';
import { t } from '@shared/i18n';
import { closeAndUnmountBitsOverlay, type MountedComponent } from '@shared/testing';
import {
  createCleanupFlow,
  type CleanupFlow,
  type CleanupFlowDeps,
} from '../model/cleanup-flow.svelte';
import type { CleanupOutcome, LeaveOutcome, LeftoverProposal } from '../model/types';
import CleanupGameLeftoversDialog from './CleanupGameLeftoversDialog.svelte';

function proposal(intent = 'intent:1'): LeftoverProposal {
  return {
    gameId: 'game:retired',
    gameName: 'Deleted game',
    installPath: 'D:/Games/Deleted',
    rootState: 'residueOnly',
    intent,
    leaveRevision: 'revision:1',
    canClean: true,
    items: [
      {
        itemId: 'file:1',
        category: 'optiscalerFile',
        path: 'D:/Games/Deleted/dxgi.dll',
        disposition: 'cleanable',
        issue: null,
      },
    ],
  };
}

describe('retired-game cleanup dialog', () => {
  let component: MountedComponent | undefined;
  let mountedFlow: CleanupFlow | undefined;
  let target: HTMLDivElement;
  let initialBodyStyle: string;
  let settlePending: () => void;

  beforeEach(() => {
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
    initialBodyStyle = document.body.style.cssText;
    settlePending = () => undefined;
  });

  afterEach(async () => {
    try {
      settlePending();
      const flow = mountedFlow;
      if (component !== undefined && flow !== undefined) {
        await vi.waitFor(() => {
          expect(flow.busy).toBe(false);
        });
        await flow.leaveAll();
        await render();
        await closeAndUnmountBitsOverlay(component, initialBodyStyle);
      }
    } finally {
      component = undefined;
      mountedFlow?.dispose();
      mountedFlow = undefined;
      vi.unstubAllGlobals();
      document.body.replaceChildren();
    }
  });

  async function show(overrides: Partial<CleanupFlowDeps> = {}): Promise<CleanupFlow> {
    const flow = createCleanupFlow({
      list: () => Promise.resolve({ proposals: [proposal()], issues: [] }),
      clean: ({ gameId }) =>
        Promise.resolve({
          gameId,
          status: 'complete',
          steps: [],
          remainingProposal: null,
          issue: null,
        }),
      leave: ({ gameId }) =>
        Promise.resolve({
          gameId,
          status: 'left',
          leaveRevision: 'revision:1',
          remainingProposal: null,
          issue: null,
        }),
      presentError: () => ({
        code: 'test_error',
        severity: 'error',
        message: 'Test error',
        suggestedActions: [],
        contractStatus: 'known',
      }),
      publishError: vi.fn(),
      publishIssue: vi.fn(),
      ...overrides,
    });
    mountedFlow = flow;
    await flow.refresh();
    component = mount(CleanupGameLeftoversDialog, { target, props: { flow } });
    await render();
    return flow;
  }

  it('starts compact and lets each game expand independently', async () => {
    const first = proposal();
    const second: LeftoverProposal = {
      ...proposal(),
      gameId: 'game:another',
      gameName: 'Another deleted game',
      installPath: 'D:/Games/Another',
      items: [
        { ...first.items[0], itemId: 'another:1', path: 'D:/Games/Another/dxgi.dll' },
        { ...first.items[0], itemId: 'another:2', path: 'D:/Games/Another/addon.ini' },
      ],
    };
    await show({ list: () => Promise.resolve({ proposals: [first, second], issues: [] }) });
    const firstTrigger = gameTrigger(first.gameName);
    const secondTrigger = gameTrigger(second.gameName);
    const firstContent = gameContent(firstTrigger);
    const secondContent = gameContent(secondTrigger);
    expect(firstTrigger.getAttribute('aria-expanded')).toBe('false');
    expect(secondTrigger.getAttribute('aria-expanded')).toBe('false');
    expect(secondTrigger.querySelector('[data-slot="badge"]')?.textContent).toBe('2');
    expect(firstContent.hidden).toBe(true);
    expect(secondContent.hidden).toBe(true);

    secondTrigger.click();
    await render();
    expect(secondTrigger.getAttribute('aria-expanded')).toBe('true');
    expect(firstTrigger.getAttribute('aria-expanded')).toBe('false');
    expect(secondContent.hidden).toBe(false);
    expect(secondContent.textContent).toContain(second.items[0].path);
    expect(firstContent.hidden).toBe(true);

    firstTrigger.click();
    await render();
    expect(firstTrigger.getAttribute('aria-expanded')).toBe('true');
    expect(secondTrigger.getAttribute('aria-expanded')).toBe('true');
    expect(firstContent.hidden).toBe(false);
    expect(firstContent.textContent).toContain(first.items[0].path);

    secondTrigger.click();
    await render();
    expect(secondTrigger.getAttribute('aria-expanded')).toBe('false');
    expect(firstTrigger.getAttribute('aria-expanded')).toBe('true');
    expect(secondContent.hidden).toBe(true);
    expect(firstContent.hidden).toBe(false);
  });

  it('keeps partial-cleanup feedback visible while its details are collapsed', async () => {
    const remaining = proposal('intent:2');
    const flow = await show({
      clean: ({ gameId }) =>
        Promise.resolve({
          gameId,
          status: 'partial',
          steps: [
            {
              itemId: 'removed:1',
              category: 'optiscalerFile',
              outcome: 'removed',
              issue: null,
            },
          ],
          remainingProposal: remaining,
          issue: { code: 'operationFailed', detail: null },
        }),
    });
    button(t('leftovers.clean')).click();
    await vi.waitFor(() => {
      expect(flow.busy).toBe(false);
    });
    await render();
    const trigger = gameTrigger(remaining.gameName);
    const content = gameContent(trigger);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(content.hidden).toBe(true);
    expect(document.body.textContent).toContain(t('leftovers.status.partial'));
    expect(document.body.textContent).toContain(t('leftovers.issue.operationFailed'));
    expect(document.body.querySelector('[role="status"]')?.closest('[hidden]')).toBeNull();

    trigger.click();
    await render();
    expect(content.hidden).toBe(false);
    expect(content.textContent).toContain(t('leftovers.outcome.removed'));
  });

  it('keeps the remaining game expanded and correctly linked after batch cleanup removes an earlier game', async () => {
    const first = proposal();
    const second: LeftoverProposal = {
      ...proposal(),
      gameId: 'game:another',
      gameName: 'Another deleted game',
      installPath: 'D:/Games/Another',
      items: [{ ...first.items[0], itemId: 'another:1', path: 'D:/Games/Another/kept.dll' }],
    };
    const remaining: LeftoverProposal = {
      ...second,
      intent: 'intent:remaining',
      canClean: false,
      items: [{ ...second.items[0], disposition: 'blocked' }],
    };
    const clean = vi.fn<CleanupFlowDeps['clean']>(({ gameId }) =>
      Promise.resolve({
        gameId,
        status: gameId === first.gameId ? 'complete' : 'partial',
        steps: [],
        remainingProposal: gameId === first.gameId ? null : remaining,
        issue: gameId === first.gameId ? null : { code: 'operationFailed', detail: null },
      }),
    );
    const flow = await show({
      list: () => Promise.resolve({ proposals: [first, second], issues: [] }),
      clean,
    });
    const firstTrigger = gameTrigger(first.gameName);
    const secondTrigger = gameTrigger(second.gameName);
    const secondContent = gameContent(secondTrigger);
    secondTrigger.click();
    await render();

    button(t('leftovers.clean')).click();
    await vi.waitFor(() => {
      expect(flow.busy).toBe(false);
    });
    await render();

    expect(clean).toHaveBeenCalledTimes(2);
    expect(clean).toHaveBeenNthCalledWith(1, { gameId: first.gameId, intent: first.intent });
    expect(clean).toHaveBeenNthCalledWith(2, { gameId: second.gameId, intent: second.intent });
    expect(firstTrigger.isConnected).toBe(false);
    expect(gameTrigger(second.gameName)).toBe(secondTrigger);
    expect(gameContent(secondTrigger)).toBe(secondContent);
    expect(secondTrigger.getAttribute('aria-expanded')).toBe('true');
    expect(secondContent.hidden).toBe(false);
    expect(secondContent.textContent).toContain(remaining.items[0].path);
    expect(document.body.querySelector('[role="status"]')?.closest('[hidden]')).toBeNull();
    expect(button(t('leftovers.clean')).disabled).toBe(true);

    secondTrigger.click();
    await render();
    expect(secondContent.hidden).toBe(true);
  });

  it('keeps the dialog open when asynchronous dismissal returns a stale proposal', async () => {
    const pending = Promise.withResolvers<LeaveOutcome>();
    const remaining = proposal('intent:2');
    const leave = vi
      .fn<CleanupFlowDeps['leave']>()
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValue({
        gameId: 'game:retired',
        status: 'left',
        leaveRevision: 'revision:1',
        remainingProposal: null,
        issue: null,
      });
    settlePending = () => {
      pending.resolve({
        gameId: 'game:retired',
        status: 'stale',
        leaveRevision: null,
        remainingProposal: remaining,
        issue: { code: 'staleIntent', detail: null },
      });
    };
    const flow = await show({ leave });
    const close = closeButton();
    expect(close).not.toBeNull();
    if (close === null) {
      throw new Error('Dialog close button not found');
    }
    close.click();
    await render();
    expect(leave).toHaveBeenCalledOnce();
    expect(openDialog()).not.toBeNull();
    settlePending();
    await vi.waitFor(() => {
      expect(flow.busy).toBe(false);
    });
    await render();
    expect(openDialog()).not.toBeNull();
    expect(document.body.textContent).toContain(t('leftovers.issue.staleIntent'));
    button(t('leftovers.leave')).click();
    await vi.waitFor(() => {
      expect(flow.proposals).toEqual([]);
    });
    expect(leave).toHaveBeenLastCalledWith({ gameId: 'game:retired', intent: 'intent:2' });
  });

  it('prevents Escape, close and duplicate actions while cleanup is in progress', async () => {
    const pending = Promise.withResolvers<CleanupOutcome>();
    const clean = vi.fn<CleanupFlowDeps['clean']>(() => pending.promise);
    const leave = vi.fn<CleanupFlowDeps['leave']>(({ gameId }) =>
      Promise.resolve({
        gameId,
        status: 'left',
        leaveRevision: 'revision:1',
        remainingProposal: null,
        issue: null,
      }),
    );
    settlePending = () => {
      pending.resolve({
        gameId: 'game:retired',
        status: 'complete',
        steps: [],
        remainingProposal: null,
        issue: null,
      });
    };
    const flow = await show({ clean, leave });
    button(t('leftovers.clean')).click();
    await render();
    expect(button(t('leftovers.leave')).disabled).toBe(true);
    expect(button(t('leftovers.cleaning')).disabled).toBe(true);
    expect(closeButton()).toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    button(t('leftovers.cleaning')).click();
    await render();
    expect(openDialog()).not.toBeNull();
    expect(clean).toHaveBeenCalledOnce();
    expect(leave).not.toHaveBeenCalled();
    settlePending();
    await vi.waitFor(() => {
      expect(flow.busy).toBe(false);
    });
    await render();
    expect(openDialog()).toBeNull();
  });

  it('shows blocked custody without a cleanup action or raw diagnostic text', async () => {
    const blocked: LeftoverProposal = {
      ...proposal(),
      canClean: false,
      items: [
        {
          itemId: 'custody',
          category: 'privateCustody',
          path: 'D:/Games/Deleted/backup.dll',
          disposition: 'blocked',
          issue: {
            code: 'privateOriginalCustody',
            detail: 'Internal diagnostic must not be displayed',
          },
        },
      ],
    };
    await show({ list: () => Promise.resolve({ proposals: [blocked], issues: [] }) });
    expect(button(t('leftovers.clean')).disabled).toBe(true);
    gameTrigger(blocked.gameName).click();
    await render();
    expect(document.body.textContent).toContain(t('leftovers.category.privateCustody'));
    expect(document.body.textContent).toContain(t('leftovers.issue.privateOriginalCustody'));
    expect(document.body.textContent).not.toContain('Internal diagnostic must not be displayed');
  });
});

function gameTrigger(gameName: string): HTMLButtonElement {
  const found = [
    ...document.body.querySelectorAll<HTMLButtonElement>('[data-slot="accordion-trigger"]'),
  ].find((candidate) => candidate.textContent.trim().startsWith(gameName));
  if (found === undefined) {
    throw new Error(`Game accordion trigger not found: ${gameName}`);
  }
  return found;
}

function gameContent(trigger: HTMLButtonElement): HTMLElement {
  const content = document.getElementById(trigger.getAttribute('aria-controls') ?? '');
  if (content === null) {
    throw new Error('Game accordion content not found');
  }
  return content;
}

function button(label: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
    (candidate) => candidate.textContent.trim() === label,
  );
  if (found === undefined) {
    throw new Error(`Button not found: ${label}`);
  }
  return found;
}

function openDialog(): Element | null {
  return document.body.querySelector('[role="dialog"][data-state="open"]');
}

function closeButton(): HTMLButtonElement | null {
  return document.body.querySelector<HTMLButtonElement>(
    '[data-slot="dialog-content"] button:has(span.sr-only)',
  );
}

async function render(): Promise<void> {
  flushSync();
  await tick();
}
