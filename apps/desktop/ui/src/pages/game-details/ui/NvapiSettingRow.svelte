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
  import { t, translateKey } from '@shared/i18n';
  import type { SettingStateResponse } from '@features/nvapi-settings';

  type Props = {
    state: SettingStateResponse;
    disabled: boolean;
    onChange: (wire: string) => void;
    onRevertPredefined: () => void;
    onRevertBaseline: () => void;
  };

  const { state, disabled, onChange, onRevertPredefined, onRevertBaseline }: Props = $props();

  // Supported values first, preserving catalog order within each group.
  const orderedValues = $derived(
    [...state.available_values].sort((a, b) => {
      if (a.supported !== b.supported) return a.supported ? -1 : 1;
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
    if (!value || disabled || value === state.current.wire) return;
    onChange(value);
  }

  const hasBaseline = $derived(state.baseline !== null);

  // Each revert button is only meaningful when it would actually change
  // something — so they enable only then, instead of being permanently active.
  // "Reset to driver default" applies when an override is present (the current
  // value differs from the driver's predefined default).
  const canReset = $derived(!state.is_current_predefined);
  // "Restore pre-RenderPilot value" applies when a baseline exists and differs
  // from the current value.
  const canRestore = $derived(
    state.baseline !== null && state.baseline.dword !== state.current.dword,
  );
</script>

<Item size="sm">
  <ItemContent>
    <ItemTitle>{translateKey(`nvapi.${state.setting_key}.label`, state.setting_label)}</ItemTitle>
    {#if state.description !== null || state.min_driver !== null}
      <ItemDescription>
        {#if state.description}{translateKey(
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
  <ItemActions>
    <Select type="single" {disabled} bind:value={selected} onValueChange={handleChange}>
      <SelectTrigger size="sm" class="w-60">
        <span class="truncate"
          >{translateKey(
            `nvapi.${state.setting_key}.value.${state.current.wire}`,
            state.current.label,
          )}</span
        >
      </SelectTrigger>
      <SelectContent>
        {#each orderedValues as option (option.wire)}
          <SelectItem
            value={option.wire}
            label={translateKey(`nvapi.${state.setting_key}.value.${option.wire}`, option.label)}
            disabled={!option.supported}
          >
            <span class="flex w-full items-center justify-between gap-2">
              <span
                >{translateKey(
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
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={disabled || !canReset}
          onclick={onRevertPredefined}
          aria-label={t('gameDetails.nvapi.resetDefault')}
        >
          <Undo2Icon class="size-4" aria-hidden="true" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {canReset ? t('gameDetails.nvapi.resetDefault') : t('gameDetails.nvapi.alreadyDefault')}
      </TooltipContent>
    </Tooltip>

    <Tooltip>
      <TooltipTrigger>
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={disabled || !canRestore}
          onclick={onRevertBaseline}
          aria-label={t('gameDetails.nvapi.restoreBaselineAria')}
        >
          <HistoryIcon class="size-4" aria-hidden="true" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {#if canRestore}
          {t('gameDetails.nvapi.restoreBaseline')}
        {:else if hasBaseline}
          {t('gameDetails.nvapi.alreadyBaseline')}
        {:else}
          {t('gameDetails.nvapi.noBaseline')}
        {/if}
      </TooltipContent>
    </Tooltip>
  </ItemActions>
</Item>
