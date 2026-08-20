/**
 * @vitest-environment jsdom
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

type DndActionOptions = {
  items: readonly unknown[];
  [key: string]: unknown;
};

const actionRecords = vi.hoisted(() => [] as { node: HTMLElement; options: DndActionOptions }[]);
const reportErrorDiagnostic = vi.hoisted(() => vi.fn());

vi.mock('@shared/diagnostics', () => ({ reportErrorDiagnostic }));

vi.mock('svelte-dnd-action', () => ({
  SHADOW_ITEM_MARKER_PROPERTY_NAME: 'isDndShadowItem',
  SOURCES: { POINTER: 'pointer', KEYBOARD: 'keyboard' },
  TRIGGERS: {
    DRAG_STARTED: 'dragStarted',
    DROPPED_INTO_ZONE: 'droppedIntoZone',
    DRAG_STOPPED: 'dragStopped',
  },
  dragHandleZone: (node: HTMLElement, options: DndActionOptions) => {
    const record = { node, options };
    actionRecords.push(record);
    return {
      update(nextOptions: DndActionOptions) {
        record.options = nextOptions;
      },
      destroy() {
        return;
      },
    };
  },
  dragHandle: () => ({
    update() {
      return;
    },
    destroy() {
      return;
    },
  }),
}));

import LauncherFilterSectionTestHost from './LauncherFilterSection.test-host.svelte';
import { clearAllNotifications, getActiveNotifications } from '@shared/notifications';

const OPTIONS = [
  { value: 'steam', label: 'Steam' },
  { value: 'epic', label: 'Epic Games' },
  { value: 'gog', label: 'GOG Galaxy' },
] as const;

describe('LauncherFilterSection', () => {
  let target!: HTMLDivElement;
  let component: object | undefined;

  afterEach(async () => {
    if (component !== undefined) {
      await unmount(component);
      component = undefined;
    }
    actionRecords.length = 0;
    reportErrorDiagnostic.mockClear();
    clearAllNotifications();
    target.remove();
  });

  function updateProps(props: Record<string, unknown>): void {
    if (!component) {
      throw new Error('Expected mounted launcher test host.');
    }
    const host = component as unknown as {
      updateDraftLauncherOrder?: (order: readonly string[]) => void;
      updateOptions?: (options: readonly { value: string; label: string }[]) => void;
    };
    if (props.draftLauncherOrder !== undefined) {
      host.updateDraftLauncherOrder?.(props.draftLauncherOrder as readonly string[]);
    }
    if (props.options !== undefined) {
      host.updateOptions?.(props.options as readonly { value: string; label: string }[]);
    }
    flushSync();
  }

  function render(props: Record<string, unknown> = {}) {
    target = document.createElement('div');
    document.body.append(target);
    component = mount(LauncherFilterSectionTestHost, {
      target,
      props: {
        options: OPTIONS,
        draftLauncherOrder: ['steam', 'epic', 'gog'],
        draftLaunchers: ['steam'],
        ...props,
      },
    });
    flushSync();
    return actionRecords[0].node;
  }

  function dispatch(node: HTMLElement, type: 'consider' | 'finalize', detail: unknown) {
    node.dispatchEvent(new CustomEvent(type, { detail }));
    flushSync();
  }

  function actionItems(): readonly unknown[] {
    const record = actionRecords.at(0);
    if (!record) {
      throw new Error('Expected a registered DnD zone action.');
    }
    return record.options.items;
  }

  it('keeps the zone and direct draggable item surface semantic', () => {
    const list = render();

    expect(list.getAttribute('aria-label')).toBe('Launcher reorder zone');
    expect(list.querySelectorAll(':scope > li')).toHaveLength(3);
    expect(list.querySelectorAll(':scope > li > button')).toHaveLength(3);
    expect(actionRecords[0].options).toMatchObject({
      type: 'renderpilot-launcher-filter',
      dropFromOthersDisabled: true,
      flipDurationMs: 0,
      dropTargetStyle: {},
      autoAriaDisabled: false,
    });
  });

  it('emits keyboard movement and gates Escape through the active signal', () => {
    const onOrderChange = vi.fn((order: readonly string[]) => {
      updateProps({ draftLauncherOrder: [...order] });
    });
    const onActiveChange = vi.fn();
    const list = render({ onOrderChange, onKeyboardReorderActiveChange: onActiveChange });

    const items = [
      { id: 'steam', launcherId: 'steam', label: 'Steam' },
      { id: 'epic', launcherId: 'epic', label: 'Epic Games' },
      { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
    ];
    dispatch(list, 'consider', {
      items,
      info: { id: 'steam', source: 'keyboard', trigger: 'dragStarted' },
    });
    dispatch(list, 'finalize', {
      items: [items[1], items[0], items[2]],
      info: { id: 'steam', source: 'keyboard', trigger: 'droppedIntoZone' },
    });

    expect(onActiveChange).toHaveBeenCalledWith(true);
    expect(onOrderChange).toHaveBeenCalledWith(['epic', 'steam', 'gog']);

    dispatch(list, 'consider', {
      items: [items[1], items[0], items[2]],
      info: { id: 'steam', source: 'keyboard', trigger: 'dragStopped' },
    });
    expect(onActiveChange).toHaveBeenLastCalledWith(false);
  });

  it('invalidates an older acknowledged prop after a newer keyboard emission', () => {
    let emissionCount = 0;
    const onOrderChange = vi.fn((order: readonly string[]) => {
      emissionCount += 1;
      if (emissionCount === 1) {
        updateProps({ draftLauncherOrder: [...order] });
      }
    });
    const onActiveChange = vi.fn();
    const list = render({ onOrderChange, onKeyboardReorderActiveChange: onActiveChange });
    const items = [
      { id: 'steam', launcherId: 'steam', label: 'Steam' },
      { id: 'epic', launcherId: 'epic', label: 'Epic Games' },
      { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
    ];

    dispatch(list, 'consider', {
      items,
      info: { id: 'steam', source: 'keyboard', trigger: 'dragStarted' },
    });
    dispatch(list, 'finalize', {
      items: [items[1], items[0], items[2]],
      info: { id: 'steam', source: 'keyboard', trigger: 'droppedIntoZone' },
    });
    dispatch(list, 'finalize', {
      items: [items[0], items[2], items[1]],
      info: { id: 'steam', source: 'keyboard', trigger: 'droppedIntoZone' },
    });

    expect(onOrderChange).toHaveBeenNthCalledWith(2, ['steam', 'gog', 'epic']);
    expect(reportErrorDiagnostic).toHaveBeenCalledExactlyOnceWith({
      source: 'client-boundary',
      operation: 'launcher_reorder_recovery',
      code: 'stale_external_update',
      contractStatus: 'known',
      severity: 'warning',
    });
    expect(onActiveChange).toHaveBeenLastCalledWith(false);
  });

  it('invalidates a session when an option label changes and reports no notification', () => {
    const onActiveChange = vi.fn();
    const list = render({ onKeyboardReorderActiveChange: onActiveChange });
    const items = [
      { id: 'steam', launcherId: 'steam', label: 'Steam' },
      { id: 'epic', launcherId: 'epic', label: 'Epic Games' },
      { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
    ];

    dispatch(list, 'consider', {
      items,
      info: { id: 'steam', source: 'keyboard', trigger: 'dragStarted' },
    });
    updateProps({
      options: [
        { value: 'steam', label: 'Steam updated' },
        { value: 'epic', label: 'Epic Games' },
        { value: 'gog', label: 'GOG Galaxy' },
      ],
    });

    expect(reportErrorDiagnostic).toHaveBeenCalledExactlyOnceWith({
      source: 'client-boundary',
      operation: 'launcher_reorder_recovery',
      code: 'stale_external_update',
      contractStatus: 'known',
      severity: 'warning',
    });
    expect(onActiveChange).toHaveBeenLastCalledWith(false);
    expect(getActiveNotifications()).toEqual([]);
  });

  it('rejects a malformed extra item, ends keyboard activity, and emits no order or notification', () => {
    const onOrderChange = vi.fn();
    const onActiveChange = vi.fn();
    const list = render({ onOrderChange, onKeyboardReorderActiveChange: onActiveChange });

    const items = [
      { id: 'steam', launcherId: 'steam', label: 'Steam' },
      { id: 'epic', launcherId: 'epic', label: 'Epic Games' },
      { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
    ];
    dispatch(list, 'consider', {
      items,
      info: { id: 'steam', source: 'keyboard', trigger: 'dragStarted' },
    });
    dispatch(list, 'consider', {
      items: [
        { id: 'steam', launcherId: 'steam', label: 'Steam' },
        {},
        { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
      ],
      info: { id: 'steam', source: 'keyboard', trigger: 'dragStarted' },
    });

    expect(onOrderChange).not.toHaveBeenCalled();
    expect(onActiveChange).toHaveBeenCalledWith(true);
    expect(onActiveChange).toHaveBeenLastCalledWith(false);
    expect(getActiveNotifications()).toEqual([]);
    expect(reportErrorDiagnostic).toHaveBeenCalledExactlyOnceWith({
      source: 'client-boundary',
      operation: 'launcher_reorder_recovery',
      code: 'malformed_dnd_event',
      contractStatus: 'malformed',
      severity: 'warning',
    });
  });

  it('preserves one pointer shadow identity through mutation and shadow-free finalize', () => {
    const onOrderChange = vi.fn();
    const list = render({ onOrderChange });
    const items = [
      { id: 'steam', launcherId: 'steam', label: 'Steam' },
      { id: 'epic', launcherId: 'epic', label: 'Epic Games' },
      { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
    ];
    const shadowItem = { ...items[0], id: 'dnd-shadow', isDndShadowItem: true };

    dispatch(list, 'consider', {
      items: [items[1], shadowItem, items[2]],
      info: { id: 'steam', source: 'pointer', trigger: 'dragStarted' },
    });
    expect(actionItems()[1]).toBe(shadowItem);
    expect(list.querySelectorAll('[data-is-dnd-shadow-item-hint="true"]')).toHaveLength(1);
    expect(
      Array.from(list.querySelectorAll<HTMLElement>(':scope > li')).map((item) =>
        item.getAttribute('aria-label'),
      ),
    ).toEqual(['Epic Games launcher', 'Steam launcher', 'GOG Galaxy launcher']);

    shadowItem.id = 'steam';
    dispatch(list, 'consider', {
      items: [items[1], shadowItem, items[2]],
      info: { id: 'steam', source: 'pointer', trigger: 'draggedOverIndex' },
    });
    expect(actionItems()[1]).toBe(shadowItem);
    expect(list.querySelectorAll('[data-is-dnd-shadow-item-hint="true"]')).toHaveLength(1);

    dispatch(list, 'finalize', {
      items: [items[1], items[0], items[2]],
      info: { id: 'steam', source: 'pointer', trigger: 'droppedIntoZone' },
    });

    expect(onOrderChange).toHaveBeenCalledWith(['epic', 'steam', 'gog']);
    expect(reportErrorDiagnostic).not.toHaveBeenCalled();
    expect(list.querySelectorAll('[data-is-dnd-shadow-item-hint="true"]')).toHaveLength(0);
  });

  it('recovers one malformed outer DnD detail without emitting order or notification', () => {
    const onOrderChange = vi.fn();
    const onActiveChange = vi.fn();
    const list = render({ onOrderChange, onKeyboardReorderActiveChange: onActiveChange });
    const items = [
      { id: 'steam', launcherId: 'steam', label: 'Steam' },
      { id: 'epic', launcherId: 'epic', label: 'Epic Games' },
      { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
    ];

    dispatch(list, 'consider', {
      items,
      info: { id: 'steam', source: 'keyboard', trigger: 'dragStarted' },
    });
    reportErrorDiagnostic.mockClear();
    dispatch(list, 'consider', null);

    expect(onOrderChange).not.toHaveBeenCalled();
    expect(onActiveChange).toHaveBeenCalledWith(true);
    expect(onActiveChange).toHaveBeenLastCalledWith(false);
    expect(getActiveNotifications()).toEqual([]);
    expect(reportErrorDiagnostic).toHaveBeenCalledExactlyOnceWith({
      source: 'client-boundary',
      operation: 'launcher_reorder_recovery',
      code: 'malformed_dnd_event',
      contractStatus: 'malformed',
      severity: 'warning',
    });
  });

  it('closes a pointer session before the synchronous parent echo', () => {
    const onActiveChange = vi.fn();
    const onOrderChange = vi.fn((order: readonly string[]) => {
      updateProps({ draftLauncherOrder: [...order] });
    });
    const list = render({ onOrderChange, onKeyboardReorderActiveChange: onActiveChange });
    const items = [
      { id: 'steam', launcherId: 'steam', label: 'Steam' },
      { id: 'epic', launcherId: 'epic', label: 'Epic Games' },
      { id: 'gog', launcherId: 'gog', label: 'GOG Galaxy' },
    ];

    dispatch(list, 'consider', {
      items,
      info: { id: 'steam', source: 'pointer', trigger: 'dragStarted' },
    });
    dispatch(list, 'finalize', {
      items: [items[1], items[0], items[2]],
      info: { id: 'steam', source: 'pointer', trigger: 'droppedIntoZone' },
    });

    expect(onOrderChange).toHaveBeenCalledExactlyOnceWith(['epic', 'steam', 'gog']);
    expect(onActiveChange).not.toHaveBeenCalled();
    expect(reportErrorDiagnostic).not.toHaveBeenCalled();
    expect(
      Array.from(list.querySelectorAll<HTMLElement>(':scope > li')).map((item) =>
        item.getAttribute('aria-label'),
      ),
    ).toEqual(['Epic Games launcher', 'Steam launcher', 'GOG Galaxy launcher']);
  });
});
