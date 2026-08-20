<script lang="ts">
  import GripVerticalIcon from '@lucide/svelte/icons/grip-vertical';
  import { onDestroy } from 'svelte';
  import {
    dragHandle,
    dragHandleZone,
    SHADOW_ITEM_MARKER_PROPERTY_NAME,
    SOURCES,
    TRIGGERS,
    type DndEvent,
  } from 'svelte-dnd-action';
  import { reportErrorDiagnostic } from '@shared/diagnostics';
  import { Label, Switch } from '@shared/ui';
  import { t } from '@shared/i18n';
  import { canonicalizeLauncherOrder } from '../model/launcher-order';
  import type { LauncherFilterOption } from '../model/launcher-filter-options';

  type DndLauncherItem = {
    id: string;
    launcherId: string;
    label: string;
    isDndShadowItem?: boolean;
  };

  export type ReorderSession = {
    source: 'keyboard' | 'pointer';
    draggedId: string;
    basisItems: readonly { launcherId: string; label: string }[];
    acknowledgedOrder: readonly string[];
    lastEmittedOrder?: readonly string[];
  };

  type Props = {
    options?: readonly LauncherFilterOption[];
    draftLaunchers?: readonly string[];
    draftLauncherOrder?: readonly string[];
    onLaunchersChange?: (launchers: readonly string[]) => void;
    onOrderChange?: (order: readonly string[]) => void;
    onKeyboardReorderActiveChange?: (active: boolean) => void;
  };

  const EMPTY_ARRAY = [] as const;
  const DND_ZONE_TYPE = 'renderpilot-launcher-filter';
  type RecoveryReason = 'malformed' | 'stale';

  let {
    options = EMPTY_ARRAY,
    draftLaunchers = EMPTY_ARRAY,
    draftLauncherOrder = EMPTY_ARRAY,
    onLaunchersChange,
    onOrderChange,
    onKeyboardReorderActiveChange,
  }: Props = $props();

  let zoneItems = $state.raw<DndLauncherItem[]>([]);
  let session = $state<ReorderSession | null>(null);
  let keyboardReorderActive = $state(false);

  const availableIds = $derived(options.map((option) => option.value));
  const projectedFromProps = $derived.by(() => projectItems(options, draftLauncherOrder));
  $effect(() => {
    const nextItems = projectedFromProps;
    reconcileProps(nextItems);
  });

  onDestroy(endSession);

  function projectItems(
    currentOptions: readonly LauncherFilterOption[],
    order: readonly string[],
  ): DndLauncherItem[] {
    return canonicalizeLauncherOrder(
      order,
      currentOptions.map((option) => option.value),
    ).map((launcherId) => ({
      id: launcherId,
      launcherId,
      label: currentOptions.find((option) => option.value === launcherId)?.label ?? launcherId,
    }));
  }

  function reconcileProps(nextItems: readonly DndLauncherItem[]): void {
    const nextOrder = nextItems.map((item) => item.launcherId);

    if (session === null) {
      if (!sameItems(zoneItems, nextItems)) {
        zoneItems = [...nextItems];
      }
      return;
    }

    const basisUnchanged = sameBasisItems(session.basisItems, nextItems);

    if (session.lastEmittedOrder !== undefined) {
      if (sameOrder(session.lastEmittedOrder, nextOrder) && basisUnchanged) {
        session = {
          ...session,
          acknowledgedOrder: [...nextOrder],
          lastEmittedOrder: undefined,
        };
        zoneItems = [...nextItems];
        return;
      }

      resetFromProps(nextItems, 'stale');
      return;
    }

    if (sameOrder(session.acknowledgedOrder, nextOrder) && basisUnchanged) {
      return;
    }

    resetFromProps(nextItems, 'stale');
  }

  function resetFromProps(nextItems: readonly DndLauncherItem[], reason: RecoveryReason): void {
    reportErrorDiagnostic({
      source: 'client-boundary',
      operation: 'launcher_reorder_recovery',
      code: reason === 'malformed' ? 'malformed_dnd_event' : 'stale_external_update',
      contractStatus: reason === 'malformed' ? 'malformed' : 'known',
      severity: 'warning',
    });
    endSession();
    zoneItems = [...nextItems];
  }

  function beginSession(source: 'keyboard' | 'pointer', draggedId: string): void {
    const item = zoneItems.find((candidate) => candidate.launcherId === draggedId);
    if (!item) {
      return;
    }

    session = {
      source,
      draggedId,
      basisItems: zoneItems.map(({ launcherId, label }) => ({ launcherId, label })),
      acknowledgedOrder: zoneItems.map(({ launcherId }) => launcherId),
    };

    if (source === 'keyboard') {
      setKeyboardReorderActive(true);
    }
  }

  function endSession(): void {
    session = null;
    setKeyboardReorderActive(false);
  }

  function setKeyboardReorderActive(active: boolean): void {
    if (keyboardReorderActive === active) {
      return;
    }
    keyboardReorderActive = active;
    onKeyboardReorderActiveChange?.(active);
  }

  function handleConsider(event: CustomEvent<unknown>): void {
    const detail = event.detail;
    if (!isRecord(detail)) {
      resetFromProps(projectedFromProps, 'malformed');
      return;
    }

    const rawInfo = detail.info;
    if (
      !isRecord(rawInfo) ||
      typeof rawInfo.id !== 'string' ||
      !isDndSource(rawInfo.source) ||
      typeof rawInfo.trigger !== 'string'
    ) {
      resetFromProps(projectedFromProps, 'malformed');
      return;
    }

    const dndDetail = detail as unknown as DndEvent<DndLauncherItem>;
    const info = dndDetail.info;
    const order = readEventOrder(dndDetail, info.source === SOURCES.POINTER);
    if (order === undefined) {
      resetFromProps(projectedFromProps, 'malformed');
      return;
    }

    if (info.source === SOURCES.KEYBOARD && info.trigger === TRIGGERS.DRAG_STOPPED) {
      setKeyboardReorderActive(false);
      return;
    }

    if (info.trigger === TRIGGERS.DRAG_STARTED) {
      beginSession(info.source, info.id);
    }

    const projectedItems = projectEventItems(dndDetail, order);
    if (projectedItems === undefined) {
      resetFromProps(projectedFromProps, 'malformed');
      return;
    }
    zoneItems = projectedItems;
  }

  function handleFinalize(event: CustomEvent<unknown>): void {
    const detail = event.detail;
    if (!isRecord(detail)) {
      resetFromProps(projectedFromProps, 'malformed');
      return;
    }

    const rawInfo = detail.info;
    if (
      !isRecord(rawInfo) ||
      typeof rawInfo.id !== 'string' ||
      !isDndSource(rawInfo.source) ||
      typeof rawInfo.trigger !== 'string'
    ) {
      resetFromProps(projectedFromProps, 'malformed');
      return;
    }

    const dndDetail = detail as unknown as DndEvent<DndLauncherItem>;
    const info = dndDetail.info;
    const order = readEventOrder(dndDetail, false);

    if (order === undefined) {
      resetFromProps(projectedFromProps, 'malformed');
      return;
    }

    const nextItems = projectItems(options, order);
    zoneItems = nextItems;

    if (info.source === SOURCES.KEYBOARD) {
      if (session?.source === 'keyboard' && info.trigger === TRIGGERS.DROPPED_INTO_ZONE) {
        emitKeyboardOrder(order);
      }
      return;
    }

    if (session?.source === 'pointer') {
      emitPointerOrder(order);
      return;
    }
    endSession();
  }

  function emitKeyboardOrder(order: readonly string[]): void {
    if (session?.source !== 'keyboard' || sameOrder(order, session.acknowledgedOrder)) {
      return;
    }

    const emittedOrder = Object.freeze([...order]);
    session = { ...session, lastEmittedOrder: emittedOrder };
    onOrderChange?.(emittedOrder);
  }

  function emitPointerOrder(order: readonly string[]): void {
    const activeSession = session;
    if (activeSession?.source !== 'pointer') {
      return;
    }

    const canonicalOrder = canonicalizeLauncherOrder(order, availableIds);
    const shouldEmit = !sameOrder(canonicalOrder, activeSession.acknowledgedOrder);
    endSession();
    if (shouldEmit) {
      onOrderChange?.(Object.freeze([...canonicalOrder]));
    }
  }

  function readEventOrder(
    detail: DndEvent<DndLauncherItem>,
    allowPointerShadow: boolean,
  ): string[] | undefined {
    if (!Array.isArray(detail.items)) {
      return undefined;
    }

    const rawItems = detail.items as unknown[];
    const shadowIndexes: number[] = [];
    const order: string[] = [];
    for (const [index, item] of rawItems.entries()) {
      if (isShadowItem(item)) {
        if (!isRecord(item) || typeof item.id !== 'string') {
          return undefined;
        }
        shadowIndexes.push(index);
        continue;
      }

      if (!isRecord(item) || typeof item.id !== 'string') {
        return undefined;
      }
      order.push(item.id);
    }

    if (shadowIndexes.length > (allowPointerShadow ? 1 : 0)) {
      return undefined;
    }

    if (shadowIndexes.length === 1) {
      if (detail.info.source !== SOURCES.POINTER || typeof detail.info.id !== 'string') {
        return undefined;
      }
      const draggedIndex = order.indexOf(detail.info.id);
      if (draggedIndex >= 0) {
        order.splice(draggedIndex, 1);
      }
      const shadowIndex = shadowIndexes.at(0);
      if (shadowIndex === undefined) {
        return undefined;
      }
      order.splice(shadowIndex, 0, detail.info.id);
    }

    return hasExactIds(order, availableIds) ? order : undefined;
  }

  function projectEventItems(
    detail: DndEvent<DndLauncherItem>,
    order: readonly string[],
  ): DndLauncherItem[] | undefined {
    const projected = projectItems(options, order);
    if (detail.info.source !== SOURCES.POINTER) {
      return projected;
    }

    const shadowIndex = detail.items.findIndex((item) => isShadowItem(item));
    if (shadowIndex < 0) {
      return projected;
    }

    const shadowItem: unknown = detail.items[shadowIndex];
    const replacement = projected.at(shadowIndex);
    if (!isValidPointerShadowItem(shadowItem, detail.info.id, replacement)) {
      return undefined;
    }

    projected[shadowIndex] = shadowItem;
    return projected;
  }

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === 'object' && value !== null;
  }

  function isDndSource(value: unknown): value is 'keyboard' | 'pointer' {
    return value === SOURCES.KEYBOARD || value === SOURCES.POINTER;
  }

  function isShadowItem(value: unknown): boolean {
    return isRecord(value) && value[SHADOW_ITEM_MARKER_PROPERTY_NAME] === true;
  }

  function isValidPointerShadowItem(
    value: unknown,
    expectedLauncherId: string,
    replacement: DndLauncherItem | undefined,
  ): value is DndLauncherItem {
    return (
      isRecord(value) &&
      isShadowItem(value) &&
      typeof value.id === 'string' &&
      typeof value.launcherId === 'string' &&
      typeof value.label === 'string' &&
      replacement !== undefined &&
      value.launcherId === expectedLauncherId &&
      value.launcherId === replacement.launcherId &&
      value.label === replacement.label
    );
  }

  function dndItemKey(item: DndLauncherItem): string {
    const shadowMarker = item[SHADOW_ITEM_MARKER_PROPERTY_NAME];
    return `${item.id}${shadowMarker ? `_${shadowMarker}` : ''}`;
  }

  function hasExactIds(candidate: readonly string[], expected: readonly string[]): boolean {
    if (candidate.length !== expected.length) {
      return false;
    }
    if (candidate.some((id, index) => candidate.indexOf(id) !== index)) {
      return false;
    }
    return candidate.every((id) => expected.includes(id));
  }

  function sameOrder(left: readonly string[], right: readonly string[]): boolean {
    return left.length === right.length && left.every((value, index) => value === right[index]);
  }

  function sameItems(left: readonly DndLauncherItem[], right: readonly DndLauncherItem[]): boolean {
    return (
      left.length === right.length &&
      left.every(
        (item, index) =>
          item.launcherId === right[index]?.launcherId && item.label === right[index]?.label,
      )
    );
  }

  function sameBasisItems(
    basis: readonly { launcherId: string; label: string }[],
    current: readonly DndLauncherItem[],
  ): boolean {
    if (basis.length !== current.length) {
      return false;
    }
    return basis.every((basisItem) =>
      current.some(
        (item) => item.launcherId === basisItem.launcherId && item.label === basisItem.label,
      ),
    );
  }

  function isSelected(value: string): boolean {
    return draftLaunchers.includes(value);
  }

  function handleLauncherToggle(value: string, checked: boolean): void {
    const selected = isSelected(value);
    if (selected === checked) {
      return;
    }
    onLaunchersChange?.(
      checked ? [...draftLaunchers, value] : draftLaunchers.filter((item) => item !== value),
    );
  }

  function stopRowDndCapture(event: MouseEvent | TouchEvent): void {
    event.stopPropagation();
  }

  function forwardKeyboardDragTrigger(event: KeyboardEvent): void {
    if (event.key !== ' ' && event.key !== 'Enter') {
      return;
    }

    event.preventDefault();
    const currentTarget = event.currentTarget;
    if (!(currentTarget instanceof HTMLElement)) {
      return;
    }

    currentTarget.closest('li')?.dispatchEvent(
      new KeyboardEvent('keydown', {
        bubbles: true,
        cancelable: true,
        key: event.key,
      }),
    );
  }
</script>

<section class="grid gap-3" aria-labelledby="launcher-filters-heading">
  <h3 id="launcher-filters-heading" class="text-sm font-medium">{t('filters.launchers.title')}</h3>

  {#if zoneItems.length > 0}
    <ol
      class="grid gap-3"
      aria-label={t('filters.launchers.reorder.zoneLabel')}
      use:dragHandleZone={{
        items: zoneItems,
        type: DND_ZONE_TYPE,
        dropFromOthersDisabled: true,
        flipDurationMs: 0,
        dropTargetStyle: {},
        autoAriaDisabled: false,
      }}
      onconsider={handleConsider}
      onfinalize={handleFinalize}
    >
      {#each zoneItems as item (dndItemKey(item))}
        <!-- Observed svelte-dnd-action shadow-item lifecycle contract used to synchronize behavior and tests; not a test-only hook. -->
        <li
          class="flex items-center justify-between gap-3"
          data-is-dnd-shadow-item-hint={item.isDndShadowItem ? 'true' : undefined}
          aria-label={t('filters.launchers.reorder.itemLabel', { label: item.label })}
        >
          <div class="flex min-w-0 items-center gap-2">
            <button
              use:dragHandle
              type="button"
              onkeydown={forwardKeyboardDragTrigger}
              class="inline-flex size-[24px] shrink-0 cursor-grab items-center justify-center rounded-sm text-muted-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing"
              aria-label={t('filters.launchers.reorder.move', {
                label: item.label,
                position: zoneItems.indexOf(item) + 1,
                total: zoneItems.length,
              })}
            >
              <GripVerticalIcon class="size-[20px]" aria-hidden="true" />
            </button>

            <Label
              for={`launcher-switch-${item.launcherId}`}
              class="truncate"
              onmousedowncapture={stopRowDndCapture}
              ontouchstartcapture={stopRowDndCapture}
            >
              {item.label}
            </Label>
          </div>

          <Switch
            id={`launcher-switch-${item.launcherId}`}
            checked={isSelected(item.launcherId)}
            onCheckedChange={(checked: boolean) => {
              handleLauncherToggle(item.launcherId, checked);
            }}
          />
        </li>
      {/each}
    </ol>
  {:else}
    <p class="text-sm text-muted-foreground">{t('filters.launchers.empty')}</p>
  {/if}
</section>
