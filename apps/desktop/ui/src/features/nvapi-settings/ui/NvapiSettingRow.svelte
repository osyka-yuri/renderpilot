<script lang="ts">
  import Undo2Icon from '@lucide/svelte/icons/undo-2';
  import HistoryIcon from '@lucide/svelte/icons/history';
  import {
    Button,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    Tooltip,
    TooltipContent,
    TooltipTrigger,
  } from '@shared/ui';
  import { t, translateExternalMessage } from '@shared/i18n';
  import { canRestoreOriginal } from '../model/original-state';
  import { canResetToDriverDefault } from '../model/setting-actions';
  import type { SettingStateResponse } from '../model/types';

  type Props = {
    state: SettingStateResponse;
    disabled: boolean;
    onChange: (wire: string) => void;
    onRevertPredefined: () => void;
    onRevertOriginal: () => void;
  };

  const { state, disabled, onChange, onRevertPredefined, onRevertOriginal }: Props = $props();

  // Supported values first, preserving catalog order within each group.
  const orderedValues = $derived(
    state.available_values.toSorted((a, b) => {
      if (a.supported !== b.supported) {
        return a.supported ? -1 : 1;
      }
      return 0;
    }),
  );

  // Writable $derived: the control tracks the live value, but the Select
  // binding may override it momentarily. It snaps back whenever `state.current`
  // changes (e.g. after a revert), so the trigger label AND the dropdown check
  // mark always reflect the actual current value.
  let selected = $derived(state.current.wire);

  function handleChange(value: string | undefined) {
    // Ignore the echo from syncing `selected` programmatically; only act on a
    // genuine user pick of a different value.
    if (!value || disabled || value === state.current.wire) {
      return;
    }
    onChange(value);
  }

  const hasOriginal = $derived(state.original !== null);

  function translateNvapi(key: string, fallback: string): string {
    return translateExternalMessage({ key, fallback });
  }

  // Each revert button is only meaningful when it would actually change
  // something — so they enable only then, instead of being permanently active.
  // Deleting an explicit override is meaningful even when its DWORD equals the
  // driver default. Inherited values cannot be reset, regardless of value.
  const canReset = $derived(canResetToDriverDefault(state.current_is_explicit));
  // Original restore compares both explicit presence and value. A stored
  // inherited state can differ from an explicit value equal to the default.
  const canRestore = $derived(
    canRestoreOriginal(state.original, state.current, state.current_is_explicit),
  );
</script>

<Item size="sm" class="grid min-w-0 grid-cols-1 gap-3 @min-[36rem]:grid-cols-[minmax(0,1fr)_auto]">
  <ItemContent class="min-w-0">
    <ItemTitle>{translateNvapi(`nvapi.${state.setting_key}.label`, state.setting_label)}</ItemTitle>
    {#if state.description !== null || state.min_driver !== null}
      <ItemDescription>
        {#if state.description}{translateNvapi(
            `nvapi.${state.setting_key}.description`,
            state.description,
          )}{/if}
        {#if state.min_driver}
          <span class="text-muted-foreground">
            · {t('gameDetails.nvapi.requiresDriver', { version: state.min_driver })}</span
          >
        {/if}
      </ItemDescription>
    {/if}
  </ItemContent>
  <ItemActions
    class="grid w-full min-w-0 grid-cols-[minmax(0,1fr)_auto_auto] @min-[36rem]:w-auto @min-[36rem]:shrink-0"
  >
    <Select type="single" {disabled} bind:value={selected} onValueChange={handleChange}>
      <SelectTrigger size="sm" class="w-full min-w-0 @min-[36rem]:w-60">
        <span class="truncate"
          >{translateNvapi(
            `nvapi.${state.setting_key}.value.${state.current.wire}`,
            state.current.label,
          )}</span
        >
      </SelectTrigger>
      <SelectContent>
        {#each orderedValues as option (option.wire)}
          <SelectItem
            value={option.wire}
            label={translateNvapi(`nvapi.${state.setting_key}.value.${option.wire}`, option.label)}
            disabled={!option.supported}
          >
            <span class="flex w-full items-center justify-between gap-2">
              <span
                >{translateNvapi(
                  `nvapi.${state.setting_key}.value.${option.wire}`,
                  option.label,
                )}</span
              >
              {#if !option.supported}
                <span class="text-xs text-muted-foreground"
                  >{t('gameDetails.nvapi.unavailable')}</span
                >
              {/if}
            </span>
          </SelectItem>
        {/each}
      </SelectContent>
    </Select>

    <Tooltip>
      <TooltipTrigger>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon-sm"
            disabled={disabled || !canReset}
            onclick={onRevertPredefined}
            aria-label={t('gameDetails.nvapi.resetDefault')}
          >
            <Undo2Icon class="size-4" aria-hidden="true" />
          </Button>
        {/snippet}
      </TooltipTrigger>
      <TooltipContent>
        {#if canReset}
          {t('gameDetails.nvapi.resetDefault')}
        {:else if state.current_is_explicit === false}
          {t('gameDetails.nvapi.noExplicitOverride')}
        {:else}
          {t('gameDetails.nvapi.resetStateUnknown')}
        {/if}
      </TooltipContent>
    </Tooltip>

    <Tooltip>
      <TooltipTrigger>
        {#snippet child({ props })}
          <Button
            {...props}
            variant="ghost"
            size="icon-sm"
            disabled={disabled || !canRestore}
            onclick={onRevertOriginal}
            aria-label={t('gameDetails.nvapi.restoreOriginalLabel')}
          >
            <HistoryIcon class="size-4" aria-hidden="true" />
          </Button>
        {/snippet}
      </TooltipTrigger>
      <TooltipContent>
        {#if canRestore}
          {t('gameDetails.nvapi.restoreOriginal')}
        {:else if hasOriginal}
          {t('gameDetails.nvapi.alreadyOriginal')}
        {:else}
          {t('gameDetails.nvapi.noOriginal')}
        {/if}
      </TooltipContent>
    </Tooltip>
  </ItemActions>
</Item>
