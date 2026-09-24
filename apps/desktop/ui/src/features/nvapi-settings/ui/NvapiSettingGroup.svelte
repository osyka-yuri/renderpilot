<script lang="ts">
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import { Alert, AlertDescription, ItemGroup, ItemSeparator } from '@shared/ui';
  import type { SettingStateResponse } from '../model/types';
  import NvapiSettingRow from './NvapiSettingRow.svelte';

  type Props = {
    /** Settings to render as rows (already filtered to one family by the caller). */
    settings: SettingStateResponse[];
    /** Family-level warnings to surface above the rows. */
    warnings: string[];
    /** Whether a given setting's row should be disabled. */
    rowDisabled: (state: SettingStateResponse) => boolean;
    onChange: (key: string, wire: string) => void;
    onRevertPredefined: (key: string) => void;
    /**
     * Restore the exact pre-RenderPilot state. Optional: the global/base profile
     * has no per-game original, so its row's restore button is disabled and this
     * never fires.
     */
    onRevertOriginal?: (key: string) => void;
  };

  const {
    settings,
    warnings,
    rowDisabled,
    onChange,
    onRevertPredefined,
    onRevertOriginal = () => undefined,
  }: Props = $props();
</script>

{#each warnings as warning (warning)}
  <Alert variant="warning" size="sm" role="note">
    <TriangleAlertIcon aria-hidden="true" />
    <AlertDescription>{warning}</AlertDescription>
  </Alert>
{/each}

<ItemGroup class="@container rounded-md border bg-muted/30">
  {#each settings as state, index (state.setting_key)}
    {#if index > 0}
      <ItemSeparator />
    {/if}
    <NvapiSettingRow
      {state}
      disabled={rowDisabled(state)}
      onChange={(wire: string) => {
        onChange(state.setting_key, wire);
      }}
      onRevertPredefined={() => {
        onRevertPredefined(state.setting_key);
      }}
      onRevertOriginal={() => {
        onRevertOriginal(state.setting_key);
      }}
    />
  {/each}
</ItemGroup>
